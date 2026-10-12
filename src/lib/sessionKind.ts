import type { ConversationItem, SessionKind } from '../types';

/** Grok Bot 等「一 bot 一会话」视为无界 */
const OPEN_ENDED_APPS = new Set(['grokbot']);

export function isOpenEndedSourceApp(sourceApp: string | null | undefined): boolean {
  return !!sourceApp && OPEN_ENDED_APPS.has(sourceApp.toLowerCase());
}

export function sessionKindOf(
  conv: Pick<ConversationItem, 'source_app' | 'session_kind'> | null | undefined
): SessionKind {
  if (conv?.session_kind === 'open_ended' || conv?.session_kind === 'bounded') {
    return conv.session_kind;
  }
  return isOpenEndedSourceApp(conv?.source_app) ? 'open_ended' : 'bounded';
}

export const OUTLINE_MIN_USER_MESSAGES = 8;
