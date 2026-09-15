import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { analyzeServiceLogErrors } from '../utils/formatServiceLog';
import {
  addIgnoredSignatures,
  clearIgnoredSignatures,
  getIgnoredSignatures,
} from '../lib/serviceLogIgnoreStore';
import type { ServiceStatus } from './useLocalServices';

export interface ServiceLogAlert {
  count: number;
  latestAtMs: number | null;
  latestLine: string | null;
}

function isWatchable(svc: ServiceStatus): boolean {
  return (
    svc.state === 'running' ||
    svc.state === 'partial' ||
    svc.state === 'unhealthy' ||
    svc.state === 'starting' ||
    svc.state === 'stopping'
  );
}

/**
 * Poll live services' logs, surface error counts + latest error time for the nav tree.
 * Ignore snapshots persist in localStorage (+ memory) across page switches.
 */
export function useServiceLogAlerts(
  services: ServiceStatus[],
  tailLog: (id: string, lines?: number) => Promise<string>,
) {
  const [alerts, setAlerts] = useState<Record<string, ServiceLogAlert>>({});
  const [nowMs, setNowMs] = useState(() => Date.now());
  const [ignoreEpoch, setIgnoreEpoch] = useState(0);
  const servicesRef = useRef(services);
  servicesRef.current = services;

  const scan = useCallback(async () => {
    const list = servicesRef.current.filter(isWatchable);
    if (list.length === 0) {
      setAlerts({});
      return;
    }

    const entries = await Promise.all(
      list.map(async (svc) => {
        try {
          const raw = await tailLog(svc.id, 200);
          const ignored = getIgnoredSignatures(svc.id);
          const { errorIndices, latestAtMs, latestLine } = analyzeServiceLogErrors(raw, {
            ignoredSignatures: ignored.size > 0 ? ignored : undefined,
          });
          if (errorIndices.length === 0) {
            return [svc.id, null] as const;
          }
          return [
            svc.id,
            {
              count: errorIndices.length,
              latestAtMs,
              latestLine,
            } satisfies ServiceLogAlert,
          ] as const;
        } catch {
          return [svc.id, null] as const;
        }
      }),
    );

    const next: Record<string, ServiceLogAlert> = {};
    for (const [id, alert] of entries) {
      if (alert) next[id] = alert;
    }
    setAlerts(next);
  }, [tailLog]);

  useEffect(() => {
    void scan();
    const poll = window.setInterval(() => void scan(), 4000);
    const tick = window.setInterval(() => setNowMs(Date.now()), 15000);
    return () => {
      window.clearInterval(poll);
      window.clearInterval(tick);
    };
  }, [scan]);

  useEffect(() => {
    const watchable = new Set(services.filter(isWatchable).map((s) => s.id));
    setAlerts((prev) => {
      let changed = false;
      const next: Record<string, ServiceLogAlert> = {};
      for (const [id, alert] of Object.entries(prev)) {
        if (watchable.has(id)) next[id] = alert;
        else changed = true;
      }
      return changed ? next : prev;
    });
  }, [services]);

  const ignoreServiceErrors = useCallback(
    (serviceId: string, signatures: string[]) => {
      addIgnoredSignatures(serviceId, signatures);
      setIgnoreEpoch((n) => n + 1);
      setAlerts((prev) => {
        if (!prev[serviceId]) return prev;
        const next = { ...prev };
        delete next[serviceId];
        return next;
      });
      void scan();
    },
    [scan],
  );

  const clearIgnored = useCallback(
    (serviceId: string) => {
      clearIgnoredSignatures(serviceId);
      setIgnoreEpoch((n) => n + 1);
      void scan();
    },
    [scan],
  );

  const ignoredByServiceId = useCallback(
    (serviceId: string) => new Set(getIgnoredSignatures(serviceId)),
    [ignoreEpoch],
  );

  const alertById = useMemo(() => alerts, [alerts]);

  return {
    alerts: alertById,
    nowMs,
    ignoreEpoch,
    ignoreServiceErrors,
    clearIgnored,
    ignoredByServiceId,
    rescan: scan,
  };
}
