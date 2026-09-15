import { api } from '../api/tauriBridge';
import type { ConversationItem, MessageItem } from '../types';
import { getServiceLlmConfig } from './serviceLlm';

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
    // 极端情况只有助手：仍截断前面
    lines = trimFrontToMax(
      entries.map((e) => e.line),
      maxChars
    );
    return joinLines(lines);
  }
  if (charLen(joinLines(lines)) <= maxChars) {
    return joinLines(lines);
  }

  // 第二轮压缩：截断前面（保留更晚的用户提问）
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

  const system = `你是会话归档助手。输入以「用户提问」为主，助手回复仅有极短片段作辅证。
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
      aiTitle: title || null,
      summary,
      status: 'ok',
      basedOnContentHash: conversation.content_hash || null,
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
