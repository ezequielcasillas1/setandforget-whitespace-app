# AI Overlay: MVP UX packet (Eden, Oct 27 2026, updated for card rev 2)
Status: draft, preview build only. No publishing, signing or spending. Cheke tests the preview himself by running an unsigned installer on his own PC (Card rev 2).
Sources: spec `docs/specs/01-mvp.md` (Spec), card `docs/architecture/01-mvp-card.md` (Card), AER Breathe at `/workspace/apps/aer-breathe/app/src` (AER). Nothing here goes beyond those three.

## 1. Screen list
1. **Tray menu** (Card §2): Settings, Pause adapters, Show whiteout now (test), Quit. It is the only entry point at rest. No main window.
2. **Whiteout** (Spec; Card §2): full-screen, always on top, opaque plain white. It blocks all typing and clicks. It is transparent only for the fade in and out. Contents: breathing circle, verse, version label, exit button. No triggering tool is shown (decided, §6).
3. **Settings window** (Spec; Card §2, §8): normal window opened from the tray. Fields: exercise, verse timing, per-app on/off (Cursor), opt-in accessibility fallback, dust theme (off by default), reduced motion (system / on / off), monitors (all / active), panic hotkey, chime volume and mute (rows below). The 20-minute cap and the 90 s quiet timeout are fixed and don't appear in settings (Card §8).
   - **Panic hotkey row:** shows Ctrl+Alt+Shift+Esc by default and can be edited. Combos Windows reserves, or that the hook can't see (anything with Ctrl+Alt+Del or Win+L), are rejected (Card §3).
   - **Chime rows:** a volume slider (default low) and a mute toggle (on by default, not muted). The sound is one built-in chime (Card §2, §8).
4. **Breathing circle**
   - Default: circle on plain white (Spec).
   - Dust theme (optional, off by default): dithered dust, faint, slow, moves with the breath. No lines and no gradient rings (Spec).
   - Reduced motion: the engine keeps running (phase labels and verse timing still work), but the circle or label stays still instead of scaling (Card §5).
5. **Exercise picker** (in Settings): the six exercises, using names as they appear in the source:
   - From AER (`useBreathing.ts`): Coherence (5 in, 5 out), Deep calm (4-4-6-2), Box breathing (4-4-4-4)
   - New (Spec / Card §6): 4-7-8, Extended exhale, Physiological sigh
   - Rows follow AER's `pattern-option` look: name, note, timing.
6. **Verse display** with its **version label** under the verse: `abbr · name` exactly as the local offline KBS verse API sidecar returns it, e.g. "KJV · King James Version". It changes at the same moment as the verse. Credits line: "Free Use Bible API (HelloAO)", taken from the response `source` (Card §7.5). Verses come only from that local API, never from an online source.

## 2. User flows
**Main flow**
1. At rest, only the tray icon shows.
2. Cursor starts an agent run, so the state goes to working.
3. After about 3 s, the whiteout fades in on all monitors or the active one (per setting). It takes focus.
4. Breathing starts with the chosen exercise, and verse 1 shows immediately from the local KBS verse API (Card §7.5).
5. A new verse comes every 45–60 s, changing only on the inhale at the start of a cycle. Each change rotates to the next translation, and the version label updates with it.
6. Exit, by any of these:
   - needs-you (accept prompt) or done: clears right away with the built-in chime, at the set volume or silent if muted
   - panic hotkey (Ctrl+Alt+Shift+Esc)
   - exit button
   - 20-minute cap
   - auto-clear after 90 s of adapter quiet

   On exit, focus goes back to the dev tool.

**Settings flow:** tray, then Settings, then change a field. Settings are saved locally (`settings.json`). "Show whiteout now (test)" in the tray previews the result.

## 3. Layout rules
- **Exit button:** always visible on the whiteout and always clickable. The sources give no placement or size. Proposed rule: match AER's minimum tap target of 44px height (`.text-button`, `.duration-options`). Placement is TBD (Cheke / Jacqueline).
- **Hotkey hint:** show the configured combo near the exit button, e.g. "Ctrl+Alt+Shift+Esc to exit". It must reflect the setting. No other wording is in the sources.
- **Circle:** centered on each whiteout window. AER centers its canvas at `width/2` and scales from `min(width, height)`. Keep that rule. The exact desktop size is not specified (TBD).
- **Verse and version label:** the verse sits with the circle, and the version label sits directly **under** the verse (Card §7). Exact offsets are TBD.
- **Multi-monitor** (Spec; Card §2):
  - All monitors: one window per monitor, all driven by one session (one breathing clock, one verse).
  - Active monitor: one window on the monitor with the foreground window.
  - Test mixed-DPI setups for offsets (Card §13).

## 4. Empty and error states
- **No exercises / bad saved exercise:** not covered by the sources. Proposed: fall back to Coherence, the first in `PATTERNS`. Needs confirmation.
- **KBS verse API down (sidecar crashed):** the overlay restarts it once. Meanwhile it shows the bundled fallback verse (KJV John 14:27) with its version label "KJV · King James Version", so the whiteout is never blank (Card §7.5). Always show the version label for whatever text is on screen.
- **Network off:** no change. The API is local and offline (Card §7.2).
- **JPS on a New Testament ref:** skip JPS (Card §7).
- **Hotkey conflict:** if the global-shortcut registration fails, the input hook still catches the combo (Card §3). Settings should show a plain message such as "This shortcut is in use by another app. The hotkey will still exit the whiteout." The wording is a proposal.
- **Hotkey rejected:** if the new combo is one Windows reserves or the hook can't see (Ctrl+Alt+Del, Win+L), Settings refuses it and keeps the previous combo (Card §3). Proposed message: "Windows reserves this shortcut. Choose another." The wording is a proposal.
- **Adapter quiet:** auto-clear after 90 s.
- **Session too long:** hard cap at 20 min (not a setting).
- **Lock or unlock (Win+L, Ctrl+Alt+Del):** the whiteout may still be up afterwards, and the hook is reinstalled (Card §3). There is no special screen for this.
- **Reduced motion on:** static circle, labels and verses continue.

## 5. Do-not-redesign list
- AER `useBreathing.ts` step model, phases, `getBreathValue` easing (`smooth`), and the three AER patterns with their names, timings, notes and phase labels ("Breathe in", "Hold softly", "Breathe out", "Rest here"). The card allows only these changes: move patterns to `patterns.ts`, add 3 exercises, the additive inhale for the sigh, and session length.
- AER type: Cormorant Garamond (display), Manrope (body), IBM Plex Mono (small labels), bundled with `@fontsource`.
- AER accent tokens are available (`--pollen` oklch(0.91 0.17 102), `--muted`, `--ink-*`). Note: AER is a dark app. The overlay background stays **plain white** per spec. Do not carry AER's dark backgrounds or color themes over.
- Plain white by default. Dust is off by default, with no lines and no gradient rings.
- One session at a time, one breathing clock, one verse across monitors.
- Escape hatches: panic hotkey, exit button, 20-minute cap, auto-clear when the adapter goes quiet.

## 6. Decisions from Cheke (formerly open questions; settled 9:07 PM CT, card rev 2)
1. Does the whiteout show which tool triggered it?
   **Decided:** No. Show only the circle, verse, version label and exit button.
2. Does the dust theme reuse AER visuals?
   **Decided:** Yes. Reuse AER's `BreathEcosystem` motes, with no rings and no lines.
3. What are the labels for the 3 new exercises?
   **Decided:** "4-7-8", "Extended exhale", "Physiological sigh". Phase labels are as the card drafted them ("Top up", "Long sigh out" for the sigh). The notes are still TBD.

## Slice 02: dev-tool detection
Sources: card §15 (rev 3), spec `docs/specs/02-dev-tool-detection.md`. Nothing here goes beyond those two, the existing packet, and `src/windows/Settings.tsx`. Wording marked "proposed" needs Cheke's OK. Nothing ships or merges without his OK (Card §15).

### S2.1 Settings: watch-list editor
- **Placement:** inside the existing **Apps** section (`<h2>Apps</h2>`), directly under the Cursor row and above "Accessibility fallback (opt-in)". There is no new window, tab or section heading. Save stays the single Save button in `.actions` (no autosave), the same as other fields.
- **Terminal builds toggle:** a `label.row` checkbox like the Cursor row. Label "Terminal builds" (the adapter's `display_name()`, Card §15.1). It maps to `adapters.build.enabled` and is **on by default** (Card §15.3).
  - Helper text, one line under the row (proposed): "Whiteout comes on while a watched build or command is running."
- **Watch list** (shown under the toggle). When the toggle is off the list stays visible but disabled (proposed; TBD).
  - **Rows:** one `.row` per entry. On the left is the entry text in IBM Plex Mono. Next to it is its kind, `process` or `command`, as a small mono label. On the right is a remove control, a text button "Remove" with a 44px minimum tap target (AER `.text-button`). Removing happens right away in the draft, with no confirm. Like every other field it isn't saved until Save.
  - **Kind meaning** (Card §15.3). Optional helper copy is TBD.
    - `process` matches the exe name exactly, case-insensitive (e.g. `cargo.exe`).
    - `command` is a case-insensitive substring of the full command line (e.g. `run build`).
  - **Add:** a text field, a kind picker (`select` with Process / Command, like the existing selects) and an **Add** button. Enter in the field also adds. On success the field clears. The default kind is TBD.
  - **Reset to defaults:** a secondary button under the list. If the current list differs from the defaults, it asks first. Proposed copy: "Replace your watch list with the defaults? Your changes will be lost." with the buttons "Reset" and "Cancel". If the list already matches the defaults, it resets with no prompt. The confirm style (inline vs dialog) is TBD.
  - **Default list** (Card §15.3, quoted): `cargo build`, `cargo run`, `npm run build`, `pnpm build`, `yarn build`, `gradle`/`gradlew`, `dotnet build`, `msbuild.exe`, `tsc`, `vite build`. The card doesn't give the kind for each default entry (TBD, Sol).

### S2.2 Validation and empty states
Errors show inline under the add field as `p.msg`, the same pattern as `hotkeyMsg`. The entry is not added, the typed text stays in the field, and the message clears on the next edit.
- **Empty entry** (blank or whitespace only): "Enter a process name or command."
- **Duplicate** (same text and kind, case-insensitive, after trimming): "That's already on the list."
  - Note: Card §15.3 says the validator "removes duplicates". This packet blocks them at Add with a message. Any duplicates in a loaded `settings.json` are removed silently.
- **Interactive shell as `process`**: the card's rule, quoted: "Interactive shells (`cmd.exe`, `powershell.exe`, `pwsh.exe`, `bash.exe`, `node.exe` on its own) can't be added as `process` entries, because they'd keep the overlay up permanently."
  - Copy: "Shells like `<name>` can't be watched. They stay open, so the whiteout would never clear. Add the build command instead (e.g. `npm run build`)."
  - The block applies only to kind `process`. The card doesn't say whether to match with or without `.exe` (e.g. `pwsh`). That is TBD (Sol).
- **Empty list** (every entry removed): show the line "No builds watched. Terminal builds won't trigger the whiteout." in place of the rows, with Add and Reset to defaults still available. The card doesn't say whether an empty list is allowed to save (TBD).

### S2.3 Overlay behavior with several tools (Card §15.2)
- **One overlay only.** Working if any enabled adapter is working. The 3 s arm timer starts on the first working tool and does **not** restart when a second tool starts.
- **Normal clear:** when every adapter is idle or done.
- **Needs-you wins:** a failed build (non-zero exit) or a Cursor accept prompt clears the overlay **instantly**, even if another tool is still working. That means no fade and no confirmation. It doesn't re-arm until a new working edge arrives.
  - The card says "right away". "No fade" comes from this brief. The main packet (§1) still describes a fade-out, so confirm with Cheke.
  - The built-in chime on needs-you carries over from §2 (assumed; card §15 is silent).
- **Builds keep it up:** while a watched process is alive, the 90 s quiet auto-clear doesn't fire. The 20-minute cap and the panic hotkey still apply (Card §15.1).
- **Whiteout contents don't change:** no triggering tool is shown (§6.1).
- **Elevated processes** that can't be opened are skipped silently. There is no UI message (Card §15.1).

### S2.4 Do-not-redesign
- The existing Settings layout in `Settings.tsx`: section order (Exercise, Verse timing, Apps, Look, Exit and sound), `.row` / `.pattern-option` / `.msg` / `.actions`, and the single Save button. Only the S2.1 rows get added.
- Type: Cormorant Garamond (display), Manrope (body), IBM Plex Mono (small labels and entry text).
- AER styling: tokens (`--pollen`, `--muted`, `--ink-*`) and the 44px tap targets. No new colors, icons or brand changes.
- The whiteout screen, its escape hatches and the §5 list are unchanged.

### S2.5 TBDs
1. Watch-list kind for each default entry.
2. Default kind in the Add picker.
3. Shell block with or without `.exe` (`pwsh` vs `pwsh.exe`).
4. Whether an empty list can be saved.
5. Disabled vs hidden list when Terminal builds is off.
6. Reset confirm style.
7. Instant clear with no fade vs the §1 fade-out, and chime on needs-you (Cheke).
8. All copy marked proposed.
