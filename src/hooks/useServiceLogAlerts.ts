import { useCallback, useEffect, useRef, useState } from 'react';
import { analyzeServiceLogErrors } from '../utils/formatServiceLog';
import type { ServiceStatus } from './useLocalServices';

export interface ServiceLogAlert {
  latestAtMs: number | null;
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
 * Poll live services' logs, surface latest error time for the nav tree icon.
 */
export function useServiceLogAlerts(
  services: ServiceStatus[],
  tailLog: (id: string, lines?: number) => Promise<string>,
) {
  const [alerts, setAlerts] = useState<Record<string, ServiceLogAlert>>({});
  const servicesRef = useRef(services);
  servicesRef.current = services;
  // Re-entrancy guard: overlapping polls coalesce into one trailing rescan.
  const scanningRef = useRef(false);
  const rescanNeededRef = useRef(false);

  const scan = useCallback(async () => {
    if (scanningRef.current) {
      rescanNeededRef.current = true;
      return;
    }
    scanningRef.current = true;
    try {
      const list = servicesRef.current.filter(isWatchable);
      if (list.length === 0) {
        setAlerts({});
        return;
      }

      const entries = await Promise.all(
        list.map(async (svc) => {
          try {
            const raw = await tailLog(svc.id, 200);
            const { errorIndices, latestAtMs } = analyzeServiceLogErrors(raw);
            if (errorIndices.length === 0) {
              return [svc.id, null] as const;
            }
            return [svc.id, { latestAtMs } satisfies ServiceLogAlert] as const;
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
    } finally {
      scanningRef.current = false;
    }
    if (rescanNeededRef.current) {
      rescanNeededRef.current = false;
      void scan();
    }
  }, [tailLog]);

  useEffect(() => {
    void scan();
    const poll = window.setInterval(() => void scan(), 4000);
    return () => window.clearInterval(poll);
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

  return { alerts };
}
