/**
 * 本地服务管理 — 左列表 / 右状态 + 控制台
 */
import { useState, useEffect, useMemo, useRef } from 'react';
import {
  RefreshCw,
  Loader2,
  Play,
  Square,
  RotateCcw,
  Globe,
  Server,
  FolderOpen,
  ChevronDown,
  ChevronRight,
  Plus,
  Trash2,
  Pencil,
  CheckCircle2,
  AlertCircle,
  AlertTriangle,
  LayoutDashboard,
  BookOpen,
  Copy,
  ExternalLink,
} from 'lucide-react';
import { useLocalServices, type ServiceStatus, type InstalledIdes } from '../../hooks/useLocalServices';
import {
  useServiceLogAlerts,
  type ServiceLogAlert,
} from '../../hooks/useServiceLogAlerts';
import { ServiceConsole } from './ServiceConsole';
import {
  ServicesFleetOverview,
  type FleetFilter,
} from './ServicesFleetOverview';
import { formatUptime } from '../../utils/formatUptime';
import { formatErrorAgo } from '../../utils/formatServiceLog';
import { cn } from '../../lib/cn';
import { api } from '../../api/tauriBridge';
import cursorIcon from '../../assets/ides/cursor.png';
import antigravityIcon from '../../assets/ides/antigravity.png';

const IDE_OPTIONS = [
  { id: 'cursor' as const, name: 'Cursor', icon: cursorIcon },
  { id: 'antigravity' as const, name: 'Antigravity', icon: antigravityIcon },
];

function defaultIde(ides: InstalledIdes) {
  return IDE_OPTIONS.find((opt) => ides[opt.id]) ?? IDE_OPTIONS[0];
}

function IdeIcon({ src, alt }: { src: string; alt: string }) {
  return <img src={src} alt={alt} className="h-4 w-4 rounded-[3px] object-contain shrink-0" />;
}

function LiveUptime({ uptimeSecs, active }: { uptimeSecs?: number; active: boolean }) {
  const [live, setLive] = useState(uptimeSecs ?? 0);

  useEffect(() => {
    if (uptimeSecs !== undefined) {
      setLive(uptimeSecs);
    }
  }, [uptimeSecs]);

  useEffect(() => {
    if (!active || uptimeSecs === undefined) return;
    const timer = setInterval(() => setLive((v) => v + 1), 1000);
    return () => clearInterval(timer);
  }, [active, uptimeSecs]);

  if (uptimeSecs === undefined) return <>—</>;
  return <>{formatUptime(live)}</>;
}

function stateLabel(state: ServiceStatus['state'], health: ServiceStatus['health']): string {
  if (state === 'starting') return '启动中';
  if (state === 'stopping') return '停止中';
  if (state === 'partial') return '部分就绪';
  if (state === 'running' && (health === 'ok' || health === 'ports_ok')) return '运行中';
  if (state === 'running' || state === 'unhealthy') return '未就绪';
  if (state === 'port_conflict') return '端口冲突';
  if (state === 'stale_pid') return '未启动';
  return '未启动';
}

function stateDotClass(state: ServiceStatus['state'], health: ServiceStatus['health']): string {
  if (state === 'starting' || state === 'stopping') return 'bg-amber-400 animate-pulse';
  if (state === 'partial') return 'bg-amber-500';
  if (state === 'running' && (health === 'ok' || health === 'ports_ok')) return 'bg-emerald-500';
  if (state === 'running' || state === 'unhealthy') return 'bg-amber-500';
  if (state === 'port_conflict') return 'bg-red-500';
  // stale_pid / stopped：未启动
  return 'bg-zinc-400';
}

function stateBadge(state: ServiceStatus['state'], health: ServiceStatus['health']) {
  const label = stateLabel(state, health);
  const base = 'inline-flex items-center rounded-md px-2 py-0.5 text-xs font-medium';
  if (state === 'starting' || state === 'stopping') {
    return <span className={cn(base, 'theme-bg-sub theme-text-muted')}>{label}</span>;
  }
  if (state === 'partial') {
    return <span className={cn(base, 'bg-amber-500 text-white')}>{label}</span>;
  }
  if (state === 'running' && (health === 'ok' || health === 'ports_ok')) {
    return <span className={cn(base, 'bg-emerald-600 text-white')}>{label}</span>;
  }
  if (state === 'running' || state === 'unhealthy') {
    return <span className={cn(base, 'theme-bg-sub theme-text-muted')}>{label}</span>;
  }
  if (state === 'port_conflict') {
    return <span className={cn(base, 'bg-red-600 text-white')}>{label}</span>;
  }
  // stopped / stale_pid
  return <span className={cn(base, 'border theme-border theme-text-muted')}>{label}</span>;
}

function HealthStatus({ health }: { health: ServiceStatus['health'] }) {
  const display =
    health === 'ok'
      ? { label: '健康', className: 'text-emerald-600 font-medium' }
      : health === 'ports_ok'
        ? { label: '端口就绪', className: 'text-amber-600 font-medium' }
        : health === 'no_response'
          ? { label: '无响应', className: 'text-red-600 font-medium' }
          : { label: '—', className: 'theme-text-muted' };
  return <p className={`mt-0.5 ${display.className}`}>{display.label}</p>;
}

function displayPath(path: string): string {
  const home = path.match(/^(\/Users\/[^/]+)/)?.[1];
  if (home && path.startsWith(home)) {
    return `~${path.slice(home.length)}`;
  }
  return path;
}

function openInFinder(path: string) {
  void api.openUrl(path);
}

function actionBtnClass(opts: {
  variant?: 'default' | 'outline' | 'ghost' | 'danger';
  disabled?: boolean;
}): string {
  const { variant = 'outline', disabled } = opts;
  const base =
    'inline-flex items-center h-8 rounded-lg px-2.5 text-xs font-medium transition-colors cursor-pointer disabled:pointer-events-none disabled:opacity-50';
  if (variant === 'default') {
    return cn(base, 'bg-blue-600 text-white hover:bg-blue-700');
  }
  if (variant === 'ghost') {
    return cn(base, 'theme-text-muted hover:theme-bg-sub hover:theme-text-main');
  }
  if (variant === 'danger') {
    return cn(base, 'theme-text-muted hover:theme-bg-sub text-red-600 hover:text-red-600');
  }
  return cn(base, 'border theme-border theme-bg-card theme-text-main hover:theme-bg-sub', disabled && '');
}

function IdeSplitButton({
  ides,
  preferredIde,
  onOpen,
}: {
  ides: InstalledIdes;
  preferredIde: (typeof IDE_OPTIONS)[number];
  onOpen: (ide: 'cursor' | 'antigravity') => void;
}) {
  const [menuOpen, setMenuOpen] = useState(false);
  const ref = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!menuOpen) return;
    const onDown = (e: MouseEvent) => {
      if (ref.current && !ref.current.contains(e.target as Node)) setMenuOpen(false);
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') setMenuOpen(false);
    };
    document.addEventListener('mousedown', onDown);
    document.addEventListener('keydown', onKey);
    return () => {
      document.removeEventListener('mousedown', onDown);
      document.removeEventListener('keydown', onKey);
    };
  }, [menuOpen]);

  return (
    <div ref={ref} className="relative flex items-center shrink-0">
      <button
        type="button"
        className="inline-flex h-6 items-center gap-1 rounded-l-md border theme-border border-r-0 px-2 text-xs theme-text-main hover:theme-bg-sub disabled:opacity-50 cursor-pointer"
        disabled={!ides[preferredIde.id]}
        onClick={() => onOpen(preferredIde.id)}
      >
        <IdeIcon src={preferredIde.icon} alt="" />
        <span>{preferredIde.name}</span>
      </button>
      <button
        type="button"
        title="选择其他 IDE"
        className="inline-flex h-6 w-6 items-center justify-center rounded-r-md border theme-border theme-text-muted hover:theme-bg-sub cursor-pointer"
        onClick={() => setMenuOpen((v) => !v)}
      >
        <ChevronDown className="h-3 w-3 opacity-70" />
      </button>
      {menuOpen && (
        <div className="absolute left-0 top-full z-50 mt-1 min-w-[10rem] rounded-xl border theme-border theme-bg-card p-1 shadow-xl">
          {IDE_OPTIONS.map((opt) => (
            <button
              key={opt.id}
              type="button"
              disabled={!ides[opt.id]}
              className={cn(
                'flex w-full items-center gap-2 rounded-lg px-2.5 py-1.5 text-left text-xs transition-colors',
                ides[opt.id]
                  ? 'theme-text-main hover:theme-bg-sub cursor-pointer'
                  : 'opacity-50 cursor-not-allowed theme-text-muted'
              )}
              onClick={() => {
                if (!ides[opt.id]) return;
                onOpen(opt.id);
                setMenuOpen(false);
              }}
            >
              <IdeIcon src={opt.icon} alt="" />
              {opt.name}
              {!ides[opt.id] ? '（未安装）' : ''}
            </button>
          ))}
        </div>
      )}
    </div>
  );
}

function projectRunningSummary(services: ServiceStatus[]): string {
  const running = services.filter(
    (s) =>
      s.state === 'running' ||
      s.state === 'partial' ||
      s.state === 'starting' ||
      s.state === 'stopping'
  ).length;
  if (running === 0) return '未启动';
  if (running === services.length) return '全部运行';
  return `${running}/${services.length} 运行`;
}

function latestGroupAlert(
  services: ServiceStatus[],
  alerts: Record<string, ServiceLogAlert>,
): ServiceLogAlert | undefined {
  let best: ServiceLogAlert | undefined;
  for (const svc of services) {
    const a = alerts[svc.id];
    if (!a) continue;
    if (!best || (a.latestAtMs ?? -1) > (best.latestAtMs ?? -1)) {
      best = { ...a };
    } else if (best && a.latestAtMs === best.latestAtMs) {
      best = { ...best, count: best.count + a.count };
    }
  }
  return best;
}

function NavStatusLine({
  label,
  alert,
  nowMs,
}: {
  label: string;
  alert?: ServiceLogAlert;
  nowMs: number;
}) {
  if (!alert) {
    return <span className="block truncate text-[10px] theme-text-muted mt-0.5">{label}</span>;
  }
  return (
    <span className="block truncate text-[10px] mt-0.5 text-red-500">
      {label} · 错误 {formatErrorAgo(alert.latestAtMs, nowMs)}
    </span>
  );
}

export function ServicesView(): React.ReactElement {
  const {
    services,
    projectGroups,
    loading,
    refreshing,
    actionLoading,
    actionMessage,
    error,
    refresh,
    startService,
    stopService,
    restartService,
    openService,
    openInIde,
    ides,
    tailLog,
    removeService,
    renameProject,
    renameService,
  } = useLocalServices();

  const { alerts, nowMs, ignoreServiceErrors, ignoredByServiceId, ignoreEpoch } =
    useServiceLogAlerts(services, tailLog);

  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [fleetFilter, setFleetFilter] = useState<FleetFilter>('all');
  const [collapsedProjects, setCollapsedProjects] = useState<Record<string, boolean>>({});
  const [addHelpOpen, setAddHelpOpen] = useState(false);
  const [copiedHint, setCopiedHint] = useState<string | null>(null);
  const [deleteTarget, setDeleteTarget] = useState<{ id: string; name: string } | null>(null);
  const [renameTarget, setRenameTarget] = useState<
    | { kind: 'project'; projectId: string; name: string }
    | { kind: 'service'; id: string; name: string }
    | null
  >(null);
  const [renameDraft, setRenameDraft] = useState('');
  const renameInputRef = useRef<HTMLInputElement>(null);

  // 进入模块默认总览；仅当已选服务被删除时清空选中，不自动跳到第一个服务
  useEffect(() => {
    if (services.length === 0) {
      setSelectedId(null);
      return;
    }
    setSelectedId((prev) => {
      if (prev && services.some((s) => s.id === prev)) return prev;
      return null;
    });
  }, [services]);

  const selected = useMemo(
    () => services.find((s) => s.id === selectedId) ?? null,
    [services, selectedId]
  );

  const selectedProjectMulti = useMemo(() => {
    if (!selected) return false;
    const group = projectGroups.find((g) => g.projectId === selected.projectId);
    return (group?.services.length ?? 0) > 1;
  }, [projectGroups, selected]);

  useEffect(() => {
    if (!renameTarget) return;
    setRenameDraft(renameTarget.name);
    const t = window.setTimeout(() => renameInputRef.current?.select(), 0);
    return () => window.clearTimeout(t);
  }, [renameTarget]);

  const submitRename = async () => {
    if (!renameTarget) return;
    const next = renameDraft.trim();
    if (!next || next === renameTarget.name) {
      setRenameTarget(null);
      return;
    }
    if (renameTarget.kind === 'project') {
      await renameProject(renameTarget.projectId, next);
    } else {
      await renameService(renameTarget.id, next);
    }
    setRenameTarget(null);
  };

  const isBusy = (id: string, actions: string[]) =>
    actions.some((a) => actionLoading === `${a}-${id}`) ||
    services.some((s) => s.id === id && (s.state === 'starting' || s.state === 'stopping'));

  const preferredIde = defaultIde(ides);
  const consoleLive =
    !!selected &&
    (selected.state === 'running' ||
      selected.state === 'partial' ||
      selected.state === 'unhealthy' ||
      selected.state === 'starting' ||
      selected.state === 'stopping');

  const copyText = async (text: string, id: string) => {
    try {
      await navigator.clipboard.writeText(text);
      setCopiedHint(id);
      window.setTimeout(() => setCopiedHint(null), 1500);
    } catch (e) {
      console.error(e);
    }
  };

  const installPrompt = `请安装 AgentDeck Services Skill：https://raw.githubusercontent.com/lepfinder/AgentDeck/main/skills/agentdeck-services/README.md
装完告诉我是否需要开启新会话。`;

  const verifyPrompt =
    '请用 agentdeck-services 探活 AgentDeck，并列出当前已注册的项目与服务。';

  const registerPrompt =
    '阅读下这个项目，请使用 agentdeck-services 技能，把本地开发服务注册一下。';

  return (
    <div className="h-full flex flex-col gap-3 overflow-hidden theme-bg-main theme-text-main p-4">
      <div className="flex items-center justify-between gap-4 shrink-0">
        <div>
          <h1 className="text-xl font-semibold tracking-tight theme-text-main">本地服务</h1>
          <p className="text-xs theme-text-muted mt-0.5">
            {projectGroups.length} 项目 · {services.length} 服务
          </p>
        </div>
        <div className="flex items-center gap-2 shrink-0">
          <button
            type="button"
            onClick={() => setAddHelpOpen(true)}
            className={actionBtnClass({ variant: 'default' })}
          >
            <Plus className="mr-1.5 h-4 w-4" />
            添加服务
          </button>
          <button
            type="button"
            onClick={() => void refresh()}
            disabled={refreshing}
            className={actionBtnClass({ variant: 'outline', disabled: refreshing })}
          >
            {refreshing ? (
              <Loader2 className="mr-1.5 h-4 w-4 animate-spin" />
            ) : (
              <RefreshCw className="mr-1.5 h-4 w-4" />
            )}
            刷新
          </button>
        </div>
      </div>

      {loading && services.length === 0 ? (
        <div className="flex flex-1 items-center justify-center">
          <Loader2 className="h-8 w-8 animate-spin theme-text-muted" />
        </div>
      ) : services.length === 0 ? (
        <div className="flex flex-1 flex-col items-center justify-center rounded-2xl border border-dashed theme-border px-6">
          <Server className="h-14 w-14 theme-text-muted opacity-30" />
          <h3 className="mt-4 text-lg font-medium theme-text-main">暂无注册服务</h3>
          <p className="mt-2 max-w-md text-center text-sm theme-text-muted leading-relaxed">
            在项目里让 Agent 使用 <span className="font-mono text-xs">agentdeck-services</span>{' '}
            技能注册本地服务后，即可在此查看与启停。
          </p>
          <button
            type="button"
            className={cn(actionBtnClass({ variant: 'default' }), 'mt-4')}
            onClick={() => setAddHelpOpen(true)}
          >
            <BookOpen className="mr-1.5 h-4 w-4" />
            查看如何添加
          </button>
        </div>
      ) : (
        <div className="flex flex-1 min-h-0 gap-3 overflow-hidden">
          {/* 左侧：项目 → 服务 */}
          <aside className="w-56 shrink-0 flex flex-col rounded-2xl border theme-border theme-bg-card overflow-hidden">
            <div className="px-3 py-2 border-b theme-border text-[11px] font-medium theme-text-muted tracking-wide">
              导航
            </div>
            <div className="flex-1 overflow-y-auto p-1.5 space-y-0.5">
              <button
                type="button"
                onClick={() => setSelectedId(null)}
                className={cn(
                  'w-full flex items-center gap-2 rounded-lg px-2.5 py-2 text-left transition-colors cursor-pointer',
                  selectedId === null
                    ? 'bg-blue-600/10 text-blue-600'
                    : 'hover:theme-bg-sub theme-text-muted hover:theme-text-main'
                )}
              >
                <LayoutDashboard className="h-3.5 w-3.5 shrink-0" />
                <span className="truncate text-sm font-medium">总览</span>
              </button>

              <div className="px-2.5 pt-2 pb-1 text-[10px] font-medium theme-text-muted uppercase tracking-wider">
                项目
              </div>

              {projectGroups.map((group) => {
                const collapsed = collapsedProjects[group.projectId];
                const multi = group.services.length > 1;
                const groupAlert = latestGroupAlert(group.services, alerts);
                const singleAlert = !multi ? alerts[group.services[0]?.id ?? ''] : undefined;
                return (
                  <div key={group.projectId} className="space-y-0.5">
                    <div
                      className={cn(
                        'group w-full flex items-center gap-1.5 rounded-lg px-2 py-1.5 transition-colors',
                        !multi && group.services[0]?.id === selectedId
                          ? 'bg-blue-600/10 theme-text-main'
                          : 'hover:theme-bg-sub theme-text-muted hover:theme-text-main'
                      )}
                    >
                      <button
                        type="button"
                        onClick={() => {
                          if (multi) {
                            setCollapsedProjects((prev) => ({
                              ...prev,
                              [group.projectId]: !prev[group.projectId],
                            }));
                          } else if (group.services[0]) {
                            setSelectedId(group.services[0].id);
                          }
                        }}
                        className="min-w-0 flex-1 flex items-center gap-1.5 text-left cursor-pointer"
                      >
                        {multi ? (
                          collapsed ? (
                            <ChevronRight className="h-3.5 w-3.5 shrink-0" />
                          ) : (
                            <ChevronDown className="h-3.5 w-3.5 shrink-0" />
                          )
                        ) : (
                          <span className="relative shrink-0 ml-0.5">
                            <span
                              className={cn(
                                'block h-2 w-2 rounded-full',
                                singleAlert
                                  ? 'bg-red-500'
                                  : stateDotClass(
                                      group.services[0].state,
                                      group.services[0].health,
                                    ),
                              )}
                            />
                          </span>
                        )}
                        <span className="min-w-0 flex-1">
                          <span className="flex items-center gap-1 min-w-0">
                            <span className="block truncate text-sm font-medium theme-text-main">
                              {group.projectName}
                            </span>
                            {(multi ? groupAlert : singleAlert) && (
                              <AlertTriangle className="h-3 w-3 shrink-0 text-red-500" />
                            )}
                          </span>
                          <NavStatusLine
                            label={
                              multi
                                ? `${group.services.length} 个服务 · ${projectRunningSummary(group.services)}`
                                : stateLabel(group.services[0].state, group.services[0].health)
                            }
                            alert={multi ? groupAlert : singleAlert}
                            nowMs={nowMs}
                          />
                        </span>
                      </button>
                      <button
                        type="button"
                        className="shrink-0 p-0.5 rounded theme-text-muted hover:theme-text-main hover:theme-bg-sub cursor-pointer opacity-0 delay-0 transition-opacity duration-150 group-hover:opacity-100 group-hover:delay-200 focus-visible:opacity-100 focus-visible:delay-0"
                        title="重命名项目"
                        onClick={() =>
                          setRenameTarget({
                            kind: 'project',
                            projectId: group.projectId,
                            name: group.projectName,
                          })
                        }
                      >
                        <Pencil className="h-3 w-3" />
                      </button>
                    </div>

                    {multi &&
                      !collapsed &&
                      group.services.map((svc) => {
                        const active = svc.id === selectedId;
                        const alert = alerts[svc.id];
                        return (
                          <div
                            key={svc.id}
                            className={cn(
                              'group w-full flex items-center gap-1 rounded-lg pl-6 pr-1.5 py-1.5 transition-colors',
                              active
                                ? 'bg-blue-600/10 theme-text-main'
                                : 'hover:theme-bg-sub theme-text-muted hover:theme-text-main'
                            )}
                          >
                            <button
                              type="button"
                              onClick={() => setSelectedId(svc.id)}
                              className="min-w-0 flex-1 flex items-center gap-2 text-left cursor-pointer"
                            >
                              <span
                                className={cn(
                                  'h-2 w-2 rounded-full shrink-0',
                                  alert ? 'bg-red-500' : stateDotClass(svc.state, svc.health),
                                )}
                              />
                              <span className="min-w-0 flex-1">
                                <span className="flex items-center gap-1 min-w-0">
                                  <span
                                    className={cn(
                                      'block truncate text-sm',
                                      active && 'font-medium',
                                    )}
                                  >
                                    {svc.name}
                                  </span>
                                  {alert && (
                                    <AlertTriangle className="h-3 w-3 shrink-0 text-red-500" />
                                  )}
                                </span>
                                <NavStatusLine
                                  label={stateLabel(svc.state, svc.health)}
                                  alert={alert}
                                  nowMs={nowMs}
                                />
                              </span>
                            </button>
                            <button
                              type="button"
                              className="shrink-0 p-0.5 rounded theme-text-muted hover:theme-text-main hover:theme-bg-sub cursor-pointer opacity-0 delay-0 transition-opacity duration-150 group-hover:opacity-100 group-hover:delay-200 focus-visible:opacity-100 focus-visible:delay-0"
                              title="重命名服务"
                              onClick={() =>
                                setRenameTarget({
                                  kind: 'service',
                                  id: svc.id,
                                  name: svc.name,
                                })
                              }
                            >
                              <Pencil className="h-3 w-3" />
                            </button>
                          </div>
                        );
                      })}
                  </div>
                );
              })}
            </div>
          </aside>

          {/* 右侧：总览 或 单服务详情 + 控制台 */}
          <div className="flex-1 min-w-0 flex flex-col gap-3 overflow-hidden">
            {selected ? (
              <>
                <section className="shrink-0 rounded-xl border theme-border theme-bg-card p-4 space-y-4">
                  <div className="flex items-start justify-between gap-4">
                    <div className="space-y-1.5 min-w-0">
                      <button
                        type="button"
                        onClick={() => setSelectedId(null)}
                        className="text-[11px] text-blue-500 hover:underline cursor-pointer"
                      >
                        ← 返回总览
                      </button>
                      <div className="flex items-center gap-2 flex-wrap">
                        <h2 className="text-lg font-semibold tracking-tight theme-text-main flex items-center gap-1.5 min-w-0">
                          {selectedProjectMulti &&
                          selected.projectName &&
                          selected.projectName !== selected.name ? (
                            <>
                              <button
                                type="button"
                                className="truncate hover:underline decoration-dotted underline-offset-2 cursor-pointer"
                                title="重命名项目"
                                onClick={() =>
                                  setRenameTarget({
                                    kind: 'project',
                                    projectId: selected.projectId,
                                    name: selected.projectName,
                                  })
                                }
                              >
                                {selected.projectName}
                              </button>
                              <span className="theme-text-muted font-normal shrink-0">·</span>
                              <button
                                type="button"
                                className="truncate hover:underline decoration-dotted underline-offset-2 cursor-pointer"
                                title="重命名服务"
                                onClick={() =>
                                  setRenameTarget({
                                    kind: 'service',
                                    id: selected.id,
                                    name: selected.name,
                                  })
                                }
                              >
                                {selected.name}
                              </button>
                            </>
                          ) : (
                            <button
                              type="button"
                              className="truncate hover:underline decoration-dotted underline-offset-2 cursor-pointer"
                              title="重命名服务"
                              onClick={() =>
                                setRenameTarget({
                                  kind: 'service',
                                  id: selected.id,
                                  name: selected.name,
                                })
                              }
                            >
                              {selected.name}
                            </button>
                          )}
                          <button
                            type="button"
                            className="shrink-0 p-1 rounded theme-text-muted hover:theme-text-main hover:theme-bg-sub cursor-pointer"
                            title="重命名服务"
                            onClick={() =>
                              setRenameTarget({
                                kind: 'service',
                                id: selected.id,
                                name: selected.name,
                              })
                            }
                          >
                            <Pencil className="h-3.5 w-3.5" />
                          </button>
                        </h2>
                        {stateBadge(selected.state, selected.health)}
                        {selected.extras?.managedBy === 'external' && (
                          <span className="inline-flex items-center rounded-md border theme-border px-2 py-0.5 text-xs font-normal theme-text-muted">
                            外部启动
                          </span>
                        )}
                        {selected.state === 'port_conflict' && selected.extras?.portOwner && (
                          <span className="inline-flex items-center rounded-md border theme-border px-2 py-0.5 text-xs font-normal text-red-600">
                            与 {selected.extras.portOwner} 冲突
                          </span>
                        )}
                      </div>
                      <p className="text-sm theme-text-muted font-mono">{selected.openUrl}</p>
                      {selected.projectDir && (
                        <div className="flex items-center gap-2 min-w-0">
                          <button
                            type="button"
                            className="flex items-center gap-1.5 min-w-0 text-left text-xs theme-text-muted hover:theme-text-main transition-colors group cursor-pointer"
                            title="在 Finder 中打开"
                            onClick={() => openInFinder(selected.projectDir!)}
                          >
                            <FolderOpen className="h-3.5 w-3.5 shrink-0 opacity-70 group-hover:opacity-100" />
                            <span className="font-mono truncate">
                              {displayPath(selected.projectDir)}
                            </span>
                          </button>
                          <IdeSplitButton
                            ides={ides}
                            preferredIde={preferredIde}
                            onOpen={(ide) => void openInIde(selected.projectDir!, ide)}
                          />
                        </div>
                      )}
                    </div>

                    <div className="flex flex-wrap gap-2 shrink-0 justify-end">
                      <button
                        type="button"
                        className={actionBtnClass({
                          variant:
                            selected.state === 'running' ||
                            selected.state === 'partial' ||
                            selected.state === 'unhealthy'
                              ? 'outline'
                              : 'default',
                          disabled: isBusy(selected.id, ['start', 'restart', 'stop']),
                        })}
                        disabled={isBusy(selected.id, ['start', 'restart', 'stop'])}
                        onClick={() => void startService(selected.id)}
                      >
                        {actionLoading === `start-${selected.id}` ? (
                          <Loader2 className="mr-1 h-4 w-4 animate-spin" />
                        ) : (
                          <Play className="mr-1 h-4 w-4" />
                        )}
                        启动
                      </button>
                      <button
                        type="button"
                        className={actionBtnClass({
                          variant:
                            selected.state === 'running' ||
                            selected.state === 'partial' ||
                            selected.state === 'unhealthy'
                              ? 'default'
                              : 'outline',
                          disabled: isBusy(selected.id, ['start', 'restart', 'stop']),
                        })}
                        disabled={isBusy(selected.id, ['start', 'restart', 'stop'])}
                        onClick={() => void stopService(selected.id)}
                      >
                        {actionLoading === `stop-${selected.id}` ? (
                          <Loader2 className="mr-1 h-4 w-4 animate-spin" />
                        ) : (
                          <Square className="mr-1 h-4 w-4" />
                        )}
                        停止
                      </button>
                      <button
                        type="button"
                        className={actionBtnClass({
                          variant: 'outline',
                          disabled: isBusy(selected.id, ['start', 'restart', 'stop']),
                        })}
                        disabled={isBusy(selected.id, ['start', 'restart', 'stop'])}
                        onClick={() => void restartService(selected.id)}
                      >
                        {actionLoading === `restart-${selected.id}` ? (
                          <Loader2 className="mr-1 h-4 w-4 animate-spin" />
                        ) : (
                          <RotateCcw className="mr-1 h-4 w-4" />
                        )}
                        重启
                      </button>
                      <button
                        type="button"
                        className={actionBtnClass({ variant: 'ghost' })}
                        disabled={actionLoading === `open-${selected.id}`}
                        onClick={() => void openService(selected.id).catch(() => {})}
                      >
                        <Globe className="mr-1 h-4 w-4" />
                        打开
                      </button>
                      <button
                        type="button"
                        className={actionBtnClass({
                          variant: 'danger',
                          disabled: isBusy(selected.id, ['start', 'restart', 'stop', 'remove']),
                        })}
                        disabled={isBusy(selected.id, ['start', 'restart', 'stop', 'remove'])}
                        onClick={() => setDeleteTarget({ id: selected.id, name: selected.name })}
                      >
                        {actionLoading === `remove-${selected.id}` ? (
                          <Loader2 className="mr-1 h-4 w-4 animate-spin" />
                        ) : (
                          <Trash2 className="mr-1 h-4 w-4" />
                        )}
                        删除
                      </button>
                    </div>
                  </div>

                  <div className="grid grid-cols-2 md:grid-cols-3 lg:grid-cols-6 gap-3 text-sm">
                    <div>
                      <span className="theme-text-muted text-xs">PID</span>
                      <p className="font-mono mt-0.5 theme-text-main">{selected.pid ?? '—'}</p>
                    </div>
                    <div>
                      <span className="theme-text-muted text-xs">健康</span>
                      <HealthStatus health={selected.health} />
                    </div>
                    <div>
                      <span className="theme-text-muted text-xs">端口</span>
                      <p className="font-mono mt-0.5 theme-text-main">
                        {selected.ports.map((p) => (
                          <span key={p.port} className="mr-2">
                            {p.port}
                            {p.listening ? ' ✓' : ''}
                          </span>
                        ))}
                      </p>
                    </div>
                    <div>
                      <span className="theme-text-muted text-xs">启动时间</span>
                      <p className="font-mono mt-0.5 text-xs leading-relaxed theme-text-main">
                        {selected.startedAt ?? '—'}
                      </p>
                    </div>
                    <div>
                      <span className="theme-text-muted text-xs">运行时长</span>
                      <p className="mt-0.5 theme-text-main">
                        <LiveUptime
                          uptimeSecs={selected.uptimeSecs}
                          active={
                            selected.state === 'running' ||
                            selected.state === 'partial' ||
                            selected.state === 'unhealthy'
                          }
                        />
                      </p>
                    </div>
                    {selected.extras?.frontend && (
                      <div>
                        <span className="theme-text-muted text-xs">前端</span>
                        <p className="mt-0.5 theme-text-main">{selected.extras.frontend}</p>
                      </div>
                    )}
                  </div>
                </section>

                <section className="flex-1 min-h-0 rounded-xl border theme-border theme-bg-card p-3">
                  <ServiceConsole
                    serviceId={selected.id}
                    logFile={selected.logFile}
                    live={consoleLive}
                    onFetchLog={tailLog}
                    ignoredSignatures={ignoredByServiceId(selected.id)}
                    ignoreEpoch={ignoreEpoch}
                    onIgnoreErrors={(signatures) =>
                      ignoreServiceErrors(selected.id, signatures)
                    }
                  />
                </section>
              </>
            ) : (
              <ServicesFleetOverview
                projectGroups={projectGroups}
                services={services}
                filter={fleetFilter}
                onFilterChange={setFleetFilter}
                onSelectService={setSelectedId}
              />
            )}
          </div>
        </div>
      )}

      {addHelpOpen && (
        <div
          className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 p-6"
          onClick={() => setAddHelpOpen(false)}
        >
          <div
            className="w-full max-w-3xl max-h-[88vh] overflow-y-auto rounded-2xl border theme-border theme-bg-main p-6 md:p-8 shadow-2xl"
            onClick={(e) => e.stopPropagation()}
          >
            <div className="flex items-start justify-between gap-4">
              <div className="min-w-0">
                <p className="text-[11px] font-medium tracking-wide theme-text-muted uppercase">
                  Agent Skill
                </p>
                <h3 className="mt-1 text-xl font-semibold theme-text-main tracking-tight">
                  一次安装，之后直接用中文管理本地服务
                </h3>
                <p className="mt-2 text-sm theme-text-muted leading-relaxed">
                  把 <span className="font-mono text-xs">agentdeck-services</span>{' '}
                  装进 Cursor / Claude Code 等 Agent，即可注册、启停、看日志。需先打开 AgentDeck Desktop。
                </p>
              </div>
              <button
                type="button"
                className="shrink-0 theme-text-muted hover:theme-text-main cursor-pointer text-sm px-1"
                onClick={() => setAddHelpOpen(false)}
              >
                关闭
              </button>
            </div>

            <div className="mt-6 grid grid-cols-1 sm:grid-cols-3 gap-3">
              {[
                {
                  n: '01',
                  t: '把安装提示词发给 Agent',
                  d: 'Agent 会从 GitHub 拉取 SKILL.md 与 scripts，写入用户级技能目录。',
                },
                {
                  n: '02',
                  t: '按需开启新会话',
                  d: '多数 Agent 只在会话开始时扫描 Skill；装完后新开一场再验证。',
                },
                {
                  n: '03',
                  t: '在项目里注册服务',
                  d: '在目标项目里粘贴下方「注册提示词」，让 Agent 自己判断并注册。',
                },
              ].map((step) => (
                <div
                  key={step.n}
                  className="rounded-xl border theme-border theme-bg-card p-3.5"
                >
                  <div className="text-[11px] font-mono theme-text-muted">{step.n}</div>
                  <div className="mt-1 text-sm font-semibold theme-text-main">{step.t}</div>
                  <p className="mt-1.5 text-xs theme-text-muted leading-relaxed">{step.d}</p>
                </div>
              ))}
            </div>

            <section className="mt-6 rounded-2xl border theme-border theme-bg-card p-4 md:p-5">
              <div className="flex items-center justify-between gap-2">
                <h4 className="text-sm font-semibold theme-text-main">安装提示词</h4>
                <button
                  type="button"
                  onClick={() => void copyText(installPrompt, 'install')}
                  className="inline-flex items-center gap-1.5 text-xs theme-text-muted hover:theme-text-main cursor-pointer"
                >
                  {copiedHint === 'install' ? (
                    <CheckCircle2 className="h-3.5 w-3.5 text-emerald-500" />
                  ) : (
                    <Copy className="h-3.5 w-3.5" />
                  )}
                  {copiedHint === 'install' ? '已复制' : '复制'}
                </button>
              </div>
              <pre className="mt-3 rounded-xl theme-bg-sub border theme-border px-3.5 py-3 text-[13px] font-mono theme-text-main whitespace-pre-wrap leading-relaxed">
                {installPrompt}
              </pre>
              <p className="mt-2 text-[11px] theme-text-muted">
                入口：
                <span className="font-mono">
                  {' '}
                  https://raw.githubusercontent.com/lepfinder/AgentDeck/main/skills/agentdeck-services/README.md
                </span>
              </p>
            </section>

            <section className="mt-4 rounded-2xl border theme-border theme-bg-card p-4 md:p-5">
              <div className="flex items-center justify-between gap-2">
                <h4 className="text-sm font-semibold theme-text-main">验证提示词</h4>
                <button
                  type="button"
                  onClick={() => void copyText(verifyPrompt, 'verify')}
                  className="inline-flex items-center gap-1.5 text-xs theme-text-muted hover:theme-text-main cursor-pointer"
                >
                  {copiedHint === 'verify' ? (
                    <CheckCircle2 className="h-3.5 w-3.5 text-emerald-500" />
                  ) : (
                    <Copy className="h-3.5 w-3.5" />
                  )}
                  {copiedHint === 'verify' ? '已复制' : '复制'}
                </button>
              </div>
              <pre className="mt-3 rounded-xl theme-bg-sub border theme-border px-3.5 py-3 text-[13px] font-mono theme-text-main whitespace-pre-wrap leading-relaxed">
                {verifyPrompt}
              </pre>
              <p className="mt-2 text-[11px] theme-text-muted">
                成功时会先探活 8788，再返回项目/服务列表（或明确「暂无服务」）。
              </p>
            </section>

            <section className="mt-4 rounded-2xl border theme-border theme-bg-card p-4 md:p-5">
              <div className="flex items-center justify-between gap-2">
                <h4 className="text-sm font-semibold theme-text-main">注册提示词（在目标项目里用）</h4>
                <button
                  type="button"
                  onClick={() => void copyText(registerPrompt, 'register')}
                  className="inline-flex items-center gap-1.5 text-xs theme-text-muted hover:theme-text-main cursor-pointer"
                >
                  {copiedHint === 'register' ? (
                    <CheckCircle2 className="h-3.5 w-3.5 text-emerald-500" />
                  ) : (
                    <Copy className="h-3.5 w-3.5" />
                  )}
                  {copiedHint === 'register' ? '已复制' : '复制'}
                </button>
              </div>
              <pre className="mt-3 rounded-xl theme-bg-sub border theme-border px-3.5 py-3 text-[13px] font-mono theme-text-main whitespace-pre-wrap leading-relaxed">
                {registerPrompt}
              </pre>
            </section>

            <div className="mt-6 flex items-center justify-between gap-2">
              <button
                type="button"
                className={actionBtnClass({ variant: 'ghost' })}
                onClick={() =>
                  void api.openUrl(
                    'https://raw.githubusercontent.com/lepfinder/AgentDeck/main/skills/agentdeck-services/README.md',
                  )
                }
              >
                <ExternalLink className="mr-1 h-3.5 w-3.5" />
                打开安装说明
              </button>
              <button
                type="button"
                className={actionBtnClass({ variant: 'default' })}
                onClick={() => setAddHelpOpen(false)}
              >
                关闭
              </button>
            </div>
          </div>
        </div>
      )}

      {(actionMessage || error) && (
        <div className="fixed bottom-8 right-6 z-40 flex max-w-sm items-start gap-2 px-4 py-2.5 bg-slate-900/90 dark:bg-slate-800/95 text-white text-xs font-medium rounded-xl shadow-2xl border border-white/10 backdrop-blur-md pointer-events-none">
          {error ? (
            <AlertCircle className="h-4 w-4 text-red-400 flex-shrink-0 mt-0.5" />
          ) : /失败|错误|冲突|无法/.test(actionMessage || '') ? (
            <AlertCircle className="h-4 w-4 text-amber-400 flex-shrink-0 mt-0.5" />
          ) : /启动|停止|重启|稍候|进行中/.test(actionMessage || '') ? (
            <Loader2 className="h-4 w-4 text-blue-400 flex-shrink-0 mt-0.5 animate-spin" />
          ) : (
            <CheckCircle2 className="h-4 w-4 text-emerald-400 flex-shrink-0 mt-0.5" />
          )}
          <span className="leading-relaxed">{error || actionMessage}</span>
        </div>
      )}

      {deleteTarget && (
        <div
          className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 p-4"
          onClick={() => setDeleteTarget(null)}
        >
          <div
            className="w-full max-w-md rounded-2xl border theme-border theme-bg-main p-5 shadow-2xl"
            onClick={(e) => e.stopPropagation()}
          >
            <h3 className="text-sm font-bold theme-text-main">删除服务？</h3>
            <p className="mt-2 text-sm theme-text-muted">
              将停止「{deleteTarget.name}」并从列表中移除，不会删除项目目录或源码。
            </p>
            <div className="mt-5 flex justify-end gap-2">
              <button
                type="button"
                className={actionBtnClass({ variant: 'outline' })}
                onClick={() => setDeleteTarget(null)}
              >
                取消
              </button>
              <button
                type="button"
                className="inline-flex h-8 items-center rounded-lg bg-red-600 px-3 text-xs font-medium text-white hover:bg-red-700 cursor-pointer"
                onClick={() => {
                  const id = deleteTarget.id;
                  setDeleteTarget(null);
                  void removeService(id);
                }}
              >
                删除
              </button>
            </div>
          </div>
        </div>
      )}

      {renameTarget && (
        <div
          className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 p-4"
          onClick={() => setRenameTarget(null)}
        >
          <div
            className="w-full max-w-md rounded-2xl border theme-border theme-bg-main p-5 shadow-2xl"
            onClick={(e) => e.stopPropagation()}
          >
            <h3 className="text-sm font-bold theme-text-main">
              {renameTarget.kind === 'project' ? '重命名项目' : '重命名服务'}
            </h3>
            <p className="mt-1 text-xs theme-text-muted">
              仅修改展示名称，不会改动 id / 路径 / 启动命令。
            </p>
            <input
              ref={renameInputRef}
              type="text"
              value={renameDraft}
              onChange={(e) => setRenameDraft(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === 'Enter') {
                  e.preventDefault();
                  void submitRename();
                }
                if (e.key === 'Escape') setRenameTarget(null);
              }}
              className="mt-4 w-full rounded-lg border theme-border theme-bg-sub px-3 py-2 text-sm theme-text-main outline-none focus:ring-2 focus:ring-blue-500/40"
              placeholder={renameTarget.kind === 'project' ? '项目名称' : '服务名称'}
            />
            <div className="mt-5 flex justify-end gap-2">
              <button
                type="button"
                className={actionBtnClass({ variant: 'outline' })}
                onClick={() => setRenameTarget(null)}
              >
                取消
              </button>
              <button
                type="button"
                className={actionBtnClass({
                  variant: 'default',
                  disabled: !renameDraft.trim(),
                })}
                disabled={!renameDraft.trim()}
                onClick={() => void submitRename()}
              >
                保存
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
