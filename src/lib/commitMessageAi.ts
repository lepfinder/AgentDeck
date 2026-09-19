/**
 * Git 提交信息 AI 生成 — 复用设置里的 LLM 主备链路（callLlmWithFallback）。
 *
 * 输入材料：待提交 diff（截断）、变更文件清单、分支名、最近提交（模仿风格与语言）。
 */
import { api } from '../api/tauriBridge';
import { getServiceLlmConfig } from './serviceLlm';
import type {
  GitBoardEntry,
  GitCommitInfo,
  GitPendingDiffResponse,
} from '../components/gitboard/GitBoardView';

const CONVENTIONAL_RE =
  /^(feat|fix|chore|docs|refactor|test|style|perf|ci|build|revert)(\([^)]+\))?!?:/;

export type CommitMsgLang = 'auto' | 'zh' | 'en';

const LANG_KEY = 'agentdeck_commit_msg_lang';

export function getCommitMsgLang(): CommitMsgLang {
  const v = localStorage.getItem(LANG_KEY);
  return v === 'zh' || v === 'en' ? v : 'auto';
}

export function saveCommitMsgLang(lang: CommitMsgLang) {
  localStorage.setItem(LANG_KEY, lang);
}

function langRequirement(lang: CommitMsgLang): string {
  if (lang === 'zh') return '- 必须使用简体中文撰写（此条优先于对最近提交语言的模仿）';
  if (lang === 'en') return '- 必须使用英文撰写（此条优先于对最近提交语言的模仿）';
  return '- 语言与最近提交保持一致';
}

function buildPrompt(
  entry: GitBoardEntry,
  pending: GitPendingDiffResponse,
  recent: GitCommitInfo[],
  lang: CommitMsgLang
): string {
  const recentLines = recent.slice(0, 5).map((c) => c.message);
  const usesConventional = recentLines.some((m) => CONVENTIONAL_RE.test(m));

  const parts: string[] = [];
  parts.push('请根据以下 git 待提交变更生成一条 commit message。');
  parts.push('');
  parts.push(`当前分支：${entry.branch ?? '(detached HEAD)'}`);
  parts.push(`变更文件（${pending.files.length} 个）：`);
  parts.push(pending.files.slice(0, 60).map((f) => `- ${f}`).join('\n'));
  if (pending.files.length > 60) {
    parts.push(`- …（其余 ${pending.files.length - 60} 个文件略）`);
  }
  if (recentLines.length > 0) {
    parts.push('');
    parts.push(
      lang === 'auto'
        ? '该仓库最近的提交信息（请模仿其语言与格式）：'
        : '该仓库最近的提交信息（请模仿其格式与前缀风格，语言以下方要求为准）：'
    );
    parts.push(recentLines.join('\n'));
  }
  parts.push('');
  parts.push(pending.truncated ? '待提交 diff（过长已截断）：' : '待提交 diff：');
  parts.push('```diff');
  parts.push(pending.diff || '(空)');
  parts.push('```');
  parts.push('');
  parts.push('要求：');
  parts.push('- 只输出 commit message 本身，不要解释、引号或代码块标记');
  parts.push('- 第一行不超过 72 个字符');
  parts.push(langRequirement(lang));
  if (usesConventional) {
    parts.push('- 使用 conventional commits 前缀（feat:/fix:/chore: 等）');
  }
  parts.push('- 如需补充说明，可在第一行后空一行添加简短正文');
  return parts.join('\n');
}

/** 拉取材料并生成提交信息；未配置 AI 或调用失败时抛错。 */
export async function generateCommitMessage(
  entry: GitBoardEntry,
  lang: CommitMsgLang
): Promise<string> {
  const cfg = getServiceLlmConfig();
  if (!cfg) {
    throw new Error('未配置 AI，请先在设置中填写 API Key');
  }
  const [pending, recent] = await Promise.all([
    api.gitBoard.pendingDiff(entry.workspace_path),
    api.gitBoard.commits(entry.workspace_path, 5),
  ]);
  if (pending.files.length === 0) {
    throw new Error('没有待提交的变更');
  }

  const result = await api.callLlmWithFallback(
    cfg.primary,
    cfg.fallback,
    [{ role: 'user', content: buildPrompt(entry, pending, recent, lang) }],
    300,
    true,
    'git_commit_message'
  );
  if (!result.success || !result.content.trim()) {
    throw new Error(result.error || 'AI 返回为空');
  }
  // 去掉模型可能残留的代码块包裹
  return result.content
    .trim()
    .replace(/^```[a-z]*\n?/i, '')
    .replace(/\n?```$/i, '')
    .trim();
}
