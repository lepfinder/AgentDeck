import React, { useEffect, useState } from 'react';
import { api } from '../../api/tauriBridge';
import { useI18n, type Locale } from '../../i18n';
import type { UsageStatsPayload } from '../../types';
import { RefreshCw } from 'lucide-react';

// 0 = 今天（本地零点起），null = 全部
type RangeOption = 0 | 7 | 30 | null;

const thCls =
  'px-3 py-1.5 text-left text-[11px] font-medium theme-text-sub whitespace-nowrap';
const tdCls = 'px-3 py-1.5 text-xs whitespace-nowrap';
const numCls = `${tdCls} text-right font-mono theme-text-main tabular-nums`;

function fmtInt(n: number): string {
  return n.toLocaleString();
}

// token 量级走中文单位（万/亿）或英文缩写（K/M/B），一位小数、去尾零
function fmtTokens(n: number, locale: Locale): string {
  const one = (v: number) => {
    const s = v.toFixed(1);
    return s.endsWith('.0') ? s.slice(0, -2) : s;
  };
  if (locale === 'en') {
    if (n >= 1e9) return `${one(n / 1e9)}B`;
    if (n >= 1e6) return `${one(n / 1e6)}M`;
    if (n >= 1e3) return `${one(n / 1e3)}K`;
    return n.toLocaleString();
  }
  if (n >= 1e8) return `${one(n / 1e8)}亿`;
  const wan = one(n / 1e4);
  if (Number(wan) >= 1e4) return `${one(n / 1e8)}亿`;
  return n >= 1e4 ? `${wan}万` : n.toLocaleString();
}

function fmtCredit(n: number): string {
  if (n === 0) return '—';
  return n.toLocaleString(undefined, { maximumFractionDigits: 3 });
}

export const UsageStatsView: React.FC = () => {
  const { t, locale } = useI18n();
  const fmtTok = (n: number) => fmtTokens(n, locale);
  const [range, setRange] = useState<RangeOption>(7);
  const [stats, setStats] = useState<UsageStatsPayload | null>(null);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    let alive = true;
    setLoading(true);
    api
      .getUsageStats(range)
      .then((data) => {
        if (!alive) return;
        setStats(data);
        setLoading(false);
      })
      .catch(() => {
        if (alive) setLoading(false);
      });
    return () => {
      alive = false;
    };
  }, [range]);

  if (loading && !stats) {
    return (
      <div className="flex h-full flex-col items-center justify-center theme-text-muted">
        <RefreshCw className="h-8 w-8 animate-spin text-blue-500 mb-3" />
        <p className="text-sm">{t('usage.loading')}</p>
      </div>
    );
  }

  if (!stats || stats.totals.requests === 0) {
    return (
      <div className="flex h-full items-center justify-center theme-text-sub text-sm">
        {t('usage.empty')}
      </div>
    );
  }

  const { totals, by_agent: byAgent, by_model: byModel, by_project: byProject, by_session: bySession, by_day: byDay } = stats;
  const rangeBtn = (opt: RangeOption, label: string) => (
    <button
      key={label}
      onClick={() => setRange(opt)}
      className={`px-2 py-0.5 text-xs rounded-md cursor-pointer transition-colors ${
        range === opt
          ? 'theme-bg-card theme-text-main font-medium'
          : 'theme-text-sub hover:theme-text-main'
      }`}
    >
      {label}
    </button>
  );

  const hasCredit = totals.credit > 0 || byAgent.some((a) => a.credit > 0);

  return (
    <div className="h-full overflow-y-auto">
      <div className="max-w-5xl mx-auto px-6 py-5 space-y-6">
        {/* 标题 + 时间范围 */}
        <div className="flex items-baseline justify-between">
          <div>
            <h1 className="text-base font-bold theme-text-main">{t('usage.title')}</h1>
            <p className="text-xs theme-text-sub mt-0.5">{t('usage.subtitle')}</p>
          </div>
          <div className="flex items-center gap-1">
            {rangeBtn(0, t('usage.rangeToday'))}
            {rangeBtn(7, t('usage.range7'))}
            {rangeBtn(30, t('usage.range30'))}
            {rangeBtn(null, t('usage.rangeAll'))}
          </div>
        </div>

        {/* 合计：扁平文本一行 */}
        <p className="text-xs theme-text-muted font-mono tabular-nums">
          {t('usage.total')} · {t('usage.requests')} {fmtInt(totals.requests)}
          <span className="theme-text-sub"> · </span>
          {t('usage.input')} {fmtTok(totals.input_tokens)}
          <span className="theme-text-sub"> · </span>
          {t('usage.cacheRead')} {fmtTok(totals.cache_read_tokens)}
          <span className="theme-text-sub"> · </span>
          {t('usage.cacheWrite')} {fmtTok(totals.cache_write_tokens)}
          <span className="theme-text-sub"> · </span>
          {t('usage.output')} {fmtTok(totals.output_tokens)}
          <span className="theme-text-sub"> · </span>
          {t('usage.reasoning')} {fmtTok(totals.reasoning_tokens)}
          {hasCredit && (
            <>
              <span className="theme-text-sub"> · </span>
              {t('usage.credit')} {fmtCredit(totals.credit)}
            </>
          )}
        </p>

        {/* 按天 */}
        {byDay.length > 0 && (
          <section>
            <h2 className="text-xs font-semibold theme-text-muted mb-1.5">{t('usage.byDay')}</h2>
            <table className="w-full">
              <thead>
                <tr className="border-b theme-border-sub">
                  <th className={thCls}>{t('usage.date')}</th>
                  <th className={`${thCls} text-right`}>{t('usage.requests')}</th>
                  <th className={`${thCls} text-right`}>{t('usage.input')}</th>
                  <th className={`${thCls} text-right`}>{t('usage.cacheRead')}</th>
                  <th className={`${thCls} text-right`}>{t('usage.cacheWrite')}</th>
                  <th className={`${thCls} text-right`}>{t('usage.output')}</th>
                  <th className={`${thCls} text-right`}>{t('usage.reasoning')}</th>
                  {hasCredit && <th className={`${thCls} text-right`}>{t('usage.credit')}</th>}
                </tr>
              </thead>
              <tbody>
                {byDay.map((row) => (
                  <tr key={row.date} className="border-b theme-border-sub/50">
                    <td className={tdCls}>
                      <span className="theme-text-main font-mono">{row.date}</span>
                    </td>
                    <td className={numCls}>{fmtInt(row.requests)}</td>
                    <td className={numCls}>{fmtTok(row.input_tokens)}</td>
                    <td className={numCls}>{fmtTok(row.cache_read_tokens)}</td>
                    <td className={numCls}>{fmtTok(row.cache_write_tokens)}</td>
                    <td className={numCls}>{fmtTok(row.output_tokens)}</td>
                    <td className={numCls}>{fmtTok(row.reasoning_tokens)}</td>
                    {hasCredit && <td className={numCls}>{fmtCredit(row.credit)}</td>}
                  </tr>
                ))}
              </tbody>
            </table>
          </section>
        )}

        {/* 按 Agent */}
        <section>
          <h2 className="text-xs font-semibold theme-text-muted mb-1.5">{t('usage.byAgent')}</h2>
          <table className="w-full">
            <thead>
              <tr className="border-b theme-border-sub">
                <th className={thCls}>{t('usage.agent')}</th>
                <th className={`${thCls} text-right`}>{t('usage.requests')}</th>
                <th className={`${thCls} text-right`}>{t('usage.input')}</th>
                <th className={`${thCls} text-right`}>{t('usage.cacheRead')}</th>
                <th className={`${thCls} text-right`}>{t('usage.cacheWrite')}</th>
                <th className={`${thCls} text-right`}>{t('usage.output')}</th>
                <th className={`${thCls} text-right`}>{t('usage.reasoning')}</th>
                {hasCredit && <th className={`${thCls} text-right`}>{t('usage.credit')}</th>}
              </tr>
            </thead>
            <tbody>
              {byAgent.map((row) => (
                <tr key={row.agent} className="border-b theme-border-sub/50">
                  <td className={tdCls}>
                    <span className="theme-text-main font-medium">{row.label}</span>
                  </td>
                  <td className={numCls}>{fmtInt(row.requests)}</td>
                  <td className={numCls}>{fmtTok(row.input_tokens)}</td>
                  <td className={numCls}>{fmtTok(row.cache_read_tokens)}</td>
                  <td className={numCls}>{fmtTok(row.cache_write_tokens)}</td>
                  <td className={numCls}>{fmtTok(row.output_tokens)}</td>
                  <td className={numCls}>{fmtTok(row.reasoning_tokens)}</td>
                  {hasCredit && <td className={numCls}>{fmtCredit(row.credit)}</td>}
                </tr>
              ))}
            </tbody>
          </table>
        </section>

        {/* 按模型 */}
        {byModel.length > 0 && (
          <section>
            <h2 className="text-xs font-semibold theme-text-muted mb-1.5">{t('usage.byModel')}</h2>
            <table className="w-full">
              <thead>
                <tr className="border-b theme-border-sub">
                  <th className={thCls}>{t('usage.agent')}</th>
                  <th className={thCls}>{t('usage.model')}</th>
                  <th className={`${thCls} text-right`}>{t('usage.requests')}</th>
                  <th className={`${thCls} text-right`}>{t('usage.input')}</th>
                  <th className={`${thCls} text-right`}>{t('usage.cacheRead')}</th>
                  <th className={`${thCls} text-right`}>{t('usage.cacheWrite')}</th>
                  <th className={`${thCls} text-right`}>{t('usage.output')}</th>
                  <th className={`${thCls} text-right`}>{t('usage.reasoning')}</th>
                  {hasCredit && <th className={`${thCls} text-right`}>{t('usage.credit')}</th>}
                </tr>
              </thead>
              <tbody>
                {byModel.map((row) => (
                  <tr
                    key={`${row.agent}:${row.model}`}
                    className="border-b theme-border-sub/50"
                  >
                    <td className={tdCls}>
                      <span className="theme-text-muted">{row.agent}</span>
                    </td>
                    <td className={tdCls}>
                      <span className="theme-text-main font-mono">
                        {row.model || t('usage.unknownModel')}
                      </span>
                    </td>
                    <td className={numCls}>{fmtInt(row.requests)}</td>
                    <td className={numCls}>{fmtTok(row.input_tokens)}</td>
                    <td className={numCls}>{fmtTok(row.cache_read_tokens)}</td>
                    <td className={numCls}>{fmtTok(row.cache_write_tokens)}</td>
                    <td className={numCls}>{fmtTok(row.output_tokens)}</td>
                    <td className={numCls}>{fmtTok(row.reasoning_tokens)}</td>
                    {hasCredit && <td className={numCls}>{fmtCredit(row.credit)}</td>}
                  </tr>
                ))}
              </tbody>
            </table>
          </section>
        )}

        {/* 按项目（前 20） */}
        {byProject.length > 0 && (
          <section>
            <h2 className="text-xs font-semibold theme-text-muted mb-1.5">
              {t('usage.byProject')}
              <span className="ml-1.5 font-normal">{t('usage.top20')}</span>
            </h2>
            <table className="w-full">
              <thead>
                <tr className="border-b theme-border-sub">
                  <th className={thCls}>{t('usage.project')}</th>
                  <th className={`${thCls} text-right`}>{t('usage.requests')}</th>
                  <th className={`${thCls} text-right`}>{t('usage.input')}</th>
                  <th className={`${thCls} text-right`}>{t('usage.cacheRead')}</th>
                  <th className={`${thCls} text-right`}>{t('usage.cacheWrite')}</th>
                  <th className={`${thCls} text-right`}>{t('usage.output')}</th>
                  <th className={`${thCls} text-right`}>{t('usage.reasoning')}</th>
                  {hasCredit && <th className={`${thCls} text-right`}>{t('usage.credit')}</th>}
                </tr>
              </thead>
              <tbody>
                {byProject.map((row) => (
                  <tr key={row.workspace_path} className="border-b theme-border-sub/50">
                    <td className={tdCls}>
                      <span className="theme-text-main font-medium">{row.project}</span>
                    </td>
                    <td className={numCls}>{fmtInt(row.requests)}</td>
                    <td className={numCls}>{fmtTok(row.input_tokens)}</td>
                    <td className={numCls}>{fmtTok(row.cache_read_tokens)}</td>
                    <td className={numCls}>{fmtTok(row.cache_write_tokens)}</td>
                    <td className={numCls}>{fmtTok(row.output_tokens)}</td>
                    <td className={numCls}>{fmtTok(row.reasoning_tokens)}</td>
                    {hasCredit && <td className={numCls}>{fmtCredit(row.credit)}</td>}
                  </tr>
                ))}
              </tbody>
            </table>
          </section>
        )}

        {/* 按会话（前 20） */}
        {bySession.length > 0 && (
          <section>
            <h2 className="text-xs font-semibold theme-text-muted mb-1.5">
              {t('usage.bySession')}
              <span className="ml-1.5 font-normal">{t('usage.top20')}</span>
            </h2>
            <table className="w-full">
              <thead>
                <tr className="border-b theme-border-sub">
                  <th className={thCls}>{t('usage.project')}</th>
                  <th className={thCls}>{t('usage.session')}</th>
                  <th className={`${thCls} text-right`}>{t('usage.requests')}</th>
                  <th className={`${thCls} text-right`}>{t('usage.input')}</th>
                  <th className={`${thCls} text-right`}>{t('usage.cacheRead')}</th>
                  <th className={`${thCls} text-right`}>{t('usage.cacheWrite')}</th>
                  <th className={`${thCls} text-right`}>{t('usage.output')}</th>
                  <th className={`${thCls} text-right`}>{t('usage.reasoning')}</th>
                  {hasCredit && <th className={`${thCls} text-right`}>{t('usage.credit')}</th>}
                </tr>
              </thead>
              <tbody>
                {bySession.map((row) => (
                  <tr key={row.conversation_id} className="border-b theme-border-sub/50">
                    <td className={tdCls}>
                      <span className="theme-text-muted">{row.project}</span>
                    </td>
                    <td className={tdCls}>
                      <span className="theme-text-main font-medium">{row.session || '—'}</span>
                    </td>
                    <td className={numCls}>{fmtInt(row.requests)}</td>
                    <td className={numCls}>{fmtTok(row.input_tokens)}</td>
                    <td className={numCls}>{fmtTok(row.cache_read_tokens)}</td>
                    <td className={numCls}>{fmtTok(row.cache_write_tokens)}</td>
                    <td className={numCls}>{fmtTok(row.output_tokens)}</td>
                    <td className={numCls}>{fmtTok(row.reasoning_tokens)}</td>
                    {hasCredit && <td className={numCls}>{fmtCredit(row.credit)}</td>}
                  </tr>
                ))}
              </tbody>
            </table>
          </section>
        )}

        {/* 如实声明不支持 token 统计的 agent */}
        {stats.agents_without_usage.length > 0 && (
          <p className="text-[11px] theme-text-sub">
            {t('usage.noUsageAgents')} {stats.agents_without_usage.join(' · ')}
          </p>
        )}
      </div>
    </div>
  );
};
