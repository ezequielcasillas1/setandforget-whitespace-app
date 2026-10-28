# AI Overlay — MVP preview (unsigned, not published)

Tauri 2 (Rust) + React 19 / TypeScript 5.9 / Vite 7. Built from docs/specs/01-mvp.md, docs/architecture/01-mvp-card.md (rev 2), docs/ux/01-mvp-ux-packet.md.

## Run
    npm install
    npm test                                  # frontend unit tests (vitest)
    cargo test --manifest-path src-tauri/Cargo.toml   # Rust unit tests
    npm run dev          # browser preview of the whiteout at http://localhost:1420 (#settings for Settings)
    npm run tauri:dev    # desktop app (tray icon → Settings / Show whiteout now (test))
    npm run tauri:build  # unsigned local binary, no installer: src-tauri/target/release/ai-overlay

Verses come only from the local KBS verse API. Point dev builds at it with
`KBS_VERSE_API_URL=http://127.0.0.1:8787` (optional `KBS_VERSE_API_TOKEN`). Without it, the
whiteout shows the bundled fallback verse (KJV John 14:27).

## Not built yet (see report)
Cursor event source (spike S1), Win32 low-level input hooks + watchdog, focus restore, UIA fallback,
kbs-verse-api sidecar, Windows installer/CI.
