import { api } from '../api/tauriBridge';
import type {
  WorkspaceDetailStats,
  WorkspaceFineBlock,
  WorkspaceModuleBlock,
  AnalysisUserMessage,
} from '../types';
import { AI_PROVIDERS } from '../components/settings/SettingsModal';

export interface PipelineProgress {
  stage: 'collect' | 'batch' | 'merge' | 'report' | 'done' | 'error';
  detail: string;
  current?: number;
  total?: number;
  fineBlocksCount?: number;
}

const BATCH_SIZE = 40;

/** 智能截断超长消息：保留前 300 字符 + 后 200 字符（上限 500 字符） */
function truncateUserMessage(rawContent: string, headLen = 300, tailLen = 200): string {
  const text = (rawContent || '').replace(/\s+/g, ' ').trim();
  const maxLimit = headLen + tailLen;
  if (text.length <= maxLimit) {
    return text;
  }
  const head = text.substring(0, headLen).trim();
  const tail = text.substring(text.length - tailLen).trim();
  return `${head} …[省略超长中间内容]… ${tail}`;
}

/** 获取当前配置的主力与备用 AI 节点 */
export function getAiEndpoints() {
  const activeProviderId = localStorage.getItem('agentdeck_active_ai_provider') || 'bailian';
  const fallbackProviderId = localStorage.getItem('agentdeck_fallback_ai_provider') || 'deepseek';
  const enableFallback = localStorage.getItem('agentdeck_enable_fallback') === 'true';

  const apiKeys = JSON.parse(localStorage.getItem('agentdeck_ai_api_keys') || '{}');
  const baseUrls = JSON.parse(localStorage.getItem('agentdeck_ai_base_urls') || '{}');
  const customModels = JSON.parse(localStorage.getItem('agentdeck_ai_models') || '{}');

  const getProviderConfig = (providerId: string) => {
    const defaultMeta = AI_PROVIDERS.find((p) => p.id === providerId);
    const baseUrl = baseUrls[providerId] || defaultMeta?.baseUrl || '';
    const apiKey = apiKeys[providerId] || '';
    const model = customModels[providerId] || defaultMeta?.defaultModel || '';
    const providerName = defaultMeta?.name || providerId;
    return { provider_name: providerName, base_url: baseUrl, api_key: apiKey, model };
  };

  const primary = getProviderConfig(activeProviderId);
  const fallback = enableFallback ? getProviderConfig(fallbackProviderId) : undefined;

  return {
    hasKey: Boolean(primary.api_key),
    primary,
    fallback,
  };
}

/** 智能从 LLM 回复中提取并解析 JSON（支持去除 think 标签、处理 markdown 代码块、直接数组等） */
function safeExtractJson<T>(content: string): T | null {
  if (!content) return null;
  let text = content.trim();

  // 1. 去除 <think>...</think> 思考链
  text = text.replace(/<think>[\s\S]*?<\/think>/gi, '').trim();

  // 2. 去除 markdown ```json ... ``` 包裹
  const codeBlockMatch = text.match(/```(?:json)?\s*([\s\S]*?)\s*```/i);
  if (codeBlockMatch && codeBlockMatch[1]) {
    text = codeBlockMatch[1].trim();
  }

  // 3. 直接尝试 JSON.parse
  try {
    return JSON.parse(text) as T;
  } catch {}

  // 4. 尝试寻找最外层的 { ... } 或 [ ... ]
  const firstBrace = text.indexOf('{');
  const lastBrace = text.lastIndexOf('}');
  const firstBracket = text.indexOf('[');
  const lastBracket = text.lastIndexOf(']');

  // 如果包含数组且在中括号范围内
  if (firstBracket !== -1 && lastBracket !== -1 && (firstBrace === -1 || firstBracket < firstBrace)) {
    try {
      const slice = text.substring(firstBracket, lastBracket + 1);
      return JSON.parse(slice) as T;
    } catch {}
  }

  // 如果包含对象
  if (firstBrace !== -1 && lastBrace !== -1) {
    try {
      const slice = text.substring(firstBrace, lastBrace + 1);
      return JSON.parse(slice) as T;
    } catch {}
  }

  return null;
}

/** 规范化单条细粒度 Block */
function normalizeFineBlock(
  raw: any,
  batchIndex: number,
  seq: number
): WorkspaceFineBlock {
  const typeMap: Record<string, string> = {
    module: 'module',
    feature: 'feature',
    refactor: 'refactor',
    bugfix: 'bugfix',
    fix: 'bugfix',
  };
  const rawType = String(raw?.type || 'feature').toLowerCase();
  const blockType = typeMap[rawType] || 'feature';

  const keywords = Array.isArray(raw?.keywords)
    ? raw.keywords.map((k: any) => String(k).substring(0, 30))
    : [];

  return {
    id: batchIndex * 1000 + seq + 1,
    block_id: raw?.id || `fine-${batchIndex + 1}-${seq + 1}`,
    batch_index: batchIndex,
    type: blockType,
    title: String(raw?.title || `功能点 ${seq + 1}`).substring(0, 100),
    summary: String(raw?.summary || '').substring(0, 600),
    start_date: raw?.start_date ? String(raw.start_date).substring(0, 10) : undefined,
    end_date: raw?.end_date ? String(raw.end_date).substring(0, 10) : undefined,
    status: raw?.status || 'completed',
    keywords,
    evidence: Array.isArray(raw?.evidence)
      ? raw.evidence.slice(0, 3).map((e: any) => ({
          date: e?.date ? String(e.date).substring(0, 10) : undefined,
          conversation_title: e?.conversation_title ? String(e.conversation_title).substring(0, 80) : undefined,
          snippet: e?.snippet ? String(e.snippet).substring(0, 150) : undefined,
        }))
      : undefined,
  };
}

/** 规范化单条合并后的模块 Block */
function normalizeModuleBlock(raw: any, seq: number): WorkspaceModuleBlock {
  const keywords = Array.isArray(raw?.keywords)
    ? raw.keywords.map((k: any) => String(k).substring(0, 30))
    : [];
  const childFineIds = Array.isArray(raw?.child_fine_ids)
    ? raw.child_fine_ids.map((id: any) => String(id).substring(0, 80))
    : [];

  return {
    id: seq + 1,
    module_id: raw?.id || `mod-${seq + 1}`,
    type: raw?.type || 'module',
    title: String(raw?.title || `核心模块 ${seq + 1}`).substring(0, 100),
    summary: String(raw?.summary || '').substring(0, 800),
    start_date: raw?.start_date ? String(raw.start_date).substring(0, 10) : undefined,
    end_date: raw?.end_date ? String(raw.end_date).substring(0, 10) : undefined,
    status: raw?.status || 'completed',
    keywords,
    child_fine_ids: childFineIds,
  };
}

/** Pipeline 阶段 1：分批提取细粒度 Blocks 并入库 */
export async function runExtractFineBlocksPipeline(
  workspacePath: string,
  force: boolean,
  onProgress?: (p: PipelineProgress) => void
): Promise<{ success: boolean; fineBlocks: WorkspaceFineBlock[]; message: string }> {
  const endpoints = getAiEndpoints();
  if (!endpoints.hasKey) {
    return {
      success: false,
      fineBlocks: [],
      message: '请先在右上角「设置」中配置 AI 供应商及 API Key',
    };
  }

  onProgress?.({ stage: 'collect', detail: '正在读取工作区历史用户对话与交互记录…' });
  const messages = await api.getWorkspaceAnalysisMessages(workspacePath);

  if (!messages || messages.length === 0) {
    return {
      success: false,
      fineBlocks: [],
      message: '该工作区暂无用户对话消息，无法进行 Blocks 智能提取',
    };
  }

  // 分批切片
  const batches: AnalysisUserMessage[][] = [];
  for (let i = 0; i < messages.length; i += BATCH_SIZE) {
    batches.push(messages.slice(i, i + BATCH_SIZE));
  }

  const totalBatches = batches.length;
  onProgress?.({
    stage: 'batch',
    detail: `共找到 ${messages.length} 条用户消息，分 ${totalBatches} 批进行局部 Blocks 提炼…`,
    current: 0,
    total: totalBatches,
    fineBlocksCount: 0,
  });

  const extractedBlocks: WorkspaceFineBlock[] = [];
  let lastError = '';

  for (let batchIdx = 0; batchIdx < totalBatches; batchIdx++) {
    const currentBatch = batches[batchIdx];
    const batchNo = batchIdx + 1;

    // 格式化当前批次用户消息（超长智能截断：前300 + 后200，总上限500）
    const formattedMsgs = currentBatch.map((m) => ({
      date: m.created_at ? m.created_at.substring(0, 10) : '',
      conversation_title: (m.conversation_title || '').substring(0, 80),
      text: truncateUserMessage(m.content || ''),
    }));

    const prompt = `你是资深研发过程分析助手。请从以下「第 ${batchNo}/${totalBatches} 批」用户历史提问消息中，提炼出局部的研发功能点与需求 Blocks（仅覆盖本批消息，不要臆测全项目）。

【提取要求】：
1. 提炼 2～8 个独立的功能点/需求/重构/修复 block。
2. type 可选: "feature"(新功能/需求), "refactor"(架构与代码重构), "bugfix"(问题修复), "module"(独立模块)。
3. 同一主题合并为一个 block，不要碎片化。
4. start_date / end_date 取本批相关消息的时间（YYYY-MM-DD），没有填 null。
5. 必须仅输出标准 JSON 格式，不要任何 markdown 解释。

【输出 JSON 结构】：
{
  "blocks": [
    {
      "id": "blk-${batchNo}-1",
      "type": "feature",
      "title": "简明功能名称（中文，20字内）",
      "summary": "详细说明做了什么、解决什么问题（100字内）",
      "start_date": "YYYY-MM-DD",
      "end_date": "YYYY-MM-DD",
      "status": "completed",
      "keywords": ["关键词1", "关键词2"]
    }
  ]
}

【当前批次用户消息数据】：
${JSON.stringify(formattedMsgs, null, 2)}`;

    const days = currentBatch.map(m => m.created_at?.substring(0, 10)).filter(Boolean) as string[];
    const dateRange = days.length > 0 ? [days[0], days[days.length - 1]] : ['—', '—'];

    onProgress?.({
      stage: 'batch',
      detail: `正在处理第 ${batchNo}/${totalBatches} 批 (${dateRange[0]} ~ ${dateRange[1]}, ${currentBatch.length} 条用户提问)…`,
      current: batchNo,
      total: totalBatches,
      fineBlocksCount: extractedBlocks.length,
    });

    console.log(`[R&D Analysis] 🚀 批次 ${batchNo}/${totalBatches} (${dateRange[0]} ~ ${dateRange[1]}, ${currentBatch.length} 条用户提问) -> 模型: ${endpoints.primary.provider_name} (${endpoints.primary.model})`);

    const llmRes = await api.callLlmWithFallback(
      endpoints.primary,
      endpoints.fallback,
      [
        { role: 'system', content: '你只输出合法的 JSON 对象，包含 blocks 数组。禁止输出任何其他分析文字。' },
        { role: 'user', content: prompt },
      ]
    );

    if (llmRes.success && llmRes.content) {
      console.log(`[R&D Analysis] Batch ${batchNo} LLM response length: ${llmRes.content.length} chars`);
      const parsed = safeExtractJson<any>(llmRes.content);
      const rawBlocks = Array.isArray(parsed?.blocks)
        ? parsed.blocks
        : Array.isArray(parsed?.data)
        ? parsed.data
        : Array.isArray(parsed?.items)
        ? parsed.items
        : Array.isArray(parsed)
        ? parsed
        : [];

      if (rawBlocks.length > 0) {
        rawBlocks.forEach((rb: any, seq: number) => {
          extractedBlocks.push(normalizeFineBlock(rb, batchIdx, seq));
        });
        console.log(`[R&D Analysis] Batch ${batchNo} parsed ${rawBlocks.length} blocks`);
      } else {
        console.warn(`[R&D Analysis] Batch ${batchNo} JSON parsed but no blocks found. Raw content:`, llmRes.content.slice(0, 300));
      }
    } else {
      console.error(`[R&D Analysis] Batch ${batchNo} LLM call failed:`, llmRes.error);
      lastError = `[${llmRes.provider_used}] ${llmRes.error || '请求失败'}`;
    }
  }

  if (extractedBlocks.length === 0) {
    return {
      success: false,
      fineBlocks: [],
      message: lastError
        ? `大模型调用失败: ${lastError}`
        : '大模型返回格式未能解析出有效的功能点 Blocks，请检查模型响应或更换模型重试',
    };
  }

  // 持久化存入 SQLite
  onProgress?.({
    stage: 'done',
    detail: `提取完成！正在将 ${extractedBlocks.length} 个细粒度 Blocks 保存入库…`,
    current: totalBatches,
    total: totalBatches,
    fineBlocksCount: extractedBlocks.length,
  });

  await api.saveWorkspaceFineBlocks(workspacePath, extractedBlocks, force);

  return {
    success: true,
    fineBlocks: extractedBlocks,
    message: `成功提取并入库 ${extractedBlocks.length} 个细粒度 Blocks！`,
  };
}

/** Pipeline 阶段 2：合并细粒度 Blocks 为模块总览并入库 */
export async function runMergeModulesPipeline(
  workspacePath: string,
  fineBlocks: WorkspaceFineBlock[],
  stats: WorkspaceDetailStats,
  force = true,
  onProgress?: (p: PipelineProgress) => void
): Promise<{ success: boolean; moduleBlocks: WorkspaceModuleBlock[]; message: string }> {
  const endpoints = getAiEndpoints();
  if (!endpoints.hasKey) {
    return {
      success: false,
      moduleBlocks: [],
      message: '请先在右上角「设置」中配置 AI 供应商及 API Key',
    };
  }

  if (!fineBlocks || fineBlocks.length === 0) {
    return {
      success: false,
      moduleBlocks: [],
      message: '当前没有可合并的细粒度 Blocks，请先完成「提取 Blocks」',
    };
  }

  onProgress?.({
    stage: 'merge',
    detail: `正在将 ${fineBlocks.length} 个细粒度 Blocks 聚合为系统核心模块总览…`,
    current: 1,
    total: 1,
  });

  const slimFineBlocks = fineBlocks.map((fb) => ({
    id: fb.block_id,
    title: fb.title,
    summary: fb.summary,
    type: fb.type,
    keywords: fb.keywords,
    start_date: fb.start_date,
    end_date: fb.end_date,
  }));

  const prompt = `你是软件架构分析专家。请将以下 ${fineBlocks.length} 个细粒度研发功能点（Blocks），按业务领域与架构主线聚合为 4～10 个顶层「核心功能模块总览」。

【合并要求】：
1. 聚合为 4～10 个清晰的核心模块（Module）。
2. 跨批次相同或相近领域的功能点必须归纳到同一个模块中。
3. 每个模块的 child_fine_ids 必须包含其涵盖的细粒度 block 的 id。
4. start_date / end_date 取涵盖的细粒度 blocks 的起止范围。
5. 必须仅输出标准 JSON 格式。

【输出 JSON 结构】：
{
  "modules": [
    {
      "id": "mod-1",
      "type": "module",
      "title": "模块名称（中文，15字内）",
      "summary": "模块核心职责与已实现功能全貌说明（150字内）",
      "start_date": "YYYY-MM-DD",
      "end_date": "YYYY-MM-DD",
      "status": "completed",
      "keywords": ["关键词1", "关键词2"],
      "child_fine_ids": ["blk-1-1", "blk-2-3"]
    }
  ]
}

【细粒度 Blocks 数据】：
${JSON.stringify(slimFineBlocks, null, 2)}`;

  const llmRes = await api.callLlmWithFallback(
    endpoints.primary,
    endpoints.fallback,
    [
      { role: 'system', content: '你只输出合法的 JSON 对象，包含 modules 数组。禁止输出多余解释文字。' },
      { role: 'user', content: prompt },
    ]
  );

  if (!llmRes.success || !llmRes.content) {
    return {
      success: false,
      moduleBlocks: [],
      message: `模块合并失败: ${llmRes.error || 'LLM 未返回有效内容'}`,
    };
  }

  const parsed = safeExtractJson<any>(llmRes.content);
  const rawModules = Array.isArray(parsed?.modules)
    ? parsed.modules
    : Array.isArray(parsed?.data)
    ? parsed.data
    : Array.isArray(parsed?.items)
    ? parsed.items
    : Array.isArray(parsed)
    ? parsed
    : [];

  if (rawModules.length === 0) {
    console.warn('[R&D Analysis] Merge modules returned unparseable content:', llmRes.content);
    return {
      success: false,
      moduleBlocks: [],
      message: 'LLM 未能成功聚合并产出模块列表，请检查模型响应或重试',
    };
  }

  const moduleBlocks = rawModules.map((m, idx) => normalizeModuleBlock(m, idx));

  // 保存入库
  await api.saveWorkspaceModuleBlocks(workspacePath, moduleBlocks, force);

  onProgress?.({
    stage: 'done',
    detail: `成功合并为 ${moduleBlocks.length} 个核心业务模块！`,
    current: 1,
    total: 1,
  });

  return {
    success: true,
    moduleBlocks,
    message: `成功合并为 ${moduleBlocks.length} 个模块总览！`,
  };
}

/** Pipeline 阶段 3：撰写 Markdown 架构演进与研发报告并入库 */
export async function runGenerateReportPipeline(
  workspacePath: string,
  stats: WorkspaceDetailStats,
  moduleBlocks: WorkspaceModuleBlock[],
  fineBlocks: WorkspaceFineBlock[],
  onProgress?: (p: PipelineProgress) => void
): Promise<{ success: boolean; reportMd: string; message: string }> {
  const endpoints = getAiEndpoints();
  if (!endpoints.hasKey) {
    return {
      success: false,
      reportMd: '',
      message: '请先在右上角「设置」中配置 AI 供应商及 API Key',
    };
  }

  if ((!moduleBlocks || moduleBlocks.length === 0) && (!fineBlocks || fineBlocks.length === 0)) {
    return {
      success: false,
      reportMd: '',
      message: '当前缺少模块总览或细粒度 Blocks，请先提取 Blocks 并合并模块',
    };
  }

  onProgress?.({
    stage: 'report',
    detail: '正在基于模块总览与项目研发指标，撰写完整的 Markdown 架构报告…',
    current: 1,
    total: 1,
  });

  const prompt = `你是高级技术写作专家与架构师。请基于以下项目的「模块总览」、「细粒度 Blocks」和「研发统计指标」，撰写一份专业、详实的中文《项目架构与研发演进复盘报告》（Markdown 格式）。

【报告必须包含以下核心章节】：
## 1. 项目概览与研发画像
- 简述项目基本背景、会话总量 (${stats.conversation_count})、用户提问交互数 (${stats.user_message_count})、活跃开发天数 (${stats.active_days} 天) 等关键指标。

## 2. 系统核心架构与模块总览
- 梳理系统主要模块构成及职责边界（参考下方模块总览）。

## 3. 模块功能演进与迭代轨迹
- 结合各模块与时间轴，详细拆解核心功能、重构与关键 Bug 修复历程。

## 4. 研发节奏与工作流分析
- 结合单日峰值 (${stats.peak_count} 条，${stats.peak_day || '近期'})、开发活跃周期与 AI 工具协同模式进行复盘总结。

## 5. 后续演进建议与待办规划
- 给出 3～5 条切实可行的架构优化与演进建议。

【写作规范】：
- 语气专业、结构清晰、排版美观（合理使用表格、列表、引用与加粗）。
- 严禁编造不存在的模块或数字。
- 直接输出 Markdown 文本，无需额外包裹说明。

【项目统计数据】：
- 项目路径: ${workspacePath}
- 活跃天数: ${stats.active_days} 天 (${stats.first_active?.substring(0, 10) || '—'} ~ ${stats.last_active?.substring(0, 10) || '—'})
- 会话总数: ${stats.conversation_count}
- 交互总消息数: ${stats.message_count} (用户提问: ${stats.user_message_count})
- 助手分布: ${stats.agent_breakdown}

【模块总览数据】：
${JSON.stringify(moduleBlocks, null, 2)}

【细粒度 Blocks 样例】：
${JSON.stringify(fineBlocks.slice(0, 30), null, 2)}`;

  const llmRes = await api.callLlmWithFallback(
    endpoints.primary,
    endpoints.fallback,
    [
      { role: 'system', content: '你输出结构严谨、内容丰富的 Markdown 技术架构报告。' },
      { role: 'user', content: prompt },
    ]
  );

  if (!llmRes.success || !llmRes.content) {
    return {
      success: false,
      reportMd: '',
      message: `报告生成失败: ${llmRes.error || 'LLM 未返回有效内容'}`,
    };
  }

  const reportMd = llmRes.content.trim();

  // 持久化存库
  await api.saveWorkspaceReport(workspacePath, reportMd);

  onProgress?.({
    stage: 'done',
    detail: 'Markdown 架构报告生成并保存完成！',
    current: 1,
    total: 1,
  });

  return {
    success: true,
    reportMd,
    message: 'Markdown 架构报告生成成功！',
  };
}
