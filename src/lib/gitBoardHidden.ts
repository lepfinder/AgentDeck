/**
 * Git 看板隐藏名单 — 看板级「不再跟踪」，不碰数据库与会话数据，完全可逆。
 *
 * 存 localStorage；变更时派发 window 事件，让顶栏胶囊与看板同步重算。
 */
const KEY = 'agentdeck_gitboard_hidden';
const EVENT = 'agentdeck-gitboard-hidden-change';

export function getHiddenPaths(): string[] {
  try {
    const raw = localStorage.getItem(KEY);
    const list = raw ? JSON.parse(raw) : [];
    return Array.isArray(list) ? list.filter((x): x is string => typeof x === 'string') : [];
  } catch {
    return [];
  }
}

export function setHiddenPaths(paths: string[]) {
  localStorage.setItem(KEY, JSON.stringify(paths));
  window.dispatchEvent(new CustomEvent(EVENT));
}

export function hideWorkspace(path: string) {
  const list = getHiddenPaths();
  if (!list.includes(path)) setHiddenPaths([...list, path]);
}

export function unhideWorkspace(path: string) {
  setHiddenPaths(getHiddenPaths().filter((p) => p !== path));
}

export function onHiddenChange(cb: () => void): () => void {
  window.addEventListener(EVENT, cb);
  return () => window.removeEventListener(EVENT, cb);
}
