// Terminal builds watch list (card §15.3, UX packet S2.1–S2.2). Mirrors src-tauri/src/adapters/build.rs.
export type WatchKind = 'process' | 'command'
export type WatchEntry = { match: string; kind: WatchKind }

// Card §15.3: `.exe` entries are process (`msbuild.exe`, `tsc.exe`), the rest whole-word commands.
// Matching itself happens in Rust (adapters/build.rs).
export const DEFAULT_WATCH: WatchEntry[] = [
  ...['cargo build', 'cargo run', 'npm run build', 'pnpm build', 'yarn build', 'gradle', 'dotnet build'].map((m) => ({ match: m, kind: 'command' as const })),
  { match: 'msbuild.exe', kind: 'process' },
  { match: 'tsc.exe', kind: 'process' },
  { match: 'vite build', kind: 'command' },
]

/** S2.5 TBD 2: the Add picker defaults to Command. */
export const DEFAULT_ADD_KIND: WatchKind = 'command'

const SHELLS = ['cmd', 'powershell', 'pwsh', 'bash', 'node']

/** Blocked with or without `.exe` (S2.5 TBD 3); only for kind `process`. */
export function blockedShell(e: WatchEntry): boolean {
  if (e.kind !== 'process') return false
  const p = e.match.trim().toLowerCase()
  return SHELLS.includes(p.endsWith('.exe') ? p.slice(0, -4) : p)
}

const key = (e: WatchEntry) => `${e.kind}:${e.match.trim().toLowerCase()}`

export type AddResult = { ok: true; list: WatchEntry[] } | { ok: false; message: string }

export function addEntry(list: WatchEntry[], text: string, kind: WatchKind): AddResult {
  const e = { match: text.trim(), kind }
  if (!e.match) return { ok: false, message: 'Enter a process name or command.' }
  if (list.some((x) => key(x) === key(e))) return { ok: false, message: "That's already on the list." }
  if (blockedShell(e)) return { ok: false, message: `Shells like ${e.match} can't be watched. They stay open, so the whiteout would never clear. Add the build command instead (e.g. npm run build).` }
  return { ok: true, list: [...list, e] }
}

/** Loaded settings: silently trim, drop empty/blocked/malformed entries and duplicates. Missing → defaults. */
export function normalizeWatch(raw: unknown): WatchEntry[] {
  if (!Array.isArray(raw)) return DEFAULT_WATCH.map((e) => ({ ...e }))
  const seen = new Set<string>()
  const out: WatchEntry[] = []
  for (const r of raw) {
    if (!r || typeof r !== 'object') continue
    const { match, kind } = r as Partial<WatchEntry>
    if (typeof match !== 'string' || (kind !== 'process' && kind !== 'command')) continue
    const e = { match: match.trim(), kind }
    if (!e.match || blockedShell(e) || seen.has(key(e))) continue
    seen.add(key(e)); out.push(e)
  }
  return out
}

export const sameAsDefaults = (list: WatchEntry[]) =>
  list.length === DEFAULT_WATCH.length && list.every((e, i) => key(e) === key(DEFAULT_WATCH[i]))
