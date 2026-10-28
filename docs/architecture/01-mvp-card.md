# AI Overlay: MVP architecture card (rev 2, Oct 27 2026)
Rev 2 folds in Remy's calls (9:02 PM CT) and Cheke's calls (9:07 PM CT): our own KBS verse API, and a preview installer that Cheke tries on his own PC.
Status: docs only. Based on `/workspace/apps/ai-overlay/docs/specs/01-mvp.md`, which is a draft. Nothing gets built until Cheke confirms the start in his own words (spec header).
Scope: preview build on Windows 11 and 10. No code, installs, commits, publishing, signing or spending in this card.

## 1. Stack and versions
- **Tauri 2.x** (current stable 2.x line, pinned in `Cargo.lock` at build time). Windows webview: WebView2. Windows 11 ships it. On Windows 10 it may be missing, so the preview uses Tauri's `downloadBootstrapper` mode (see Risks).
- **Rust** stable (MSRV is whatever Tauri 2 requires). Crates: `tauri`, `tauri-plugin-global-shortcut`, `tauri-plugin-store`, `windows` (Win32: `UI_WindowsAndMessaging`, `UI_Input_KeyboardAndMouse`, `System_Threading`, `UI_Accessibility` for the fallback), `sysinfo` (process list), `notify` (file watching), `serde`/`serde_json`, `reqwest` (calls the local KBS verse API only, never HelloAO), `rand` (shuffle).
- **Frontend: React 19 + TypeScript ~5.9 + Vite 7.** This matches AER Breathe (`/workspace/apps/aer-breathe/app/package.json`: react ^19.2.0, typescript ~5.9.3, vite ^7.2.4). Tailwind 3 is optional. Leave out the large Radix/shadcn set, because the overlay UI is small: a circle, a verse, a version label, and a settings panel.
- Fonts can be the same `@fontsource` packages AER Breathe uses (Cormorant Garamond, Manrope). They get bundled, so the app makes no font network calls.

## 2. Process and windows model
One Tauri process, no visible main window.
- **Tray icon** (Tauri 2 `tray` API). Menu: Settings, Pause adapters, Show whiteout now (test), Quit. It is the only entry point at rest.
- **Rust core** (main process) owns the adapter registry, the state machine, the input hook thread, timers (3 s show delay, 20 min cap, 90 s adapter-quiet auto-clear), settings and the verse cache.
- **Whiteout window(s)** (`label: whiteout-<monitor>`): `decorations:false`, `transparent:true`, `alwaysOnTop:true`, `skipTaskbar:true`, `fullscreen` sized to the monitor bounds, created hidden at startup and shown or hidden instead of recreated, so they appear within the 3 s target. The spec says "one overlay at a time" and gives a monitor setting: **all monitors** means one window per monitor, all driven by the same single session (one breathing clock, one verse); **active monitor** means one window on the monitor with the foreground window. The window renders opaque white (the spec calls it "plain white" and a "complete" whiteout). It is transparent only so the fade in and fade out can happen.
- **Settings window** (`label: settings`): normal decorated window, opened from the tray.
- The spec has no separate non-blocking "overlay" mode. The always-on-top transparent window *is* the whiteout. Any lighter status overlay is left out (open question).

State machine (Rust): `idle → working → (3 s) → whiteout_shown → {needs-you | done | panic | cap 20 min | adapter quiet} → cleared`. Clearing plays the soft chime (one built-in sound, played by the webview just before it hides; on by default at low volume, with a volume slider and a mute toggle in settings) and gives focus back to the window that had it (saved `HWND`, restored with `SetForegroundWindow`), which "drops them back to the dev tool."

## 3. Input blocking on Windows
Goal (Cheke's answer): block all typing and clicks, always with a way out.
Layered design:
1. **Full-screen topmost window that takes focus**: `HWND_TOPMOST`, `SetForegroundWindow` and `SetFocus` on show. This alone stops most clicks from reaching Cursor, but not Alt+Tab, the Win key, or apps that steal focus.
2. **Low-level hooks** (`windows` crate): `SetWindowsHookExW(WH_KEYBOARD_LL)` and `WH_MOUSE_LL`, installed only while the whiteout is up, on a dedicated thread running its own message loop. They return `1` (swallow) for every event, **except**:
   - events whose pointer is over our own whiteout window, if we want its exit button clickable. Simpler: pass mouse input through only when the foreground window is our whiteout, and swallow it when the foreground window is anything else;
   - the **panic hotkey combo**, which the hook spots itself (it tracks modifier state) and then clears the whiteout directly. Even if global-shortcut registration fails or another app takes the combo, the hook still lets the user out.
2b. **Panic hotkey: Ctrl+Alt+Shift+Esc** by default (Remy), changeable in settings. Settings rejects combos that Windows reserves or that the hook can't see (anything with Ctrl+Alt+Del, Win+L).
3. **Global shortcut** (`tauri-plugin-global-shortcut`) registers the same panic combo as a second path, for when the hook is not installed (for example in the "active monitor" mode while the hook is disabled).
4. **Failsafes**: the 20-minute hard cap (spec), adapter-quiet auto-clear after 90 s (Remy), a watchdog that removes the hooks if the UI thread stops answering for more than N seconds, and `UnhookWindowsHookEx` on every exit path, including panic and drop. Windows itself drops LL hooks that go past `LowLevelHooksTimeout`, so the hook callback has to stay trivial: no I/O, no locks it could wait on.
5. **Exit button** on the whiteout (Cheke's answer). It works because clicks to our own window are let through.

Limits we accept and document:
- **Ctrl+Alt+Del** (the Secure Attention Sequence) and **Win+L** cannot be blocked from user mode. That is fine: they are extra escape routes. After unlock or the secure desktop, the whiteout is still up, and the hook may have been dropped, so we re-check and reinstall it on `WM_WTSSESSION_CHANGE`.
- Windows elevated above us (UAC prompts, Task Manager run as admin) do not get our hooks (UIPI). We run unelevated.
- Some Win-key combos are handled by the shell before LL hooks can stop them. Swallowing VK_LWIN/VK_RWIN covers most of them.

## 4. The Cursor adapter
Interface (Rust trait, in `src-tauri/src/adapters/mod.rs`):
```rust
enum ToolState { Idle, Working, NeedsYou, Done }
trait Adapter: Send {
    fn id(&self) -> &'static str;            // "cursor"
    fn display_name(&self) -> &'static str;  // for per-app on/off
    fn is_target_running(&self) -> bool;
    fn poll(&mut self) -> Option<ToolState>; // or push via channel
    fn last_signal_at(&self) -> Instant;     // drives "adapter goes quiet" auto-clear
}
```
Later adapters (Claude, Grok Bot, browser extension, per spec "Later") plug into the same registry. Per-app on/off maps to `adapter.id()`.

Detection (Remy's call): Sol and Dex pick the events from **Cursor's documented hooks and logs only** (no undocumented internals). Mapping:
- **Working** = an agent run has started.
- **NeedsYou** = an accept or approval prompt is up.
- **Done** = the run has ended.
Which documented hook or log event carries each of these, and its path and format on Windows, is spike **S1** (done on a Windows CI runner or VM, not on Cheke's PC). The spike's output is a mapping table checked into `docs/architecture/`.
- **Primary:** a `CursorAdapter` that consumes those documented hook outputs or logs (file watcher via `notify`, or a hook script that writes events to a local file or pipe) and maps them to the states above.
- **Gate:** process presence (`sysinfo`, for example `Cursor.exe`) only enables or disables the adapter. It never sets `Working` by itself.
- **Quiet:** if `Working` and no signal for **90 s**, the whiteout auto-clears (Remy).
- **Fallback (opt-in, off by default):** UI Automation through the `windows` crate on the Cursor window. Screen reading (OCR) is left out of the MVP unless Cheke asks for it.
- "Clears on every accept prompt" means `NeedsYou` is raised for every accept prompt the source reports.

## 5. Reusing useBreathing.ts
Source: `/workspace/apps/aer-breathe/app/src/hooks/useBreathing.ts`. It exports `PhaseKind` (`inhale|hold|exhale`), `BreathPhase {kind,duration,label}`, `BreathPattern {id,name,note,phases}`, `PATTERNS` (coherence 5/5; calm 4-4-6-2; box 4-4-4-4), the pure `getBreathValue`, and the `useBreathing(pattern, durationMinutes)` hook. The hook is driven by `requestAnimationFrame`, keeps a `breathRef` value from 0 to 1, and exposes `setOnPhaseChange`.
- **Decision: copy, don't share.** These are separate apps with no monorepo tooling. Copy the file into `ai-overlay/src/breathing/useBreathing.ts` with a header comment naming the source path and the date. Changes allowed: (a) move `PATTERNS` out into `patterns.ts` and add the three new exercises; (b) the overlay runs with no session end, so pass `durationMinutes = 20` (the cap) or `Infinity`.
- **React-free?** Not required, because the frontend is React. It does help testing, though, so split it: pure `breath-engine.ts` (types, patterns, `getBreathValue`, a `step(state, delta)` function lifted out of the `tick` body) plus a thin `useBreathing` React wrapper. The step model stays unchanged, as the spec asks.
- **Verse sync:** `setOnPhaseChange(kind)` fires on every phase change. The verse scheduler listens for `kind === 'inhale'`. Note: the hook's `lastPhaseRef` starts as `'inhale-0'`, so the *first* inhale doesn't fire. Verses show right away (Cheke, Done when #4), so verse 1 is shown on `toggle()` start, not on a callback. To make that instant, the scheduler fetches verse 1 from the verse API when the adapter reports `Working`, during the 3 s show delay.
- Reduced motion: still run the engine (so the labels and the verse timing keep working), but render a static circle or label in place of the scaling.

## 6. The 3 new exercises (same `BreathPattern` shape)
```ts
{ id: '478', name: '4-7-8', note: '…', phases: [
  { kind: 'inhale', duration: 4, label: 'Breathe in' },
  { kind: 'hold',   duration: 7, label: 'Hold softly' },
  { kind: 'exhale', duration: 8, label: 'Breathe out' } ] },
{ id: 'extended-exhale', name: 'Extended exhale', note: '…', phases: [
  { kind: 'inhale', duration: 4, label: 'Breathe in' },
  { kind: 'exhale', duration: 8, label: 'Breathe out' } ] },
{ id: 'sigh', name: 'Physiological sigh', note: '…', phases: [
  { kind: 'inhale', duration: 2, label: 'Breathe in' },
  { kind: 'inhale', duration: 1, label: 'Top up' },
  { kind: 'exhale', duration: 6, label: 'Long sigh out' } ] },
```
The `note` text and the labels for these three aren't in the spec, so they are placeholders (open question).
Edge case: in the physiological sigh, the second `inhale` restarts `getBreathValue` at 0 (`smooth(p)`), so the circle would drop back down. Fix this in the engine copy with an additive inhale: if the previous phase was an inhale, ease from the current value toward 1. Also, `onPhaseChange('inhale')` fires twice per cycle. The verse scheduler should only react to an inhale at `phaseIndex === 0`.

## 7. KBS verse API (Cheke's call, 9:07 PM CT)
**Background:** there is no hosted King Bible Study API. The Android app (`/workspace/apps/bible-study/`) has bundled curated chains in `app/src/main/assets/chains/`: `golden.json` (10 chains, 200 steps), `prayer.json` (1 chain, 54 steps), `theme.json` (7 chains, 225 steps). That's 479 steps and 372 unique refs, 162 of them ranges like `EPH.2.8-9`. Shape: `[{id, kind, title, source:"kbs-curated", steps:[{ref, topics}]}]`. The text comes at runtime from HelloAO (`HelloAoApi.kt`, `https://bible.helloao.org/api/{translationId}/{BOOK}/{chapter}.json`). Linked and Cross chains are built at runtime from HelloAO's cross-ref data and are not in the assets, so **"KBS chains minus Linked and Cross" = exactly these three files.**

So we build our own small **KBS verse API** over those three files. **The overlay calls only this API, never HelloAO.**

### 7.1 Endpoints (HTTP/JSON, read-only, GET)
- `GET /v1/verse/random?translation=<abbr|any>&exclude=<ref,…>` returns a random chain ref. `translation=any` (default) means the API picks; the overlay normally passes an explicit one (round-robin, below). `exclude` lets a client avoid recent refs; the overlay does its own no-repeat shuffle anyway.
- `GET /v1/verse/{ref}?translation=<abbr>` returns one ref, e.g. `/v1/verse/EPH.2.8-9?translation=KJV`. Only refs in the three chains are served (404 otherwise), since this is a chains API, not a whole Bible.
- `GET /v1/refs` returns all 372 refs (the overlay shuffles these locally, so the shuffle and no-repeat rule are testable on the client).
- `GET /v1/translations` returns the 13 translations: `{abbr, name, testaments}`.
- `GET /v1/health` returns `{ok, bundleVersion, refs, translations}`.

Response for a verse:
```json
{ "ref": "EPH.2.8-9", "display": "Ephesians 2:8-9",
  "text": "For by grace are ye saved through faith; …",
  "translation": { "abbr": "KJV", "name": "King James Version" },
  "source": "Free Use Bible API (HelloAO)" }
```
Errors: `400` unknown translation, `404` ref not in chains, `422` JPS asked for an NT ref (JPS is OT only). Ranges are joined with a space, the same as `CrossRefLoader` does.

**Translations (all 13, from `enum class Translation` in `data/Models.kt`):** KJV `eng_kjv` · WEB `ENGWEBP` · ASV `eng_asv` · Darby `eng_dby` · YLT `eng_ylt` · BSB `BSB` · GNV `eng_gnv` · Webster `eng_wbs` · RV `eng_rv5` · BBE `eng_bbe` · DRA `eng_dra` · WEBU `eng_webu` · JPS `eng_jps` (OT only). The API owns the abbr-to-full-name table, so the overlay just shows what it gets back.

### 7.2 Data source: pre-built verse bundle (chosen)
Two options: (a) the API fetches from HelloAO at request time and caches to disk, or (b) a **pre-built bundle**. **Pick (b)**, because:
- **"Everything stays on the device"** (spec): with a bundle, the running app makes zero network calls. Option (a) would make the preview hit HelloAO on Cheke's PC.
- **Size is small:** 372 refs × 13 translations ≈ 4,800 entries (fewer, since JPS skips NT refs). That's a few MB of JSON before compression.
- **Verse 1 has to show right away:** a local lookup is instant, a cold HelloAO chapter fetch isn't.
- **Works offline first time, deterministic tests,** and no dependency on HelloAO uptime.

How it's built: a build script in the API project (`scripts/build-bundle`) reads the three chain JSONs straight from `/workspace/apps/bible-study/app/src/main/assets/chains/`, fetches each needed `(translation, book, chapter)` from HelloAO **once at build time**, pulls out the verse or range (port of `HelloAoApi.verseText` plus the range join), and writes `data/verses.bundle.json` (plus `manifest.json`: chain file hashes, HelloAO fetch date, counts). Missing verses are listed in the manifest and fail the build if above zero, except JPS-for-NT, which is expected. Re-run by hand when the chains change. The fetched chapter JSONs are kept in a **build-time disk cache** (`.cache/helloao/{translation}/{BOOK}/{ch}.json`, git-ignored) so rebuilds don't refetch.
**Runtime disk cache:** the API loads the bundle into memory at start. It keeps a small disk cache under `%LOCALAPPDATA%\<app-id>\kbs-verse-api\` with the unpacked bundle and its version, so start-up after the first run is fast. There's no runtime HelloAO fallback in the preview (it can be added later behind a setting).

### 7.3 Where it lives
`/workspace/apps/kbs-verse-api/` as its **own project**, not inside ai-overlay, because Cheke framed it as a KBS API that other apps (the Android app later, Mac build) can use. Rust (`axum` + `serde`) so it can ship as a single small Windows `.exe`:
```
/workspace/apps/kbs-verse-api/
  README.md  Cargo.toml
  src/main.rs (server)  src/lib.rs (bundle load, lookup, random)  src/refs.rs (ref parse, display names)
  scripts/build-bundle.rs   (build-time only, the only code that talks to HelloAO)
  data/verses.bundle.json  data/manifest.json  data/SOURCE.md
  tests/
```

### 7.4 How it runs and how the overlay talks to it
- **Preview: Tauri sidecar.** The API `.exe` is bundled via Tauri `bundle.externalBin` and started by the overlay at launch (`tauri-plugin-shell` sidecar). It binds **`127.0.0.1` only** on a random free port, and prints `{port, token}` on stdout once; the overlay reads that. Every request carries `Authorization: Bearer <token>` so other local apps can't use it. The overlay kills it on quit; the sidecar also exits if its parent goes away.
- **Dev option:** run it as a local service (`cargo run` in `kbs-verse-api/`, fixed port 8787); the overlay takes an env var `KBS_VERSE_API_URL` in dev builds only.
- **Overlay side:** the Rust core owns the HTTP client (`reqwest` to `http://127.0.0.1:<port>`) and exposes Tauri commands `verse_next()` / `verse_get(ref, translation)` to the webview. The webview never makes network calls (tight CSP). On start it calls `/v1/refs` and `/v1/translations` once, then prefetches the next 2 verses so changes on an inhale are instant.
- **Not hosted publicly** without Cheke's OK: no deploys, no domains, no spending.

### 7.5 Overlay verse rules
- Shuffle the 372 refs (Fisher–Yates), no repeats until all are used, then reshuffle.
- Translation round-robin over all 13, skipping JPS for NT refs (same rule as `TranslationSelection.effective`).
- **Version label** under the verse: `abbr · name` from the API response (e.g. "KJV · King James Version"), updated at the same moment as the verse. Credits line: "Free Use Bible API (HelloAO)", from the response `source`.
- **Timing:** verse 1 right away at breathing start (Cheke; Done when #4), then every 45–60 s (random in range), each change waiting for the next cycle-start inhale. The spec's older "after 30 seconds" line in Must ship is superseded.
- If the API is down (sidecar crashed), the overlay restarts it once and meanwhile shows a short bundled fallback verse (KJV John 14:27) so the whiteout is never blank.

## 8. Settings and storage
`tauri-plugin-store`: `%APPDATA%\<app-id>\settings.json`, local only.
```json
{ "exercise": "coherence", "verseIntervalSec": [45, 60], "verseFirstAtStart": true,
  "adapters": { "cursor": true }, "fallbackAccessibility": false,
  "dustTheme": false, "reducedMotion": "system|on|off",
  "monitors": "all|active", "panicHotkey": "Ctrl+Alt+Shift+Esc",
  "adapterQuietSec": 90, "chime": { "muted": false, "volume": 0.25 } }
```
Defaults from the spec: dust off, plain white. Reduced motion follows the Windows "Show animations" setting through `prefers-reduced-motion`, with a manual override. The 20-minute cap and the 90 s quiet timeout are fixed in the MVP, not shown in settings. Settings shows the panic hotkey (editable) and the chime volume slider and mute toggle. No telemetry, no accounts.

## 9. File and folder layout (planned, not created)
```
/workspace/apps/ai-overlay/
  docs/specs/01-mvp.md                 (exists)
  docs/architecture/01-mvp-card.md     (this card)
  package.json  vite.config.ts  tsconfig.json  index.html
  src/
    main.tsx  App.tsx
    windows/Whiteout.tsx  windows/Settings.tsx
    breathing/breath-engine.ts  breathing/patterns.ts  breathing/useBreathing.ts
    verses/scheduler.ts
    theme/DustField.tsx
    assets/chime.(ogg|wav)
  src-tauri/
    Cargo.toml  tauri.conf.json  capabilities/default.json
    binaries/kbs-verse-api-x86_64-pc-windows-msvc.exe   (sidecar, built from ../kbs-verse-api)
    src/main.rs  src/tray.rs  src/state.rs  src/windows.rs
    src/input_block.rs        (LL hooks + watchdog)
    src/adapters/mod.rs  src/adapters/cursor.rs  src/adapters/uia_fallback.rs
    src/verses/{client.rs,sidecar.rs}   (talks only to the local KBS verse API)
  tests/ (frontend unit)  src-tauri/tests/ (Rust)
  .github/workflows/preview-windows.yml   (Windows CI build of the unsigned preview installer)
/workspace/apps/kbs-verse-api/          (separate project, see 7.3)
```

## 10. Off-limits and how Cheke gets the preview
Off-limits: no publishing, no release, no code signing, no store listing (Microsoft Store or winget), no public hosting of the verse API, no paid APIs, no spending, all without Cheke's OK. Repo work stays off his computers (spec Constraints).

**Preview delivery (Cheke's call):** Dex hands Cheke an **unsigned preview Windows installer**, as a private link or file. Cheke installs and tries it on his own PC himself. Nobody installs it on his computers for him and no repo work happens there. It is not published anywhere.

**How the installer gets built (pick: Windows CI):**
- **Chosen: a GitHub Actions `windows-latest` runner** in a private repo (or a Windows cloud agent, if that's what Dex has). It runs `tauri build` producing an NSIS `-setup.exe` (and optionally an MSI), with WebView2 `downloadBootstrapper`, and builds the `kbs-verse-api` sidecar `.exe` in the same job. The installer is uploaded as a **workflow artifact** (short retention), **not** a GitHub Release. Windows also runs the Rust tests and a smoke test there. This is the realistic path: Tauri's official guidance is to build Windows bundles on Windows; MSVC, WebView2 and the Win32 code all build natively.
- **Not chosen: cross-building from Linux** (`cargo-xwin` + NSIS). Tauri supports it only as experimental, NSIS only (no MSI), and it still can't run or test the hooks, UIA or WebView2. Kept as a fallback only.
- Note: Windows runner minutes on private repos come from the free monthly quota at a 2× rate. If that runs out, more minutes cost money, which needs Cheke's OK.
- Each build gets a version like `0.1.0-preview.N` and a SHA-256 that Dex sends with the link, so Cheke can check the file.
- **What Dex tells Cheke with the link:** it's unsigned, so SmartScreen will say "Windows protected your PC" (More info → Run anyway); his antivirus may flag it because it installs a keyboard hook; the panic hotkey is Ctrl+Alt+Shift+Esc; Ctrl+Alt+Del always works; uninstall is in Settings → Apps.

## 11. Keys and accounts needed
None for the MVP. HelloAO needs no key (`HelloAoApi.kt`). Tauri needs no account. A code-signing certificate is deliberately **not** obtained. A Windows build and test environment is needed: GitHub Actions (or a cloud agent) for builds, plus a Windows VM for Kit's manual tests. Those are environments, not new keys. Cheke's own PC is used only by Cheke, for trying the preview.

## 12. Tests
- **Unit (TS, Vitest):** breath-engine step and phase transitions for all 6 patterns, and cycle durations (10, 16, 16, 19, 12, 9 s). The sigh's additive inhale. The scheduler: verse 1 at start, interval 45–60 s, changes only at a cycle-start inhale, no repeats until exhausted, translation round-robin with JPS skipped for NT.
- **Unit (Rust):** state machine (3 s delay, instant clear on NeedsYou/Done, 20 min cap, adapter-quiet clear). Verse client against a fake local API (success, 404, API down → restart and fallback verse). Sidecar start-up: reads `{port, token}`, sends the token.
- **KBS verse API (`kbs-verse-api/tests`):**
  - Bundle builder: chain loading from the three files only (no Linked/Cross), dedupe to 372 refs, ref parsing including ranges and display names, verse/range extraction against saved HelloAO chapter fixtures (no network in tests), manifest counts, fails on missing verses except JPS-for-NT.
  - Bundle completeness: every ref × every translation is present (JPS OT only).
  - Endpoints: random returns a chain ref; by-ref returns ref, text, `translation.abbr` and `translation.name` for all 13; 400/404/422 cases; `exclude` honoured; random distribution sanity check.
  - Server: binds 127.0.0.1 only, rejects requests without the token, exits when the parent dies.
  - Contract test: a shared JSON schema that both the API and the overlay client test against.
- **Hook logic:** the panic-combo detector as a pure function fed with synthetic key-event sequences.
- **CI (Windows runner):** Rust tests, frontend tests, `tauri build`, then a smoke test that installs the NSIS installer silently on the runner, starts the app, checks the sidecar's `/v1/health`, and uninstalls.
- **Manual on Windows 10 and 11 (Kit, in a VM):** the Done-when list, installed from the same installer Cheke will get. Also: SmartScreen flow; panic hotkey Ctrl+Alt+Shift+Esc and a changed hotkey; panic with the global shortcut broken; 90 s quiet auto-clear; chime volume and mute; verse shows at once; works with the network off; Win+L and unlock; Ctrl+Alt+Del; multi-monitor all vs active; focus goes back to Cursor; offline start.
- **Screenshots** of whiteout plain, whiteout with dust, and settings, for Jacqueline (spec Done when #8).

## 13. Risks
- **Cursor signal source unknown.** The whole MVP depends on spike S1. If Cursor doesn't expose usable hooks or logs, we fall back to UIA, which is fragile when Cursor updates.
- **Keyboard-hook lock-out on Cheke's own PC.** A bug in the LL hooks could freeze input on the machine he's using. Failsafes, all required before the first installer goes out: hook-side panic detection of Ctrl+Alt+Shift+Esc (works even if the global shortcut fails); the global shortcut as a second path; exit button; 90 s quiet auto-clear; 20 min hard cap; a watchdog that unhooks if the UI thread stalls; unhook on every exit path including crash/panic handlers; trivial hook callback (Windows drops slow LL hooks); hooks only installed while the whiteout is up; Ctrl+Alt+Del and Win+L always work. A tray item "Pause adapters" lets him turn it off without quitting. Kit tests each one in a VM before Dex sends the link.
- **Unsigned installer: SmartScreen.** Windows will show "Windows protected your PC" / unknown publisher, and some setups (company policy, Smart App Control on Windows 11) may block it outright. Mitigation: tell Cheke up front; signing needs a certificate, which costs money and needs his OK. If Smart App Control blocks it, the preview can't run on that PC without signing.
- **Antivirus/EDR warnings.** An unsigned `.exe` that installs a global keyboard and mouse hook, plus a second unsigned sidecar `.exe` that opens a local port, looks like a keylogger to heuristics. Defender or other AV may quarantine it. Mitigations: hooks only while the whiteout is up, sidecar bound to 127.0.0.1 only, no network calls at runtime, SHA-256 sent with the link. If Defender flags it, it can be submitted to Microsoft as a false positive (only with Cheke's OK, since that uploads the file).
- **Sidecar adds a moving part.** If the verse API crashes, the overlay restarts it and shows a fallback verse.
- **Development happens on a Linux box.** Win32 hooks, WebView2 and UIA can't be tested here. Builds run on Windows CI, manual tests in a Windows VM; nothing is built or set up on Cheke's machines.
- **WebView2 missing on some Windows 10 machines.**
- **Spec text still says "after 30 seconds"** in Must ship; Cheke's answer and Done when #4 say right away. The card follows right away; the spec line should be cleaned up.
- **Licensing:** the translations come from HelloAO and the Android app already credits it. The pre-built bundle redistributes text from 13 translations inside the installer. Check each translation's terms before anything beyond a private preview.
- Transparent full-screen windows on mixed-DPI multi-monitor setups can be offset. Test per-monitor DPI.

## 14. Open questions for Cheke
1. Kit's checklist for "Kit passes it" (Done when #7)?
2. Notes and labels for the 3 new exercises (and the sigh's second inhale)?
3. Dust theme: reuse an AER Breathe visual or a new design? Does Jacqueline have a spec?
4. Should the whiteout show the current tool state or which app triggered it, or only circle + verse + exit button?
5. Should the shuffle order and no-repeat history carry over across app restarts?
6. Fallback: is UIA alone enough for the MVP, with OCR left for later?
7. Is a private GitHub Actions Windows runner OK for building the preview (free quota; anything paid needs your OK)? How do you want the link delivered?
8. If SmartScreen/Smart App Control or your antivirus blocks the unsigned installer, do you want to pay for code signing, or stop there?
9. Should `kbs-verse-api` stay its own project for other KBS apps (the card's pick), or live inside ai-overlay?

## 15. Slice 02: dev-tool detection (rev 3, Oct 27 2026, Sol)
Spec: `docs/specs/02-dev-tool-detection.md`. Source as Remy relayed it from Dex: Cheke said "Build it for Cursor and other dev tools like terminal builds." Nothing ships or merges without Cheke's OK.

### 15.1 New adapter: `BuildAdapter` (`src-tauri/src/adapters/build.rs`)
- It uses the same `Adapter` trait as section 4: `id() = "build"` and `display_name() = "Terminal builds"`. It's added to the registry next to `cursor`.
- **Detection:** every 500 ms, `sysinfo` refreshes processes (name plus command line, current user only) and matches them against the watch list (15.3).
- **Working:** at least one matched process is alive. For each match, open a handle with `OpenProcess(SYNCHRONIZE | PROCESS_QUERY_LIMITED_INFORMATION)` through the `windows` crate and hold it while the process runs.
- **Done or failed:** when the handle signals, read `GetExitCodeProcess`. Exit 0 means `Done`. Anything else means `NeedsYou`, because the build failed and the user should look. If the handle can't be opened, for example an elevated process, the match is skipped and logged rather than guessed.
- **Quiet rule:** while a watched process is alive, each poll counts as a signal, so the 90 s auto-clear doesn't fire in the middle of a build. The 20-minute cap and the panic hotkey still apply.
- Nested matches are counted once, for example `npm run build` spawning `node` and `esbuild`: a child whose parent PID is already tracked is ignored.

### 15.2 Aggregating several tools (`src-tauri/src/state.rs`)
- One overlay only. The combined state is `Working` if any enabled adapter is `Working`. The 3 s arm timer starts on the first `Working` and is not reset when a second tool starts.
- It clears when every adapter is `Idle` or `Done`.
- **My call, which Cheke can change:** a `NeedsYou` from any adapter, such as a Cursor accept prompt or a failed build, clears the overlay right away even if another tool is still working. That's because the user is needed. It does not re-arm until a new `Working` edge arrives after that.

### 15.3 Watch list (settings)
- New settings field: `watch: [{ match: string, kind: "process" | "command" }]`, plus `adapters.build: true` (default on, same shape as `adapters.cursor`).
  - `process` matches the exe name exactly, case-insensitive (for example `cargo.exe`).
  - `command` is a case-insensitive substring of the full command line (for example `run build`).
- **Default list:** `cargo build`, `cargo run`, `npm run build`, `pnpm build`, `yarn build`, `gradle` (whole word; skip the Gradle daemon), `dotnet build`, `msbuild.exe`, `tsc.exe` as a process entry (never `--watch`), `vite build`. Command entries match whole words, not raw substrings.
- **Settings UI (Eden):** a list with add and remove, a "Reset to defaults" button, and the per-app on/off toggle for "Terminal builds". It validates that entries aren't empty and removes duplicates. Interactive shells (`cmd.exe`, `powershell.exe`, `pwsh.exe`, `bash.exe`, `node.exe` on its own) can't be added as `process` entries, because they'd keep the overlay up permanently.

### 15.4 Files touched
`adapters/mod.rs` (register), `adapters/build.rs` (new), `state.rs` (aggregation), `settings.rs` plus `src/settings.ts` (watch list), and `src/windows/Settings.tsx` (UI). The `windows` crate features add `Win32_System_Threading`. No network, nothing outside the device.

### 15.5 Tests
- Unit: the matcher (process vs command, case, dedupe, blocked shells), aggregation (two tools, NeedsYou wins, single arm timer), and the exit-code mapping.
- On the Windows CI runner: a script that runs a fake watched build (`cargo build` on a tiny crate) proves the whiteout arms within about 3 s and clears on exit, and that an unwatched `ping -n 10` does nothing.

### 15.6 Off-limits
Don't read terminal output or scrollback, don't inject into shells, and don't use admin rights or a driver. Reading process name, command line and exit code only.

## 16. Single instance (rev 4, Oct 28, 2026)
- Requested by Cheke on Oct 28, 2026, and added to Remy's spec as a must-ship.
- Add `tauri-plugin-single-instance` (Tauri 2) and register it first in `run()` in `src-tauri/src/lib.rs`, before the other plugins.
- What a second launch does: no new process starts. The plugin callback runs `open_settings(app)` on the copy that's already running.
- Files touched: `src-tauri/Cargo.toml` and `src-tauri/src/lib.rs`. Nothing else changes.
- Built on top of `31d0d04`, so the lock fix ships in the same installer.
- Done when: double-clicking the shortcut several times leaves exactly one tray icon and one process, and Settings opens.
- Off-limits: the first-run tray notice (Eden's proposal) is a separate later item and waits for Cheke's OK.
