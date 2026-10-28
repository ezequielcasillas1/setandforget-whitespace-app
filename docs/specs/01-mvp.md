# AI Overlay: MVP spec (Remy, Oct 27 2026)
Status: draft. Not handed to Sol until Cheke confirms the start in his own words.

## Problem
While an AI dev tool is working, Cheke wants a calm whiteout screen with breathing and Bible verses instead of watching the tool. The whiteout clears when he needs to accept a change or when the work is done.

## User
Cheke.

## Must ship (preview only)
- Per-tool adapters reporting four states: idle, working, needs-you, or done.
  - Cursor goes first, through hooks or event logs.
  - OS accessibility signals or screen reading as an opt-in fallback.
- Whiteout:
  - Shows after about 3 seconds in the working state.
  - Clears right away on needs-you or done, with a soft chime.
  - Escape hatches: a panic hotkey, a 20-minute cap, and auto-clear when an adapter goes quiet.
- Window: full-screen, transparent, always on top. One overlay at a time.
- Breathing circle on plain white, which is the default.
  - Six exercises in total. The three from AER Breathe, reused from useBreathing.ts with the same step model:
    - Coherence (5 in, 5 out)
    - Deep Calm (4 in, 4 hold, 6 out, 2 rest)
    - Box (4-4-4-4)
  - Three new ones:
    - 4-7-8 (4 in, 7 hold, 8 out)
    - Extended Exhale (4 in, 8 out)
    - Physiological Sigh (2 in, 1 in, 6 out)
- Verses:
  - The first verse shows right away with the breathing, then a new one every 45 to 60 seconds.
  - Verses change only on an inhale.
  - Shuffled, with no repeats.
  - Pulled from the King Bible Study API (chains scope), across all translations, with the version shown.
- Settings:
  - Exercise.
  - Verse timing.
  - Per-app on/off.
  - Dithered dust theme toggle, off by default. When on, it's faint, slow and moves with the breath. No lines and no gradient rings.
  - Reduced motion.
- Everything stays on the device.

## Cheke's answers (Oct 27, 8:57 PM CT, in Remy's chat)
- Platforms: Windows 11 and 10 come first. Tauri is still the choice.
- Verses start as soon as the breathing starts, with no wait. They rotate with the breathing, changing on an inhale.
- Verse source: King Bible Study API data, chains scope only. That means all chain data except Linked and Cross.
- Translations: every translation in the King Bible Study API. The UI shows the version name and updates it as the verse changes with the breathing.
- Monitors: a setting to cover every monitor or only the active one.
- The overlay whites out the screen completely and blocks all typing and clicks. The user can interrupt it at any time with a panic hotkey or an exit button, which drops them back to the dev tool.
- Accept prompts: the overlay clears on every accept prompt (Remy's call).

## Later
- Adapters for Claude, Grok Bot and browser builders (a browser extension).
- Full dust field (v2).
- Mac build.

## Constraints
- Preview only. No publishing, release, signing or spending without Cheke's OK.
- Repo work stays off his computers.

## Done when
1. A Cursor agent run triggers the whiteout within about 3 seconds.
2. It clears on an accept prompt or when the work is done.
3. All six exercises run.
4. Verses start right away with the breathing and change on an inhale.
5. The dust toggle and reduced-motion setting work.
6. The hotkey and 20-minute cap work.
7. Kit passes it.
8. Screenshots go to Jacqueline.

## Remy's calls (Oct 27, 9:02 PM CT)
- Cursor events: Sol and Dex pick them from Cursor's documented hooks and logs. Working means an agent run has started, needs-you means an accept or approval prompt is up, and done means the run has ended.
- Panic hotkey: Ctrl+Alt+Shift+Esc. It can be changed in settings.
- Auto-clear after 90 seconds of quiet from the adapter.
- Chime: one soft built-in sound with a volume slider and a mute toggle. It's on by default at low volume.

## Cheke's calls (Oct 27, 9:07 PM CT)
- Verse source: we build our own King Bible Study verse API over the KBS chains data, leaving out the Linked and Cross chains.
  - It returns the verse text for every translation, with the version name.
  - The overlay calls only this API, never HelloAO directly. The API itself may load its text from HelloAO.
  - For the preview it runs locally or bundled, with a disk cache. Hosting it publicly needs Cheke's OK (no deploys and no spending).
- Testing: Dex hands Cheke a preview Windows installer to try on his own PC. It's not published, and it's delivered to him through a link or file. Nobody installs it on his computers for him.

## Cheke, Oct 27, 9:16 PM CT (room)
- Name: Whitespace. Domain: whitespace-setandforget.app.
- Order after the desktop preview: (1) Android edition, (2) a landing page on that URL that markets the app.
- Cheke asked us to connect Cloudflare to the domain. Open question: is the domain registered, and in which Cloudflare account? No buying, DNS changes or going live until he confirms.
