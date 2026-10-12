import { api } from '../api/tauriBridge';
import type {
  ConversationItem,
  ConversationOutline,
  MessageItem,
  OutlineSegment,
} from '../types';
import { getServiceLlmConfig } from './serviceLlm';
import {
  OUTLINE_MIN_USER_MESSAGES,
  isOpenEndedSourceApp,
  sessionKindOf,
} from './sessionKind';

export { OUTLINE_MIN_USER_MESSAGES, isOpenEndedSourceApp, sessionKindOf };

function clip(text: string, max: number): string {
  const t = text.trim().replace(/\s+/g, ' ');
  const chars = [...t];
  if (chars.length <= max) return t;
  return chars.slice(0, max).join('') + '…';
}

/** 超长用户提问：前 head + 后 tail，总长不超过 head+tail */
function clipUserPrompt(text: string, head = 300, tail = 300): string {
  const t = text.trim().replace(/\s+/g, ' ');
  const chars = [...t];
  const max = head + tail;
  if (chars.length <= max) return t;
  return `${chars.slice(0, head).join('')}…${chars.slice(-tail).join('')}`;
}

function charLen(s: string): number {
  return [...s].length;
}

function joinLines(lines: string[]): string {
  return lines.join('\n');
}

/** 从前往后丢掉更早的消息，直到总长 ≤ maxChars；必要时硬截断单行 */
function trimFrontToMax(lines: string[], maxChars: number): string[] {
  const result = [...lines];
  while (result.length > 1 && charLen(joinLines(result)) > maxChars) {
    result.shift();
  }
  if (result.length === 1 && charLen(result[0]) > maxChars) {
    return [clip(result[0], maxChars)];
  }
  while (result.length > 0 && charLen(joinLines(result)) > maxChars) {
    result.shift();
  }
  return result;
}

const MAX_TRANSCRIPT_CHARS = 15000;
const MAX_OUTLINE_USER_CHARS = 14000;

/**
 * 以用户提问为主；助手正文极度压缩，仅作辅证。
 * 整体超过 15000 字时：先去掉助手行，仍超出则截断更早的消息。
 */
function buildTranscript(
  messages: MessageItem[],
  maxChars = MAX_TRANSCRIPT_CHARS
): string {
  const entries: Array<{ role: 'user' | 'assistant'; line: string }> = [];

  for (const m of messages) {
    const body = (m.text || '').trim();
    if (!body) continue;

    if (m.sender === 'user') {
      entries.push({ role: 'user', line: `[用户] ${clipUserPrompt(body, 300, 300)}` });
    } else if (m.sender === 'assistant') {
      entries.push({ role: 'assistant', line: `[助手] ${clip(body, 80)}` });
    }
  }

  if (entries.length === 0) return '';

  let lines = entries.map((e) => e.line);
  if (charLen(joinLines(lines)) <= maxChars) {
    return joinLines(lines);
  }

  // 第一轮压缩：删掉全部助手消息
  lines = entries.filter((e) => e.role === 'user').map((e) => e.line);
  if (lines.length === 0) {
    lines = trimFrontToMax(
      entries.map((e) => e.line),
      maxChars
    );
    return joinLines(lines);
  }
  if (charLen(joinLines(lines)) <= maxChars) {
    return joinLines(lines);
  }

  return joinLines(trimFrontToMax(lines, maxChars));
}

function parseJsonObject(raw: string): { title?: string; summary?: string } | null {
  const text = raw.trim();
  const tryParse = (s: string) => {
    try {
      return JSON.parse(s) as { title?: string; summary?: string };
    } catch {
      return null;
    }
  };
  const direct = tryParse(text);
  if (direct) return direct;
  const fenced = text.match(/```(?:json)?\s*([\s\S]*?)```/i);
  if (fenced?.[1]) {
    const parsed = tryParse(fenced[1].trim());
    if (parsed) return parsed;
  }
  const brace = text.match(/\{[\s\S]*\}/);
  if (brace?.[0]) return tryParse(brace[0]);
  return null;
}

export async function renameConversationTitle(
  conversationId: string,
  aiTitle: string
): Promise<ConversationItem> {
  return api.updateConversationAiTitle(conversationId, aiTitle);
}

export async function clearConversationAiTitle(
  conversationId: string
): Promise<ConversationItem> {
  return api.clearConversationAiTitle(conversationId);
}

/** 调用 LLM 生成短标题 + 摘要，并写入 conversation_ai */
export async function summarizeConversation(params: {
  conversation: ConversationItem;
  messages: MessageItem[];
}): Promise<ConversationItem> {
  const { conversation, messages } = params;
  const cfg = getServiceLlmConfig();
  if (!cfg) {
    throw new Error('未配置 AI，请先在设置中填写 API Key');
  }

  const transcript = buildTranscript(messages);
  if (!transcript.trim()) {
    throw new Error('会话暂无可总结的文本内容');
  }

  await api.setConversationAiStatus(conversation.id, 'pending');

  const openEnded = sessionKindOf(conversation) === 'open_ended';
  const system = openEnded
    ? `你是会话归档助手。这是一场长期无界会话（标题以 bot/固定名为准，不要用本结果覆盖主标题）。
输入以「用户提问」为主，助手回复仅有极短片段作辅证。
请据此生成：
1) title：可留空字符串，或给「最近主题」短标签（8-16 字），不要当整场会话主标题
2) summary：中文摘要，120-280 字，概括近期用户诉求与推进脉络；不要臆造细节
只输出 JSON：{"title":"...","summary":"..."}`
    : `你是会话归档助手。输入以「用户提问」为主，助手回复仅有极短片段作辅证。
请据此生成：
1) title：简短中文标题，8-24 字，不要标点堆砌，不要用引号；优先概括用户目标
2) summary：中文摘要，120-280 字，围绕用户诉求与推进脉络；助手片段不足时不要臆造细节
只输出 JSON：{"title":"...","summary":"..."}`;

  const user = `源标题：${conversation.source_title || conversation.title || '未命名'}
来源：${conversation.source_app}
工作区：${conversation.workspace_path}

对话摘录（用户为主，助手已极度压缩）：
${transcript}`;

  try {
    const result = await api.callLlmWithFallback(
      cfg.primary,
      cfg.fallback,
      [
        { role: 'system', content: system },
        { role: 'user', content: user },
      ],
      900,
      true,
      'conversation_summary'
    );

    if (!result.success || !result.content?.trim()) {
      const err = result.error || 'AI 返回为空';
      await api.saveConversationAiSummary({
        conversationId: conversation.id,
        summary: conversation.ai_summary || '',
        status: 'error',
        error: err,
        model: result.provider_used,
      });
      throw new Error(err);
    }

    const parsed = parseJsonObject(result.content);
    const title = parsed?.title?.trim() || '';
    const summary = parsed?.summary?.trim() || result.content.trim();
    if (!summary) {
      throw new Error('未能解析出有效摘要');
    }

    return await api.saveConversationAiSummary({
      conversationId: conversation.id,
      // 无界会话不写 ai_title，避免盖住 bot 名主标题
      aiTitle: openEnded ? null : title || null,
      summary,
      status: 'ok',
      basedOnContentHash: conversation.content_hash || null,
      basedOnMessageCount: conversation.message_count ?? null,
      model: result.provider_used,
      error: null,
    });
  } catch (e) {
    const msg = e instanceof Error ? e.message : String(e);
    try {
      await api.setConversationAiStatus(conversation.id, 'error', msg);
    } catch {
      /* ignore */
    }
    throw e;
  }
}

// —— 会话目录（TOC）——

export interface UserPromptEntry {
  /** 1-based index in user-only sequence */
  userIndex: number;
  stepIndex: number;
  text: string;
}

export function collectUserPrompts(messages: MessageItem[]): UserPromptEntry[] {
  const out: UserPromptEntry[] = [];
  for (const m of messages) {
    if (m.sender !== 'user') continue;
    const body = (m.text || '').trim();
    if (!body) continue;
    out.push({
      userIndex: out.length + 1,
      stepIndex: typeof m.step_index === 'number' ? m.step_index : out.length,
      text: body,
    });
  }
  return out;
}

function buildUserPromptLines(
  entries: UserPromptEntry[],
  fromUserIndex = 1,
  maxChars = MAX_OUTLINE_USER_CHARS
): string {
  const lines: string[] = [];
  for (const e of entries) {
    if (e.userIndex < fromUserIndex) continue;
    lines.push(`#${e.userIndex} [step ${e.stepIndex}] ${clipUserPrompt(e.text, 300, 200)}`);
  }
  if (lines.length === 0) return '';
  if (charLen(joinLines(lines)) <= maxChars) return joinLines(lines);
  return joinLines(trimFrontToMax(lines, maxChars));
}

function parseOutlineJson(raw: string): OutlineSegment[] | null {
  const text = raw.trim();
  const tryParse = (s: string): { segments?: unknown } | null => {
    try {
      return JSON.parse(s) as { segments?: unknown };
    } catch {
      return null;
    }
  };
  let parsed = tryParse(text);
  if (!parsed) {
    const fenced = text.match(/```(?:json)?\s*([\s\S]*?)```/i);
    if (fenced?.[1]) parsed = tryParse(fenced[1].trim());
  }
  if (!parsed) {
    const brace = text.match(/\{[\s\S]*\}/);
    if (brace?.[0]) parsed = tryParse(brace[0]);
  }
  if (!parsed || !Array.isArray(parsed.segments)) return null;

  const segments: OutlineSegment[] = [];
  for (const item of parsed.segments) {
    if (!item || typeof item !== 'object') continue;
    const rec = item as Record<string, unknown>;
    const title = typeof rec.title === 'string' ? rec.title.trim() : '';
    if (!title) continue;
    const startUser =
      typeof rec.start_user_index === 'number'
        ? rec.start_user_index
        : typeof rec.startUserIndex === 'number'
          ? rec.startUserIndex
          : NaN;
    if (!Number.isFinite(startUser) || startUser < 1) continue;
    const summary =
      typeof rec.summary === 'string' && rec.summary.trim()
        ? rec.summary.trim()
        : undefined;
    segments.push({
      title,
      summary,
      start_user_index: Math.floor(startUser),
      start_step_index: 0,
    });
  }
  if (segments.length === 0) return null;
  segments.sort((a, b) => a.start_user_index - b.start_user_index);
  return segments;
}

function attachStepIndices(
  segments: OutlineSegment[],
  entries: UserPromptEntry[]
): OutlineSegment[] {
  const byIndex = new Map(entries.map((e) => [e.userIndex, e]));
  return segments.map((seg) => {
    const hit = byIndex.get(seg.start_user_index);
    return {
      ...seg,
      start_step_index: hit ? hit.stepIndex : seg.start_step_index,
    };
  });
}

export function parseConversationOutline(
  json: string | null | undefined
): ConversationOutline | null {
  if (!json?.trim()) return null;
  try {
    const data = JSON.parse(json) as ConversationOutline;
    if (!data || !Array.isArray(data.segments) || data.segments.length === 0) return null;
    return data;
  } catch {
    return null;
  }
}

/** 生成或增量更新会话目录（仅用户提问） */
export async function generateConversationOutline(params: {
  conversation: ConversationItem;
  messages: MessageItem[];
  /** 强制全量重算（忽略已有目录） */
  forceFull?: boolean;
}): Promise<ConversationItem> {
  const { conversation, messages, forceFull } = params;
  const cfg = getServiceLlmConfig();
  if (!cfg) {
    throw new Error('未配置 AI，请先在设置中填写 API Key');
  }

  const entries = collectUserPrompts(messages);
  if (entries.length < OUTLINE_MIN_USER_MESSAGES) {
    throw new Error(`用户提问不足 ${OUTLINE_MIN_USER_MESSAGES} 条，暂不生成目录`);
  }

  const existing = parseConversationOutline(conversation.ai_outline_json);
  const openEnded = sessionKindOf(conversation) === 'open_ended';
  const lastStart = existing?.segments.length
    ? existing.segments[existing.segments.length - 1].start_user_index
    : 0;
  const hasNewBeyondLast =
    !!existing && entries.length > lastStart && lastStart > 0;
  const canIncremental =
    !forceFull && openEnded && !!existing && existing.segments.length > 0 && hasNewBeyondLast;

  let system: string;
  let user: string;
  let mergeFromUserIndex = 1;

  if (canIncremental && existing) {
    const lastSeg = existing.segments[existing.segments.length - 1];
    mergeFromUserIndex = Math.max(1, lastSeg.start_user_index);
    const kept = existing.segments.filter((s) => s.start_user_index < mergeFromUserIndex);
    const promptBlock = buildUserPromptLines(entries, mergeFromUserIndex);
    if (!promptBlock.trim()) {
      throw new Error('没有新增的用户提问可更新目录');
    }
    system = `你是会话目录助手。这是一场长期无界会话的增量更新。
已有更早段落请保留不动；你只需为「从某编号起的用户提问」划主题段。
规则：
1) 仅依据用户提问；短回复如「继续」「改一下」「好的」并入上一段
2) 输出 1～8 段，覆盖从给定起点到结尾的全部用户提问
3) 每段：title（中文 4-16 字）、summary（一句）、start_user_index（该段第一条用户提问编号）
4) segments 按 start_user_index 升序；第一段的 start_user_index 必须等于或接近起点编号
只输出 JSON：{"segments":[{"title":"...","summary":"...","start_user_index":N},...]}`;

    user = `会话：${conversation.source_title || conversation.title || '未命名'}
来源：${conversation.source_app}（长期会话）
已保留的更早目录（勿重复输出）：
${
  kept.length
    ? kept.map((s) => `- #${s.start_user_index} ${s.title}`).join('\n')
    : '（无）'
}

请为以下用户提问生成目录段落（起点 #${mergeFromUserIndex}）：
${promptBlock}`;
  } else {
    const promptBlock = buildUserPromptLines(entries);
    if (!promptBlock.trim()) {
      throw new Error('会话暂无可生成目录的用户提问');
    }
    system = `你是会话目录助手。输入是按时间排序、带编号的用户提问（不含助手回复）。
请划出 3～12 个主题段落，作为会话目录。
规则：
1) 仅依据用户提问概括主题；不要臆造助手做了什么
2) 短回复如「继续」「改一下」「好的」并入上一段，不要单独成段
3) 每段：title（中文短标题 4-16 字）、summary（一句中文）、start_user_index（该段第一条用户提问的编号，从 1 起）
4) segments 必须按 start_user_index 升序，且第一段从 1 起（或接近 1）
只输出 JSON：{"segments":[{"title":"...","summary":"...","start_user_index":1},...]}`;

    user = `源标题：${conversation.source_title || conversation.title || '未命名'}
来源：${conversation.source_app}
用户提问数：${entries.length}

用户提问列表：
${promptBlock}`;
  }

  const result = await api.callLlmWithFallback(
    cfg.primary,
    cfg.fallback,
    [
      { role: 'system', content: system },
      { role: 'user', content: user },
    ],
    1200,
    true,
    'conversation_outline'
  );

  if (!result.success || !result.content?.trim()) {
    throw new Error(result.error || 'AI 返回为空');
  }

  const parsed = parseOutlineJson(result.content);
  if (!parsed) {
    throw new Error('未能解析出有效目录');
  }

  let segments: OutlineSegment[];
  if (canIncremental && existing) {
    const kept = existing.segments.filter((s) => s.start_user_index < mergeFromUserIndex);
    const refreshed = attachStepIndices(parsed, entries).filter(
      (s) => s.start_user_index >= mergeFromUserIndex
    );
    segments = [...kept, ...refreshed];
  } else {
    segments = attachStepIndices(parsed, entries);
  }

  segments = segments
    .filter((s) => s.start_user_index >= 1 && s.start_user_index <= entries.length)
    .map((s) => {
      const hit = entries.find((e) => e.userIndex === s.start_user_index);
      return hit ? { ...s, start_step_index: hit.stepIndex } : s;
    });

  if (segments.length === 0) {
    throw new Error('目录段落为空');
  }

  const seen = new Set<number>();
  segments = segments.filter((s) => {
    if (seen.has(s.start_user_index)) return false;
    seen.add(s.start_user_index);
    return true;
  });

  const outline: ConversationOutline = { segments };
  return await api.saveConversationOutline({
    conversationId: conversation.id,
    outlineJson: JSON.stringify(outline),
    basedOnContentHash: conversation.content_hash || null,
    basedOnUserCount: entries.length,
    model: result.provider_used,
  });
}
