# Whitespace slice 02: dev-tool activity detection (Remy, Oct 27 2026)
Source: Cheke, relayed by Dex from their DM (9:21 PM CT): "Build it for Cursor and other dev tools like terminal builds."

## Problem
The whiteout should come on whenever dev tools are busy, not only Cursor agent runs.

## Must ship
- Cursor adapter (from 01).
- Terminal or build adapter: it reports working while a watched command or process runs, such as `npm run build`, `cargo build`, `gradle` or `dotnet build`. It reports done when that ends, using the exit code.
- A user-editable list of process names or commands to watch in settings, with a sensible default list.
- The same rules as 01: about 3 seconds in the working state before the whiteout, an instant clear on done or needs-you, the hotkey, the 20-minute cap, and auto-clear after 90 seconds of quiet.
- When several tools are busy, one overlay stays up until all of them are idle or done.

## Later
- Adapters for Claude, Grok Bot and browser builders.

## Constraints
- Windows 11 and 10.
- Detection happens on the device only. Nothing gets published.

## Done when
1. A watched terminal build triggers the whiteout within about 3 seconds and clears when it ends.
2. Unwatched processes don't trigger it.
3. The list can be edited in settings.
4. Kit passes it.
