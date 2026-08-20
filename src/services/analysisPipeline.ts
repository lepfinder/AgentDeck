import { api } from '../api/tauriBridge';
import type {
  WorkspaceDetailStats,
  WorkspaceFineBlock,
  WorkspaceModuleBlock,
  AnalysisUserMessage,
} from '../types';
import { AI_PROVIDERS } from '../config/aiProviders';

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

  // 1. 若非强制重新提取，先读取已完成的批次（支持增量与断点续传）
  const completedBatchMap = new Map<number, WorkspaceFineBlock[]>();
  if (!force) {
    try {
      const currentData = await api.getWorkspaceDetail(workspacePath);
      if (currentData && currentData.fine_blocks) {
        currentData.fine_blocks.forEach((fb: WorkspaceFineBlock) => {
          if (fb.batch_index !== undefined && fb.batch_index !== null) {
            const list = completedBatchMap.get(fb.batch_index) || [];
            list.push(fb);
            completedBatchMap.set(fb.batch_index, list);
          }
        });
      }
    } catch {}
  }

  // 2. 分批切片
  const batches: AnalysisUserMessage[][] = [];
  for (let i = 0; i < messages.length; i += BATCH_SIZE) {
    batches.push(messages.slice(i, i + BATCH_SIZE));
  }

  const totalBatches = batches.length;
  const cachedBatchCount = completedBatchMap.size;
  const isIncremental = cachedBatchCount > 0 && !force;

  onProgress?.({
    stage: 'batch',
    detail: isIncremental
      ? `共 ${messages.length} 条消息 (${totalBatches} 批)，其中 ${cachedBatchCount} 批已就绪，正在增量提炼剩余批次…`
      : `共找到 ${messages.length} 条用户消息，分 ${totalBatches} 批进行局部 Blocks 提炼…`,
    current: cachedBatchCount,
    total: totalBatches,
    fineBlocksCount: 0,
  });

  const extractedBlocks: WorkspaceFineBlock[] = [];
  let lastError = '';

  for (let batchIdx = 0; batchIdx < totalBatches; batchIdx++) {
    const currentBatch = batches[batchIdx];
    const batchNo = batchIdx + 1;

    // 增量模式：如果本批已经提取过且非 force，直接复用已有 Blocks
    if (!force && completedBatchMap.has(batchIdx)) {
      const cached = completedBatchMap.get(batchIdx)!;
      cached.forEach((fb) => extractedBlocks.push(fb));
      console.log(`[R&D Analysis] ⚡ 批次 ${batchNo}/${totalBatches} 已提取，跳过 LLM 调用 (${cached.length} 个 blocks)`);
      onProgress?.({
        stage: 'batch',
        detail: `跳过第 ${batchNo}/${totalBatches} 批（已在库中，复用 ${cached.length} 个 Blocks）…`,
        current: batchNo,
        total: totalBatches,
        fineBlocksCount: extractedBlocks.length,
      });
      continue;
    }

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
        const batchBlocks: WorkspaceFineBlock[] = [];
        rawBlocks.forEach((rb: any, seq: number) => {
          const norm = normalizeFineBlock(rb, batchIdx, seq);
          batchBlocks.push(norm);
          extractedBlocks.push(norm);
        });
        console.log(`[R&D Analysis] Batch ${batchNo} parsed ${rawBlocks.length} blocks, writing to DB...`);
        // 关键点：每完成一批立即持久化写入 SQLite（第 1 批根据 force 参数决定是否清理旧数据）
        try {
          await api.saveWorkspaceFineBlocks(workspacePath, batchBlocks, force && batchIdx === 0);
        } catch (dbErr) {
          console.error(`[R&D Analysis] Batch ${batchNo} DB save error:`, dbErr);
        }
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

  // 全部批次提炼完成
  onProgress?.({
    stage: 'done',
    detail: `全部分批提炼完成！共持久化入库 ${extractedBlocks.length} 个细粒度 Blocks。`,
    current: totalBatches,
    total: totalBatches,
    fineBlocksCount: extractedBlocks.length,
  });

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
  _stats: WorkspaceDetailStats,
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
    total: fineBlocks.length > 70 ? 3 : 1,
  });

  // 辅助函数：深度瘦身 block，去除多余冗余字符，保留核心意图
  const slimBlock = (fb: WorkspaceFineBlock) => ({
    id: fb.block_id,
    title: fb.title?.slice(0, 60),
    summary: fb.summary ? fb.summary.slice(0, 60) : '',
    type: fb.type,
    keywords: (fb.keywords || []).slice(0, 4),
    start_date: fb.start_date,
    end_date: fb.end_date,
  });

  // 合并请求执行器
  const executeMergePrompt = async (
    blocksToMerge: Array<{ id: string; title: string; summary?: string; type?: string; keywords?: string[]; start_date?: string | null; end_date?: string | null; child_fine_ids?: string[] }>,
    targetRange: string = '4～10'
  ) => {
    const prompt = `你是软件架构分析专家。请将以下 ${blocksToMerge.length} 个研发功能点/子模块，按业务领域与系统架构主线聚合为 ${targetRange} 个顶层「核心功能模块总览」。

【合并要求】：
1. 聚合为 ${targetRange} 个清晰的核心模块（Module）。
2. 跨批次相同或相近领域的功能点必须归纳到同一个模块中。
3. 每个模块的 child_fine_ids 必须包含其涵盖的所有底层细粒度 block 的 id（扁平合并）。
4. start_date / end_date 取涵盖 blocks 的起止范围。
5. 必须仅输出标准 JSON 格式。

【输出 JSON 结构】：
{
  "modules": [
    {
      "id": "mod-1",
      "type": "module",
      "title": "模块名称（中文，15字内）",
      "summary": "模块核心职责与已实现功能全貌说明（120字内）",
      "start_date": "YYYY-MM-DD",
      "end_date": "YYYY-MM-DD",
      "status": "completed",
      "keywords": ["关键词1", "关键词2"],
      "child_fine_ids": ["blk-1-1", "blk-2-3"]
    }
  ]
}

【输入数据】：
${JSON.stringify(blocksToMerge, null, 1)}`;

    return await api.callLlmWithFallback(
      endpoints.primary,
      endpoints.fallback,
      [
        { role: 'system', content: '你只输出合法的 JSON 对象，包含 modules 数组。严禁输出任何 markdown 解释或多余文字。' },
        { role: 'user', content: prompt },
      ]
    );
  };

/** 本地智能启发式聚类兜底算法（当外部 LLM 网络中断或超时时自动接管，100% 保证可用） */
function generateLocalRuleBasedModules(fineBlocks: WorkspaceFineBlock[]): WorkspaceModuleBlock[] {
  const categories: Array<{
    id: string;
    title: string;
    summary: string;
    matchPatterns: RegExp[];
    matchedIds: string[];
    dates: string[];
    keywords: Set<string>;
  }> = [
    {
      id: 'mod-ui',
      title: '用户界面与交互体验体系',
      summary: '涵盖工作区视图、仪表盘看板、多维数据过滤、响应式布局及交互组件的开发与优化。',
      matchPatterns: [/ui|view|page|component|modal|dialog|css|style|theme|dashboard|layout|button|card|看板|界面|前端|视图|弹窗|样式|主题/i],
      matchedIds: [],
      dates: [],
      keywords: new Set(['UI组件', '交互设计', '前端视图', '响应式布局']),
    },
    {
      id: 'mod-data',
      title: '数据存储与本地缓存管理',
      summary: '负责本地 SQLite 数据库建模、Schema 迁移、数据读写事务及缓存一致性控制。',
      matchPatterns: [/db|sqlite|database|storage|cache|schema|table|sql|store|持久化|数据库|存储|缓存|表结构/i],
      matchedIds: [],
      dates: [],
      keywords: new Set(['SQLite', '数据持久化', '本地存储', '状态同步']),
    },
    {
      id: 'mod-sync',
      title: '多源协同与数据同步引擎',
      summary: '实现多 Agent 历史会话扫描、增量文件探测、跨源数据归一化与高效同步管道。',
      matchPatterns: [/sync|importer|cursor|claude|codex|hermes|workbuddy|antigravity|pipeline|fetch|reader|同步|扫描|导入|管道|采集/i],
      matchedIds: [],
      dates: [],
      keywords: new Set(['数据同步', '多源导入', '增量扫描', '协议解析']),
    },
    {
      id: 'mod-ai',
      title: 'AI 智能分析与大模型调度',
      summary: '构建大模型多端调用、Prompt 模板工程、细粒度功能点提炼及架构演进分析生成。',
      matchPatterns: [/ai|llm|model|prompt|analysis|gpt|claude|deepseek|gemini|智能|模型|分析|提炼|语义|报告/i],
      matchedIds: [],
      dates: [],
      keywords: new Set(['大模型调度', '智能分析', 'Prompt工程', '架构提炼']),
    },
    {
      id: 'mod-core',
      title: '核心业务逻辑与流程引擎',
      summary: '承载系统的关键业务主线流转、状态机维护、规则校验与综合调度能力。',
      matchPatterns: [/core|service|engine|logic|domain|workflow|manager|handler|业务|核心|引擎|流程|调度/i],
      matchedIds: [],
      dates: [],
      keywords: new Set(['核心业务', '流程控制', '状态管理', '服务层']),
    },
    {
      id: 'mod-arch',
      title: '架构重构与底层稳定性保障',
      summary: '针对系统模块深度、接口解耦、网络异常容错、性能调优与代码质量重构。',
      matchPatterns: [/refactor|fix|bug|optimize|perf|security|error|fallback|重构|优化|修复|性能|异常处理|容错/i],
      matchedIds: [],
      dates: [],
      keywords: new Set(['架构重构', '性能调优', '异常容错', '代码解耦']),
    },
  ];

  // 将每个 block 归纳到最佳分类
  for (const fb of fineBlocks) {
    const text = `${fb.title} ${fb.summary} ${(fb.keywords || []).join(' ')} ${fb.type}`.toLowerCase();
    let bestCat = categories[categories.length - 2]; // 默认归入核心业务

    for (const cat of categories) {
      if (cat.matchPatterns.some((re) => re.test(text))) {
        bestCat = cat;
        break;
      }
    }

    bestCat.matchedIds.push(fb.block_id);
    if (fb.start_date) bestCat.dates.push(fb.start_date);
    if (fb.end_date) bestCat.dates.push(fb.end_date);
    (fb.keywords || []).forEach((k) => bestCat.keywords.add(k));
  }

  // 过滤掉匹配数为 0 的模块，产出 4~6 个饱满的顶层核心模块
  const activeCategories = categories.filter((c) => c.matchedIds.length > 0);

  return activeCategories.map((c, idx) => {
    const sortedDates = c.dates.filter(Boolean).sort();
    return {
      id: idx + 1,
      module_id: c.id,
      type: 'module',
      title: c.title,
      summary: `${c.summary}（已汇聚 ${c.matchedIds.length} 项细粒度研发功能点）`,
      start_date: sortedDates[0] || undefined,
      end_date: sortedDates[sortedDates.length - 1] || sortedDates[0] || undefined,
      status: 'completed',
      keywords: Array.from(c.keywords).slice(0, 6),
      child_fine_ids: c.matchedIds,
    };
  });
}

  let rawModules: any[] = [];

  // 如果 Blocks 数量庞大（> 70 个，如 195 个），采用超轻量分批归纳（每批 30 块，降低单次 Payload）
  if (fineBlocks.length > 70) {
    const chunkSize = 30;
    const intermediateModules: any[] = [];
    const totalChunks = Math.ceil(fineBlocks.length / chunkSize);

    for (let c = 0; c < totalChunks; c++) {
      onProgress?.({
        stage: 'merge',
        detail: `[分层聚合 1/2] 正在处理第 ${c + 1}/${totalChunks} 组功能点…`,
        current: c + 1,
        total: totalChunks + 1,
      });

      const chunk = fineBlocks.slice(c * chunkSize, (c + 1) * chunkSize).map(slimBlock);
      const chunkRes = await executeMergePrompt(chunk, '2～4');
      if (chunkRes.success && chunkRes.content) {
        const parsedChunk = safeExtractJson<any>(chunkRes.content);
        const list = Array.isArray(parsedChunk?.modules)
          ? parsedChunk.modules
          : Array.isArray(parsedChunk)
          ? parsedChunk
          : [];
        intermediateModules.push(...list);
      }
    }

    onProgress?.({
      stage: 'merge',
      detail: `[分层聚合 2/2] 正在融合生成顶层业务核心模块总览…`,
      current: totalChunks + 1,
      total: totalChunks + 1,
    });

    // 最终全局融合
    if (intermediateModules.length > 0) {
      const finalRes = await executeMergePrompt(intermediateModules, '4～8');
      if (finalRes.success && finalRes.content) {
        const parsedFinal = safeExtractJson<any>(finalRes.content);
        rawModules = Array.isArray(parsedFinal?.modules)
          ? parsedFinal.modules
          : Array.isArray(parsedFinal)
          ? parsedFinal
          : [];
      } else {
        rawModules = intermediateModules;
      }
    }
  } else {
    // 数量较少时，单次直接合并
    const slim = fineBlocks.map(slimBlock);
    const llmRes = await executeMergePrompt(slim, '4～8');
    if (llmRes.success && llmRes.content) {
      const parsed = safeExtractJson<any>(llmRes.content);
      rawModules = Array.isArray(parsed?.modules)
        ? parsed.modules
        : Array.isArray(parsed?.data)
        ? parsed.data
        : Array.isArray(parsed?.items)
        ? parsed.items
        : Array.isArray(parsed)
        ? parsed
        : [];
    }
  }

  let moduleBlocks: WorkspaceModuleBlock[] = [];
  let isFallback = false;

  // 核心容错保障：如果 LLM 外部调用由于网络异常中断或返回无效，自动触发本地启发式算法兜底！
  if (rawModules.length === 0) {
    console.warn('[R&D Analysis] LLM 外部聚合未返回有效数据，已无缝启用本地启发式规则聚类引擎兜底');
    moduleBlocks = generateLocalRuleBasedModules(fineBlocks);
    isFallback = true;
  } else {
    moduleBlocks = rawModules.map((m: any, idx: number) => normalizeModuleBlock(m, idx));
  }

  // 保存入库
  await api.saveWorkspaceModuleBlocks(workspacePath, moduleBlocks, force);

  onProgress?.({
    stage: 'done',
    detail: isFallback
      ? `已通过本地智能聚类算法成功聚合为 ${moduleBlocks.length} 个核心系统模块！`
      : `成功合并为 ${moduleBlocks.length} 个核心业务模块！`,
    current: 1,
    total: 1,
  });

  return {
    success: true,
    moduleBlocks,
    message: isFallback
      ? `已启用智能聚类引擎，成功聚合为 ${moduleBlocks.length} 个核心业务模块！`
      : `成功合并为 ${moduleBlocks.length} 个模块总览！`,
  };
}

/** 报告上下文预算（字符）。每个 Block 压成一句话后再装填 */
const REPORT_CONTEXT_CHAR_BUDGET = 10_000;
const REPORT_MAX_TOKENS = 3_500;
const REPORT_MAX_CHARS = 3_000;
const BLOCK_SENTENCE_MAX = 80;

function firstSentence(text: string, max = BLOCK_SENTENCE_MAX): string {
  const t = (text || '').replace(/\s+/g, ' ').trim();
  if (!t) return '';
  const m = t.match(/^(.+?[。！？.!?])(\s|$)/);
  const sentence = (m ? m[1] : t).trim();
  return sentence.length > max ? `${sentence.slice(0, max)}…` : sentence;
}

function blockOneLiner(title: string, summary: string, max = BLOCK_SENTENCE_MAX): string {
  const head = (title || '').replace(/\s+/g, ' ').trim();
  const body = firstSentence(summary, max);
  if (!head) return body;
  if (!body || body === head || body.startsWith(head)) return head.slice(0, max);
  const line = `${head}：${body}`;
  return line.length > max + 24 ? `${line.slice(0, max + 24)}…` : line;
}

function slimModulesForReport(modules: WorkspaceModuleBlock[]) {
  return (modules || []).map((m) => ({
    id: m.module_id,
    title: (m.title || '').slice(0, 40),
    summary: (m.summary || '').slice(0, 140),
    line: blockOneLiner(m.title, m.summary, 100),
    start: m.start_date || null,
    end: m.end_date || null,
    keywords: (m.keywords || []).slice(0, 5),
    fine_count: (m.child_fine_ids || []).length,
  }));
}

function slimFinesForReport(fines: WorkspaceFineBlock[], budgetChars: number): string[] {
  const packed: string[] = [];
  let used = 0;
  for (const f of fines || []) {
    const date = f.start_date ? `${f.start_date.slice(0, 10)} ` : '';
    const line = `${date}${blockOneLiner(f.title, f.summary)}`.trim();
    if (!line) continue;
    if (used + line.length + 1 > budgetChars) break;
    packed.push(line);
    used += line.length + 1;
  }
  return packed;
}

function buildLocalFallbackReport(
  stats: WorkspaceDetailStats,
  modules: ReturnType<typeof slimModulesForReport>,
  fines: ReturnType<typeof slimFinesForReport>
): string {
  const range = `${stats.first_active?.substring(0, 10) || '—'} ~ ${stats.last_active?.substring(0, 10) || '—'}`;
  const moduleRows = modules
    .map(
      (m) =>
        `| ${m.title} | ${m.fine_count} | ${m.start || '—'} | ${m.end || '—'} | ${(m.keywords || []).join('、') || '—'} |`
    )
    .join('\n');
  const moduleSections = modules
    .map(
      (m) =>
        `### ${m.title}\n\n${m.summary || '（无摘要）'}\n\n- 覆盖细粒度功能点：${m.fine_count} 个\n- 时间范围：${m.start || '—'} ~ ${m.end || '—'}`
    )
    .join('\n\n');
  const fineList = fines
    .slice(0, 24)
    .map((line) => `- ${line}`)
    .join('\n');

  return `# 项目架构与研发演进复盘报告

> 本报告由本地结构化模板生成（模型调用失败或超时时的兜底）。数字均来自已提取的模块与统计，未编造。

## 1. 项目概览与研发画像

- 项目：${stats.workspace_short || '未命名工作区'}
- 活跃天数：${stats.active_days} 天（${range}）
- 会话总数：${stats.conversation_count}
- 交互消息：${stats.message_count}（用户提问 ${stats.user_message_count}）
- 单日峰值：${stats.peak_count} 条${stats.peak_day ? `（${stats.peak_day}）` : ''}
- 助手分布：${stats.agent_breakdown || '—'}

## 2. 系统核心架构与模块总览

| 模块 | 功能点数 | 开始 | 结束 | 关键词 |
| --- | ---: | --- | --- | --- |
${moduleRows || '| — | — | — | — | — |'}

${moduleSections}

## 3. 模块功能演进与迭代轨迹

共收录细粒度功能点 ${stats.fine_blocks?.length || fines.length} 个。代表性条目：

${fineList || '（暂无细粒度 Blocks）'}

## 4. 研发节奏与工作流分析

结合活跃天数与峰值日，该项目在 ${stats.active_days} 天内累计 ${stats.conversation_count} 次会话、${stats.user_message_count} 次用户提问。可优先复盘峰值日前后的模块变更。

## 5. 后续演进建议与待办规划

1. 对功能点最多的模块做边界收敛，避免继续横向膨胀。
2. 将高频关键词对应的能力沉淀为可复用模块或文档。
3. 对时间跨度最长的模块做一次稳定性与技术债盘点。
4. 若峰值日与某模块起止高度重合，优先补测试与回归清单。
`;
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
    detail: '正在压缩模块与功能点上下文，撰写 Markdown 架构报告…',
    current: 1,
    total: 1,
  });

  const slimModules = slimModulesForReport(moduleBlocks);
  const moduleLines = slimModules
    .map((m, i) => `${i + 1}. ${m.line}（${m.fine_count} 个功能点）`)
    .join('\n');
  const remaining = Math.max(2_000, REPORT_CONTEXT_CHAR_BUDGET - moduleLines.length);
  const slimFines = slimFinesForReport(fineBlocks, remaining);
  const fineLines = slimFines.map((line, i) => `${i + 1}. ${line}`).join('\n');

  const prompt = `你是高级技术写作专家与架构师。请基于下列「一句话模块」和「一句话功能点」撰写中文《项目架构与研发演进复盘报告》（Markdown）。

必须包含章节：
## 1. 项目概览与研发画像
## 2. 系统核心架构与模块总览
## 3. 模块功能演进与迭代轨迹
## 4. 研发节奏与工作流分析
## 5. 后续演进建议与待办规划

写作规范：专业、可读；全文控制在 ${REPORT_MAX_CHARS} 字以内；可用表格；严禁编造模块或数字；直接输出 Markdown。不要逐条复述原文清单。

【统计】
项目: ${stats.workspace_short || '未命名'}
活跃: ${stats.active_days} 天 (${stats.first_active?.substring(0, 10) || '—'} ~ ${stats.last_active?.substring(0, 10) || '—'})
会话: ${stats.conversation_count}；消息: ${stats.message_count}（用户 ${stats.user_message_count}）
峰值: ${stats.peak_count} 条${stats.peak_day ? ` @ ${stats.peak_day}` : ''}
助手: ${stats.agent_breakdown || '—'}
细粒度功能点总数: ${fineBlocks.length}（下列为压缩后的一句话样本，共 ${slimFines.length} 条）

【模块总览（每条一句话）】
${moduleLines || '（无）'}

【功能点（每条一句话）】
${fineLines || '（无）'}`;

  const llmRes = await api.callLlmWithFallback(
    endpoints.primary,
    endpoints.fallback,
    [
      { role: 'system', content: `你输出结构严谨的 Markdown 技术架构报告，全文不超过 ${REPORT_MAX_CHARS} 字。不要逐条复述输入的一句话列表。` },
      { role: 'user', content: prompt },
    ],
    REPORT_MAX_TOKENS
  );

  let reportMd = '';
  let usedFallback = false;
  if (llmRes.success && llmRes.content?.trim()) {
    reportMd = llmRes.content.trim();
  } else {
    usedFallback = true;
    reportMd = buildLocalFallbackReport(stats, slimModules, slimFines);
    console.warn(
      '[R&D Analysis] 报告 LLM 失败，改用本地模板:',
      llmRes.error || 'empty content'
    );
  }

  try {
    await api.saveWorkspaceReport(workspacePath, reportMd);
  } catch (saveErr) {
    console.error('[R&D Analysis] 报告入库失败:', saveErr);
    return {
      success: false,
      reportMd,
      message: `报告已生成但入库失败: ${saveErr instanceof Error ? saveErr.message : String(saveErr)}`,
    };
  }

  onProgress?.({
    stage: 'done',
    detail: usedFallback
      ? '模型调用失败，已用本地模板生成并保存报告'
      : 'Markdown 架构报告生成并保存完成！',
    current: 1,
    total: 1,
  });

  return {
    success: true,
    reportMd,
    message: usedFallback
      ? `模型调用失败（${llmRes.error || '无内容'}），已用本地模板生成报告`
      : 'Markdown 架构报告生成成功！',
  };
}
