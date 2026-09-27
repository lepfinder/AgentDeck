import React, { useCallback, useEffect, useMemo, useState } from 'react';
import { api } from '../../api/tauriBridge';
import { useI18n } from '../../i18n';
import type { SkillItem, SkillsSummary, SkillUsageRow } from '../../types';
import { formatBeijingTime } from '../../utils/date';
import { ChevronLeft, ChevronRight, RefreshCw } from 'lucide-react';

const PAGE_SIZE = 20;
type ScopeFilter = 'all' | 'public' | 'private';

const thCls = 'px-3 py-1.5 text-left text-[11px] font-medium theme-text-sub whitespace-nowrap';
const tdCls = 'px-3 py-1.5 text-xs whitespace-nowrap';
const numCls = `${tdCls} text-right font-mono theme-text-main tabular-nums`;

export const SkillsView: React.FC = () => {
  const { t } = useI18n();
  const [skills, setSkills] = useState<SkillItem[]>([]);
  const [summary, setSummary] = useState<SkillsSummary | null>(null);
  const [loading, setLoading] = useState(true);
  const [refreshing, setRefreshing] = useState(false);
  const [search, setSearch] = useState('');
  const [scope, setScope] = useState<ScopeFilter>('all');
  const [agent, setAgent] = useState('');
  const [page, setPage] = useState(1);
  const [expanded, setExpanded] = useState<SkillItem | null>(null);
  const [detailContent, setDetailContent] = useState<string | null>(null);
  const [detailLoading, setDetailLoading] = useState(false);
  const [detailUsages, setDetailUsages] = useState<SkillUsageRow[]>([]);

  const reload = useCallback(() => {
    setLoading(true);
    Promise.all([api.listSkills(undefined, undefined), api.getSkillsSummary()])
      .then(([list, sum]) => {
        setSkills(list);
        setSummary(sum);
      })
      .catch(() => undefined)
      .finally(() => setLoading(false));
  }, []);

  useEffect(() => {
    reload();
  }, [reload]);

  const filtered = useMemo(() => {
    const q = search.trim().toLowerCase();
    return skills
      .filter((s) => (scope === 'all' ? true : scope === 'public' ? s.scope === 'public' : s.scope !== 'public'))
      .filter((s) => (agent ? s.agents.includes(agent) : true))
      .filter((s) => (q ? s.name.toLowerCase().includes(q) || s.description.toLowerCase().includes(q) : true))
      .sort((a, b) => b.use_count - a.use_count || a.name.localeCompare(b.name));
  }, [skills, search, scope, agent]);

  const pageCount = Math.max(1, Math.ceil(filtered.length / PAGE_SIZE));
  const safePage = Math.min(page, pageCount);
  const pageRows = filtered.slice((safePage - 1) * PAGE_SIZE, safePage * PAGE_SIZE);

  useEffect(() => {
    setPage(1);
  }, [search, scope, agent]);

  const openDetail = (item: SkillItem) => {
    if (expanded?.id === item.id) {
      setExpanded(null);
      return;
    }
    setExpanded(item);
    setDetailContent(null);
    setDetailUsages([]);
    setDetailLoading(true);
    Promise.all([api.getSkillContent(item.id), api.getSkillDetail(item.name)])
      .then(([content, usages]) => {
        setDetailContent(content);
        setDetailUsages(usages);
      })
      .catch(() => undefined)
      .finally(() => setDetailLoading(false));
  };

  const handleRefresh = async () => {
    setRefreshing(true);
    try {
      await api.refreshSkills();
    } finally {
      reload();
      setRefreshing(false);
    }
  };

  const scopeLabel = (s: SkillItem) =>
    s.scope === 'public' ? t('skills.public') : t('skills.private');

  const pageBtn = (disabled: boolean, onClick: () => void, icon: React.ReactNode, label: string) => (
    <button
      type="button"
      disabled={disabled}
      onClick={onClick}
      title={label}
      className={`p-1 rounded-md transition-all ${
        disabled
          ? 'theme-text-sub opacity-40 cursor-not-allowed'
          : 'theme-text-main hover:bg-blue-600 hover:text-white cursor-pointer'
      }`}
    >
      {icon}
    </button>
  );

  return (
    <div className="h-full overflow-y-auto">
      <div className="max-w-5xl mx-auto px-6 py-5 space-y-5">
        {/* 标题 + 重新扫描 */}
        <div className="flex items-baseline justify-between gap-3">
          <div>
            <h1 className="text-base font-bold theme-text-main">{t('skills.title')}</h1>
            <p className="text-xs theme-text-sub mt-0.5">{t('skills.subtitle')}</p>
          </div>
          <button
            onClick={handleRefresh}
            disabled={refreshing}
            className="flex items-center gap-1.5 px-2.5 py-1 text-xs theme-bg-sub border theme-border rounded-lg theme-text-muted hover:theme-text-main transition-colors cursor-pointer"
          >
            <RefreshCw className={`h-3.5 w-3.5 ${refreshing ? 'animate-spin text-blue-500' : ''}`} />
            <span>{refreshing ? t('skills.scanning') : t('skills.rescan')}</span>
          </button>
        </div>

        {/* 汇总：扁平文本一行 */}
        {summary && (
          <p className="text-xs theme-text-muted font-mono tabular-nums">
            {t('skills.summary', {
              total: summary.total,
              pub: summary.public_count,
              priv: summary.private_count,
              used30: summary.usage_events_30d,
              used: summary.usage_events,
            })}
          </p>
        )}

        {/* 过滤行 */}
        <div className="flex items-center gap-3 flex-wrap">
          <input
            value={search}
            onChange={(e) => setSearch(e.target.value)}
            placeholder={t('skills.search')}
            className="px-2.5 py-1 text-xs w-52 theme-bg-input border theme-border rounded-lg theme-text-main placeholder-slate-400 focus:outline-none focus:border-blue-500"
          />
          <div className="flex items-center theme-bg-sub rounded-lg border theme-border text-xs">
            {(['all', 'public', 'private'] as ScopeFilter[]).map((opt) => (
              <button
                key={opt}
                onClick={() => setScope(opt)}
                className={`px-2.5 py-1 rounded-md transition-colors cursor-pointer ${
                  scope === opt
                    ? 'theme-bg-card theme-text-main font-medium'
                    : 'theme-text-sub hover:theme-text-main'
                }`}
              >
                {opt === 'all' ? t('skills.scopeAll') : opt === 'public' ? t('skills.public') : t('skills.private')}
              </button>
            ))}
          </div>
          <select
            value={agent}
            onChange={(e) => setAgent(e.target.value)}
            className="px-2 py-1 text-xs theme-bg-input border theme-border rounded-lg theme-text-main focus:outline-none focus:border-blue-500"
          >
            <option value="">{t('skills.allAgents')}</option>
            {(summary?.agents || []).map((a) => (
              <option key={a} value={a}>
                {a}
              </option>
            ))}
          </select>
        </div>

        {loading && skills.length === 0 ? (
          <div className="flex flex-col items-center justify-center py-20 theme-text-muted">
            <RefreshCw className="h-8 w-8 animate-spin text-blue-500 mb-3" />
            <p className="text-sm">{t('skills.loading')}</p>
          </div>
        ) : filtered.length === 0 ? (
          <div className="flex items-center justify-center py-20 theme-text-sub text-sm">{t('skills.empty')}</div>
        ) : (
          <>
            {/* 技能表 */}
            <table className="w-full">
              <thead>
                <tr className="border-b theme-border-sub">
                  <th className={thCls}>{t('skills.name')}</th>
                  <th className={thCls}>{t('skills.scope')}</th>
                  <th className={thCls}>{t('skills.agents')}</th>
                  <th className={`${thCls} text-right`}>{t('skills.use30')}</th>
                  <th className={`${thCls} text-right`}>{t('skills.useTotal')}</th>
                  <th className={`${thCls} text-right`}>{t('skills.lastUsed')}</th>
                </tr>
              </thead>
              <tbody>
                {pageRows.map((s) => {
                  const isOpen = expanded?.id === s.id;
                  return (
                    <tr
                      key={s.id}
                      onClick={() => openDetail(s)}
                      className={`border-b theme-border-sub/50 cursor-pointer transition-colors ${
                        isOpen ? 'theme-bg-card' : 'hover:theme-bg-card/60'
                      }`}
                    >
                      <td className={tdCls}>
                        <span className="theme-text-main font-medium">{s.name}</span>
                      </td>
                      <td className={tdCls}>
                        <span className={s.scope === 'public' ? 'theme-text-main' : 'theme-text-muted'}>
                          {scopeLabel(s)}
                        </span>
                      </td>
                      <td className={`${tdCls} theme-text-muted max-w-[18rem] truncate`}>
                        {s.agents.join(' · ') || '—'}
                      </td>
                      <td className={numCls}>{s.use_count_30d > 0 ? s.use_count_30d : '—'}</td>
                      <td className={numCls}>{s.use_count > 0 ? s.use_count : '—'}</td>
                      <td className={`${tdCls} text-right font-mono theme-text-muted text-[11px]`}>
                        {s.last_used_at ? formatBeijingTime(s.last_used_at) : '—'}
                      </td>
                    </tr>
                  );
                })}
              </tbody>
            </table>

            {/* 分页条（常驻） */}
            <div className="flex items-center justify-between">
              <span className="text-[11px] theme-text-sub font-mono">
                {t('skills.pageInfo', { page: safePage, pages: pageCount, n: filtered.length })}
              </span>
              <div className="flex items-center theme-bg-sub rounded-lg border theme-border">
                {pageBtn(safePage <= 1, () => setPage(safePage - 1), <ChevronLeft className="h-3.5 w-3.5" />, t('skills.prevPage'))}
                <span className="px-2 py-1 text-xs theme-text-main tabular-nums min-w-[3.5rem] text-center">
                  {safePage} / {pageCount}
                </span>
                {pageBtn(safePage >= pageCount, () => setPage(safePage + 1), <ChevronRight className="h-3.5 w-3.5" />, t('skills.nextPage'))}
              </div>
            </div>

            {/* 行内预览 */}
            {expanded && (
              <section className="theme-bg-card border theme-border rounded-xl p-4 space-y-3">
                <div className="flex items-baseline justify-between gap-2">
                  <h2 className="text-sm font-semibold theme-text-main">
                    {expanded.name}
                    <span className="ml-2 text-[11px] font-normal theme-text-sub">{scopeLabel(expanded)}</span>
                  </h2>
                  <button
                    onClick={() => setExpanded(null)}
                    className="text-[11px] theme-text-sub hover:theme-text-main cursor-pointer"
                  >
                    {t('skills.collapse')}
                  </button>
                </div>
                {expanded.description && (
                  <p className="text-xs theme-text-muted leading-relaxed">{expanded.description}</p>
                )}
                <p className="text-[11px] theme-text-sub font-mono break-all">{expanded.path}</p>
                <div>
                  <h3 className="text-xs font-semibold theme-text-muted mb-1">{t('skills.recentUsages')}</h3>
                  {detailLoading ? (
                    <p className="text-xs theme-text-sub">{t('skills.loading')}</p>
                  ) : detailUsages.length === 0 ? (
                    <p className="text-xs theme-text-sub">{t('skills.noUsages')}</p>
                  ) : (
                    <ul className="space-y-1">
                      {detailUsages.slice(0, 8).map((u, i) => (
                        <li key={i} className="flex items-center gap-2 text-[11px] theme-text-muted">
                          <span className="theme-text-sub font-mono flex-shrink-0">
                            {u.occurred_at ? formatBeijingTime(u.occurred_at) : '—'}
                          </span>
                          <span className="flex-shrink-0">{u.agent || '—'}</span>
                          <span className="flex-shrink-0">
                            {u.evidence === 'explicit' ? t('skills.evidenceExplicit') : t('skills.evidenceRead')}
                          </span>
                          <span className="theme-text-main truncate">{u.conversation_title || '—'}</span>
                        </li>
                      ))}
                    </ul>
                  )}
                </div>
                <div>
                  <h3 className="text-xs font-semibold theme-text-muted mb-1">SKILL.md</h3>
                  {detailLoading ? (
                    <p className="text-xs theme-text-sub">{t('skills.loading')}</p>
                  ) : detailContent ? (
                    <pre className="max-h-72 overflow-y-auto text-[11px] leading-relaxed theme-text-muted font-mono whitespace-pre-wrap break-all theme-bg-input border theme-border rounded-lg p-3">
                      {detailContent}
                    </pre>
                  ) : (
                    <p className="text-xs theme-text-sub">{t('skills.noContent')}</p>
                  )}
                </div>
              </section>
            )}
          </>
        )}
      </div>
    </div>
  );
};
