import { api } from '../api/tauriBridge';
import type { ConversationItem, MessageItem } from '../types';
import { getServiceLlmConfig } from './serviceLlm';
import { summarizeConversation } from './conversationAi';

export interface BatchSummarizeOptions {
  /** 跳过已有成功摘要/标题的会话 */
  onlyMissing: boolean;
  /** 包含上次失败的会话 */
  includeErrors: boolean;
  /** 已有结果也重新生成（覆盖 onlyMissing） */
  redoExisting: boolean;
  /** 单次最多处理条数 */
  maxCount: number;
}

export const DEFAULT_BATCH_SUMMARIZE_OPTIONS: BatchSummarizeOptions = {
  onlyMissing: true,
  includeErrors: true,
  redoExisting: false,
  maxCount: 50,
};

export type BatchItemStatus = 'queued' | 'running' | 'ok' | 'skip' | 'error';

export interface BatchItemProgress {
  id: string;
  title: string;
  status: BatchItemStatus;
  error?: string;
}

export interface BatchProgress {
  total: number;
  done: number;
  ok: number;
  skip: number;
  error: number;
  currentId: string | null;
  items: BatchItemProgress[];
  cancelled: boolean;
  finished: boolean;
}

function hasSuccessfulAi(c: ConversationItem): boolean {
  return c.ai_status === 'ok' && !!(c.ai_title || c.ai_summary);
}

/** 按选项从当前列表筛出待批量总结的会话 */
export function selectConversationsForBatch(
  conversations: ConversationItem[],
  opts: BatchSummarizeOptions,
): ConversationItem[] {
  const selected = conversations.filter((c) => {
    // 无消息的跳过（加载后仍可能空，那时再 skip）
    if ((c.message_count ?? 0) <= 0 && (c.user_message_count ?? 0) <= 0) {
      return false;
    }

    if (opts.redoExisting) return true;

    if (hasSuccessfulAi(c) && !c.ai_summary_stale) {
      return false;
    }

    if (c.ai_status === 'error') {
      return opts.includeErrors;
    }

    if (c.ai_summary_stale) return true;

    if (opts.onlyMissing) {
      return !c.ai_title && !c.ai_summary;
    }

    return true;
  });

  const max = Math.max(1, Math.min(200, opts.maxCount || 50));
  return selected.slice(0, max);
}

export function estimateBatchSeconds(count: number): number {
  // 粗算：每条约 4–8 秒，取中位 6
  return Math.max(0, count) * 6;
}

/**
 * 串行批量总结命名。可取消；单条失败不中断。
 */
export async function runBatchSummarize(params: {
  conversations: ConversationItem[];
  fetchMessages: (conversationId: string) => Promise<MessageItem[]>;
  onProgress: (progress: BatchProgress) => void;
  shouldCancel: () => boolean;
  onConversationUpdated?: (updated: ConversationItem) => void;
}): Promise<BatchProgress> {
  const { conversations, fetchMessages, onProgress, shouldCancel, onConversationUpdated } =
    params;

  if (!getServiceLlmConfig()) {
    throw new Error('未配置 AI，请先在设置中填写 API Key');
  }

  const items: BatchItemProgress[] = conversations.map((c) => ({
    id: c.id,
    title: c.title || c.source_title || '未命名',
    status: 'queued',
  }));

  const progress: BatchProgress = {
    total: items.length,
    done: 0,
    ok: 0,
    skip: 0,
    error: 0,
    currentId: null,
    items,
    cancelled: false,
    finished: false,
  };

  const emit = () => onProgress({ ...progress, items: [...progress.items] });

  emit();

  for (let i = 0; i < conversations.length; i++) {
    if (shouldCancel()) {
      progress.cancelled = true;
      for (let j = i; j < progress.items.length; j++) {
        if (progress.items[j].status === 'queued') {
          progress.items[j] = { ...progress.items[j], status: 'skip', error: '已取消' };
          progress.skip += 1;
          progress.done += 1;
        }
      }
      break;
    }

    const conv = conversations[i];
    progress.currentId = conv.id;
    progress.items[i] = { ...progress.items[i], status: 'running' };
    emit();

    try {
      const messages = await fetchMessages(conv.id);
      if (shouldCancel()) {
        progress.cancelled = true;
        progress.items[i] = { ...progress.items[i], status: 'skip', error: '已取消' };
        progress.skip += 1;
        progress.done += 1;
        for (let j = i + 1; j < progress.items.length; j++) {
          if (progress.items[j].status === 'queued') {
            progress.items[j] = { ...progress.items[j], status: 'skip', error: '已取消' };
            progress.skip += 1;
            progress.done += 1;
          }
        }
        break;
      }

      const hasUserText = messages.some(
        (m) => m.sender === 'user' && (m.text || '').trim().length > 0,
      );
      if (!hasUserText) {
        progress.items[i] = {
          ...progress.items[i],
          status: 'skip',
          error: '无可总结的用户内容',
        };
        progress.skip += 1;
        progress.done += 1;
        emit();
        continue;
      }

      const updated = await summarizeConversation({ conversation: conv, messages });
      onConversationUpdated?.(updated);
      progress.items[i] = {
        ...progress.items[i],
        status: 'ok',
        title: updated.title || progress.items[i].title,
      };
      progress.ok += 1;
      progress.done += 1;
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      progress.items[i] = { ...progress.items[i], status: 'error', error: msg };
      progress.error += 1;
      progress.done += 1;
    }

    progress.currentId = null;
    emit();
  }

  progress.finished = true;
  progress.currentId = null;
  emit();
  return progress;
}

/** 便于外部直接拉消息 */
export async function fetchConversationMessages(
  conversationId: string,
): Promise<MessageItem[]> {
  return api.getConversationMessages(conversationId);
}
