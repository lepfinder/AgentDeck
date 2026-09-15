import { useState, useCallback, useEffect, useMemo } from 'react';
import { listen } from '@tauri-apps/api/event';
import { api } from '../api/tauriBridge';

export interface PortStatus {
  port: number;
  listening: boolean;
}

export interface ServiceMeta {
  id: string;
  projectId: string;
  serviceId: string;
  projectName: string;
  name: string;
  ports: number[];
  openUrl: string;
  projectDir: string;
}

export interface ServiceStatus {
  id: string;
  projectId: string;
  serviceId: string;
  projectName: string;
  name: string;
  state:
    | 'stopped'
    | 'running'
    | 'starting'
    | 'stopping'
    | 'unhealthy'
    | 'stale_pid'
    | 'port_conflict'
    | 'partial';
  pid: number | null;
  ports: PortStatus[];
  health: 'ok' | 'ports_ok' | 'no_response' | 'unknown';
  openUrl: string;
  logFile: string;
  projectDir?: string;
  startedAt?: string;
  uptimeSecs?: number;
  lastError?: string;
  extras?: Record<string, string>;
}

export interface ServiceProjectGroup {
  projectId: string;
  projectName: string;
  projectDir: string;
  services: ServiceStatus[];
}

export interface InstalledIdes {
  cursor: boolean;
  antigravity: boolean;
}

export interface ServiceActionResult {
  success: boolean;
  message: string;
}

function placeholderFromMeta(meta: ServiceMeta): ServiceStatus {
  return {
    id: meta.id,
    projectId: meta.projectId,
    serviceId: meta.serviceId,
    projectName: meta.projectName,
    name: meta.name,
    state: 'stopped',
    pid: null,
    ports: meta.ports.map((port) => ({ port, listening: false })),
    health: 'unknown',
    openUrl: meta.openUrl,
    logFile: '',
    projectDir: meta.projectDir,
  };
}

export function groupServicesByProject(services: ServiceStatus[]): ServiceProjectGroup[] {
  const map = new Map<string, ServiceProjectGroup>();
  for (const svc of services) {
    const key = svc.projectId || svc.projectDir || svc.id;
    let group = map.get(key);
    if (!group) {
      group = {
        projectId: svc.projectId,
        projectName: svc.projectName || svc.name,
        projectDir: svc.projectDir || '',
        services: [],
      };
      map.set(key, group);
    }
    group.services.push(svc);
  }
  return Array.from(map.values()).sort((a, b) => a.projectName.localeCompare(b.projectName));
}

export function useLocalServices() {
  const [services, setServices] = useState<ServiceStatus[]>([]);
  const [refreshing, setRefreshing] = useState(false);
  const [initializing, setInitializing] = useState(true);
  const [actionLoading, setActionLoading] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [actionMessage, setActionMessage] = useState<string | null>(null);
  const [ides, setIdes] = useState<InstalledIdes>({ cursor: false, antigravity: false });

  const projectGroups = useMemo(() => groupServicesByProject(services), [services]);

  const refresh = useCallback(async () => {
    setRefreshing(true);
    try {
      const list = await api.services.status();
      setServices(list);
      setError(null);
    } catch (err) {
      console.error('[useLocalServices] refresh failed:', err);
      setError(err instanceof Error ? err.message : '加载服务状态失败');
    } finally {
      setRefreshing(false);
      setInitializing(false);
    }
  }, []);

  useEffect(() => {
    let cancelled = false;

    const bootstrap = async () => {
      try {
        const metas = await api.services.list();
        if (!cancelled && metas.length > 0) {
          setServices(metas.map(placeholderFromMeta));
          setInitializing(false);
        }
      } catch (err) {
        console.error('[useLocalServices] list failed:', err);
      }
      if (!cancelled) {
        await refresh();
      }
    };

    void bootstrap();
    api.services
      .detectIdes()
      .then((detected) => setIdes(detected))
      .catch((err: unknown) => console.error('[useLocalServices] detectIdes failed:', err));
    const interval = setInterval(() => void refresh(), 5000);
    return () => {
      cancelled = true;
      clearInterval(interval);
    };
  }, [refresh]);

  const hasPending = services.some((s) => s.state === 'starting' || s.state === 'stopping');

  useEffect(() => {
    if (!hasPending) return;
    const fast = setInterval(() => void refresh(), 2000);
    return () => clearInterval(fast);
  }, [hasPending, refresh]);

  useEffect(() => {
    let unlisten: (() => void) | null = null;
    listen<{ id: string; success: boolean; message: string }>('service:action-complete', (event) => {
      setActionMessage(event.payload.message);
      void refresh();
    }).then((fn) => {
      unlisten = fn;
    });
    return () => {
      if (unlisten) unlisten();
    };
  }, [refresh]);

  useEffect(() => {
    if (!actionMessage) return;
    const timer = setTimeout(() => setActionMessage(null), 6000);
    return () => clearTimeout(timer);
  }, [actionMessage]);

  const runAction = useCallback(
    async (key: string, fn: () => Promise<ServiceActionResult>) => {
      setActionLoading(key);
      try {
        const result = await fn();
        setActionMessage(result.message);
        void refresh();
        return result;
      } finally {
        setActionLoading(null);
      }
    },
    [refresh]
  );

  const startService = useCallback(
    (id: string) => runAction(`start-${id}`, () => api.services.start(id)),
    [runAction]
  );

  const stopService = useCallback(
    (id: string, force = false) => runAction(`stop-${id}`, () => api.services.stop(id, force)),
    [runAction]
  );

  const restartService = useCallback(
    (id: string) => runAction(`restart-${id}`, () => api.services.restart(id)),
    [runAction]
  );

  const openService = useCallback(async (id: string) => {
    setActionLoading(`open-${id}`);
    try {
      await api.services.open(id);
    } catch (err) {
      console.error('[useLocalServices] open failed:', err);
      throw err;
    } finally {
      setActionLoading(null);
    }
  }, []);

  const openInIde = useCallback(async (path: string, ide: 'cursor' | 'antigravity') => {
    try {
      await api.services.openInIde(path, ide);
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err);
      setActionMessage(message);
      throw err;
    }
  }, []);

  const removeService = useCallback(
    (id: string) => runAction(`remove-${id}`, () => api.services.remove(id)),
    [runAction]
  );

  const renameProject = useCallback(
    (projectId: string, name: string) =>
      runAction(`rename-project-${projectId}`, () => api.services.renameProject(projectId, name)),
    [runAction]
  );

  const renameService = useCallback(
    (id: string, name: string) =>
      runAction(`rename-service-${id}`, () => api.services.renameService(id, name)),
    [runAction]
  );

  const tailLog = useCallback((id: string, lines = 30) => api.services.tailLog(id, lines), []);

  return {
    services,
    projectGroups,
    loading: initializing,
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
  };
}
