const STORAGE_KEY = 'agentdeck.serviceLogIgnored.v1';
const MAX_SIGS_PER_SERVICE = 200;

type Persisted = Record<string, string[]>;

/** In-memory cache so ignore survives ServicesView unmount without waiting on disk. */
let memory: Record<string, Set<string>> | null = null;

function readDisk(): Persisted {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (!raw) return {};
    const parsed = JSON.parse(raw) as unknown;
    if (!parsed || typeof parsed !== 'object') return {};
    const out: Persisted = {};
    for (const [id, sigs] of Object.entries(parsed as Record<string, unknown>)) {
      if (!Array.isArray(sigs)) continue;
      out[id] = sigs.filter((s): s is string => typeof s === 'string' && s.trim().length > 0);
    }
    return out;
  } catch {
    return {};
  }
}

function writeDisk(map: Record<string, Set<string>>) {
  const out: Persisted = {};
  for (const [id, set] of Object.entries(map)) {
    if (set.size === 0) continue;
    out[id] = Array.from(set).slice(-MAX_SIGS_PER_SERVICE);
  }
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(out));
  } catch {
    // quota / private mode — memory still works for this session
  }
}

function ensureMemory(): Record<string, Set<string>> {
  if (memory) return memory;
  const disk = readDisk();
  memory = {};
  for (const [id, sigs] of Object.entries(disk)) {
    memory[id] = new Set(sigs);
  }
  return memory;
}

export function getIgnoredSignatures(serviceId: string): Set<string> {
  const map = ensureMemory();
  return map[serviceId] ?? new Set();
}

export function addIgnoredSignatures(serviceId: string, signatures: string[]): Set<string> {
  const map = ensureMemory();
  const set = map[serviceId] ?? new Set<string>();
  for (const sig of signatures) {
    const t = sig.trim();
    if (t) set.add(t);
  }
  // Bound growth: keep most recent insertions by rebuilding from array tail
  if (set.size > MAX_SIGS_PER_SERVICE) {
    const trimmed = Array.from(set).slice(-MAX_SIGS_PER_SERVICE);
    map[serviceId] = new Set(trimmed);
  } else {
    map[serviceId] = set;
  }
  writeDisk(map);
  return map[serviceId];
}

export function clearIgnoredSignatures(serviceId: string) {
  const map = ensureMemory();
  delete map[serviceId];
  writeDisk(map);
}
