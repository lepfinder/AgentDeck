/**
 * 本地服务 Fleet 总览 — 服务级状态卡片
 */
import type { ServiceProjectGroup, ServiceStatus } from '../../hooks/useLocalServices';
import { cn } from '../../lib/cn';

function isHealthyRunning(s: ServiceStatus): boolean {
  return s.state === 'running' && (s.health === 'ok' || s.health === 'ports_ok');
}

function isAlert(s: ServiceStatus): boolean {
  // stale_pid 视为未启动，不进入「需关注」
  return (
    s.state === 'port_conflict' ||
    s.state === 'unhealthy' ||
    (s.state === 'running' && s.health !== 'ok' && s.health !== 'ports_ok')
  );
}

function stateLabel(state: ServiceStatus['state'], health: ServiceStatus['health']): string {
  if (state === 'starting') return '启动中';
  if (state === 'stopping') return '停止中';
  if (state === 'partial') return '部分就绪';
  if (state === 'running' && (health === 'ok' || health === 'ports_ok')) return '运行中';
  if (state === 'running' || state === 'unhealthy') return '未就绪';
  if (state === 'port_conflict') return '端口冲突';
  // PID 文件过期 ≈ 进程已不在，按未启动展示
  if (state === 'stale_pid') return '未启动';
  return '未启动';
}

type ServiceTone = 'ok' | 'alert' | 'busy' | 'stopped';

function serviceTone(s: ServiceStatus): ServiceTone {
  if (isAlert(s)) return 'alert';
  if (isHealthyRunning(s)) return 'ok';
  if (s.state === 'partial' || s.state === 'starting' || s.state === 'stopping') return 'busy';
  // stopped + stale_pid 都算未启动
  return 'stopped';
}

function toneMeta(tone: ServiceTone): { card: string; badge: string; glow: string } {
  switch (tone) {
    case 'ok':
      return {
        card: 'border-emerald-500/25 hover:border-emerald-500/45 hover:bg-emerald-500/[0.04]',
        badge: 'text-emerald-700 bg-emerald-500/15 dark:text-emerald-400',
        glow: 'bg-emerald-500',
      };
    case 'alert':
      return {
        card: 'border-red-500/30 hover:border-red-500/50 hover:bg-red-500/[0.04]',
        badge: 'text-red-700 bg-red-500/15 dark:text-red-400',
        glow: 'bg-red-500',
      };
    case 'busy':
      return {
        card: 'border-amber-500/30 hover:border-amber-500/50 hover:bg-amber-500/[0.04]',
        badge: 'text-amber-700 bg-amber-500/15 dark:text-amber-400',
        glow: 'bg-amber-500',
      };
    default:
      return {
        card: 'theme-border hover:border-zinc-400/50 hover:theme-bg-sub/60',
        badge: 'theme-text-muted theme-bg-sub',
        glow: 'bg-zinc-400',
      };
  }
}

function primaryPort(svc: ServiceStatus): string | null {
  if (svc.ports.length > 0) return String(svc.ports[0].port);
  const m = svc.openUrl?.match(/:(\d+)/);
  return m?.[1] ?? null;
}

export type FleetFilter = 'all' | 'running' | 'alert' | 'stopped';

interface Props {
  projectGroups: ServiceProjectGroup[];
  services: ServiceStatus[];
  filter: FleetFilter;
  onFilterChange: (f: FleetFilter) => void;
  onSelectService: (id: string) => void;
}

export function ServicesFleetOverview({
  projectGroups,
  services,
  filter,
  onFilterChange,
  onSelectService,
}: Props): React.ReactElement {
  const running = services.filter(isHealthyRunning).length;
  const alert = services.filter(isAlert).length;
  const stopped = services.filter((s) => serviceTone(s) === 'stopped').length;

  const stats: Array<{ key: FleetFilter; label: string; count: number; activeTone: string }> = [
    {
      key: 'all',
      label: '全部',
      count: services.length,
      activeTone: 'bg-slate-800 text-white dark:bg-slate-100 dark:text-slate-900',
    },
    {
      key: 'running',
      label: '运行中',
      count: running,
      activeTone: 'bg-emerald-600 text-white',
    },
    {
      key: 'alert',
      label: '需关注',
      count: alert,
      activeTone: 'bg-red-600 text-white',
    },
    {
      key: 'stopped',
      label: '未启动',
      count: stopped,
      activeTone: 'bg-zinc-500 text-white',
    },
  ];

  const matchFilter = (s: ServiceStatus) => {
    const tone = serviceTone(s);
    if (filter === 'all') return true;
    if (filter === 'running') return tone === 'ok';
    if (filter === 'alert') return tone === 'alert';
    if (filter === 'stopped') return tone === 'stopped';
    return true;
  };

  // 按项目分组展示卡片，但粒度是服务
  const visibleGroups = projectGroups
    .map((g) => ({
      ...g,
      services: [...g.services]
        .filter(matchFilter)
        .sort((a, b) => {
          const order: Record<ServiceTone, number> = { alert: 0, busy: 1, ok: 2, stopped: 3 };
          return order[serviceTone(a)] - order[serviceTone(b)];
        }),
    }))
    .filter((g) => g.services.length > 0);

  return (
    <div className="flex-1 min-h-0 flex flex-col gap-4 overflow-hidden">
      <div className="shrink-0 flex items-center justify-between gap-3">
        <div className="inline-flex items-center rounded-full theme-bg-card border theme-border p-0.5 shadow-xs">
          {stats.map((s) => (
            <button
              key={s.key}
              type="button"
              onClick={() => onFilterChange(s.key)}
              className={cn(
                'inline-flex items-center gap-1.5 rounded-full px-3 py-1.5 text-xs transition-all cursor-pointer',
                filter === s.key ? cn(s.activeTone, 'shadow-sm') : 'theme-text-muted hover:theme-text-main'
              )}
            >
              <span className="font-mono tabular-nums font-semibold">{s.count}</span>
              <span>{s.label}</span>
            </button>
          ))}
        </div>
        <p className="text-[11px] theme-text-muted shrink-0 hidden sm:block">
          点击服务卡片进入详情
        </p>
      </div>

      <div className="flex-1 min-h-0 overflow-y-auto">
        {visibleGroups.length === 0 ? (
          <div className="flex h-full min-h-[14rem] items-center justify-center rounded-2xl border border-dashed theme-border text-sm theme-text-muted">
            当前筛选下没有服务
          </div>
        ) : (
          <div className="space-y-5 pb-1">
            {visibleGroups.map((group) => (
              <section key={group.projectId}>
                <h3 className="text-xs font-medium theme-text-muted mb-2 px-0.5">
                  {group.projectName}
                </h3>
                <div className="grid grid-cols-1 sm:grid-cols-2 xl:grid-cols-3 gap-2.5">
                  {group.services.map((svc) => {
                    const tone = serviceTone(svc);
                    const meta = toneMeta(tone);
                    const port = primaryPort(svc);
                    const showProjectPrefix =
                      group.services.length > 1 &&
                      svc.name.toLowerCase().includes(group.projectName.toLowerCase()) === false;
                    return (
                      <button
                        key={svc.id}
                        type="button"
                        onClick={() => onSelectService(svc.id)}
                        className={cn(
                          'group relative text-left rounded-2xl border theme-bg-card px-4 py-3.5 transition-all cursor-pointer',
                          'hover:shadow-md hover:-translate-y-0.5',
                          meta.card
                        )}
                      >
                        <div className="flex items-start justify-between gap-2">
                          <div className="min-w-0 flex items-center gap-2">
                            <span className={cn('h-2.5 w-2.5 rounded-full shrink-0', meta.glow)} />
                            <div className="min-w-0">
                              <div className="text-[15px] font-semibold theme-text-main truncate">
                                {svc.name}
                              </div>
                              {showProjectPrefix && (
                                <div className="text-[11px] theme-text-muted truncate mt-0.5">
                                  {group.projectName}
                                </div>
                              )}
                            </div>
                          </div>
                          <span
                            className={cn(
                              'shrink-0 rounded-full px-2 py-0.5 text-[11px] font-medium',
                              meta.badge
                            )}
                          >
                            {stateLabel(svc.state, svc.health)}
                          </span>
                        </div>

                        <div className="mt-3 flex items-center justify-between gap-2">
                          <span className="font-mono text-[11px] theme-text-muted tabular-nums">
                            {port ? `:${port}` : '—'}
                          </span>
                          <span className="text-[11px] theme-text-muted opacity-0 group-hover:opacity-100 transition-opacity">
                            详情 →
                          </span>
                        </div>
                      </button>
                    );
                  })}
                </div>
              </section>
            ))}
          </div>
        )}
      </div>
    </div>
  );
}
