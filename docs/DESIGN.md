# Jarvis design

Decisions agreed on 2026-11-26 when the project restarted. The earlier prototype (Raspberry Pi clients, Socket.IO server, Vosk, Piper) is in git history before commit "Wipe old Pi/server prototype".

## What it is

A desktop voice assistant that runs in the background on Windows, macOS and Linux. Mainly an assistant (you ask, it answers and acts), with a dictation mode on top ("Jarvis, type …").

Platform priority for building and testing: **Windows → macOS → Wayland → X11.** The other platforms only need to keep compiling until their turn.

## Stack

- Rust, Cargo workspace, edition 2024.
- UI: `gpui-kit` (currently 0.11), which pins its own GPUI release (`gpui-pre`). Depend on `gpui-kit` only, never on `gpui` directly.
- Tray: `tray-icon` (KSNI backend on Linux, so no GTK). Hotkeys: `global-hotkey`. Start at login: `auto-launch`.

### Crates

| Crate | Responsibility |
|---|---|
| `jarvis-app` | GPUI windows, tray, panel, setup wizard. Binary `jarvis`. |
| `core` | Headless assistant engine: turn state machine, conversation context, provider fallback, barge-in, summaries. Talks to every outside service through port traits so it can be tested with fakes (the main test seam). |
| `audio` | Mic capture (`cpal`), voice activity detection (`earshot`). |
| `wake` | Energy gate + `WakeDetector` trait: openWakeWord for the built-in "Hey Jarvis", Rustpotter for custom and personal wake words. |
| `stt` | Groq transcription client. |
| `llm` | Gemini and llama-server clients (one OpenAI-compatible client), fallback logic. |
| `tools` | Assistant actions. |
| `tts` | OS voice (`tts` crate), optional Kokoro. |
| `store` | Settings, SQLite (history, memory, timers), keyring, migrations, purge. |
| `sidecar` | Download, verify and supervise `llama-server`. |

## App shape

- Runs in the background with a tray icon. Closing the main window hides it; **Quit** in the tray quits.
- On wake word or hotkey, a small **floating panel** appears near the bottom centre of the screen the mouse is on. It shows the transcript and reply, then fades out a few seconds after the reply. **The panel must never take keyboard focus** (dictation types into the previously focused app). Clicking it allows typing to Jarvis.
- The main window holds history and settings.

**Verified 2026-11-26 (gpui-pre 0.3.14 source):** open the panel as `WindowKind::PopUp` with `focus: false`.

- Windows: PopUp gets `WS_EX_TOOLWINDOW | WS_EX_TOPMOST` (always on top, no taskbar button), and `focus: false` shows it with `SW_SHOWNOACTIVATE`, so it does not take focus. Clicking it does activate it (GPUI answers `WM_MOUSEACTIVATE` with `MA_ACTIVATE`), which is what we want for typing.
- macOS: PopUp is an `NSPanel` with `NonactivatingPanel`, pop-up window level, visible on all Spaces and over full-screen apps. `focus: false` uses `orderFront` instead of `makeKeyAndOrderFront`. It can still become key when clicked.
- Open a fresh panel window for each activation instead of re-showing a hidden one, since the re-show path was not checked.
- Wayland (`WindowKind::LayerShell`) and X11 are still to be checked when their turn comes.
- On macOS the app should also run as an accessory app (no Dock icon) via `LSUIElement`.

## Voice pipeline

1. **Wake word:** an RMS energy gate passes audio to the detector only when there is sound. The detector sits behind a `WakeDetector` trait with two engines:
   - **Built-in "Hey Jarvis" (default):** openWakeWord's pretrained `hey_jarvis` model (a few MB of ONNX, trained on many synthetic voices, so it works for any speaker). It runs in the pure-Rust `tract` crate if that loads the models correctly; otherwise in `ort` (ONNX Runtime).
   - **Custom or personal wake words (optional):** Rustpotter, trained on the device from 3–5 recordings of the user. Offered in setup and settings.
   - **Rustpotter status (checked 2026-11-26):** 3.0.2 is the latest release, and the repo has had no commits since 2023-10-01. It does not compile with current dependencies: its `candle-core` 0.2.2 breaks with newer `half`. With `half` pinned to `=2.3.1` it builds and runs on Linux and passes `cargo check` for Windows (`x86_64-pc-windows-msvc`) and macOS (`aarch64-apple-darwin`). If the pin starts clashing with other dependencies, fork Rustpotter to update `candle`, or drop custom words.
   - Plain "Jarvis" (without "Hey") as the built-in word would need our own openWakeWord model trained through its synthetic-speech pipeline. Not planned for now.
   - Accuracy and CPU are measured in the wake-word bake-off (issue #2) before the `wake` crate is built; the results go here.
2. **Hotkey:** push-to-talk. Default `Ctrl+Alt+Space` (Windows/Linux), `Option+Space` (macOS). Configurable. If the combination is taken, say so in the panel.
3. **End of speech:** `earshot` VAD. Stop after **1 s** of silence following speech; hard cap **30 s**.
4. **Speech-to-text:** Groq, `whisper-large-v3-turbo`, one batch upload per utterance. Language auto-detected.
5. **Barge-in:** the wake word or hotkey during speech stops TTS and starts listening. The system prompt tells the model never to say "Jarvis" (no other echo handling; speakers are rarely used).
6. **Typing is always possible** in the panel, including offline.

Audio is never written to disk.

## Answers (LLM)

- **Primary:** Gemini via its OpenAI-compatible endpoint (`https://generativelanguage.googleapis.com/v1beta/openai/`). Default model `gemini-3.5-flash-lite`; settings dropdown offers `gemini-3.8-flash`, `gemini-3.6-flash`, `gemini-4-hydrogen` (each verified to call tools before it is listed). Avoid `gemini-2.5-*` (redirected from 2026-12-01).
- When the Gemini key is entered, show a one-line notice: free-tier content is used by Google to improve its products.
- **Fallback:** local `llama-server`. Switch on 429, 5xx, timeout (~8 s) or unreachable. Stay on local until the rate-limit reset time, or 15 minutes if none is given. If no local model is installed, say so and offer the download in the panel.
- Offline means no speech-to-text (Groq), so offline use is typed only. Local speech-to-text is a possible later add-on.
- **Replies are always in English.**

### Local model

- `llama-server` from llama.cpp's official releases, **downloaded only when the user chooses**. Vulkan build on Windows and Linux, Metal on macOS, CPU build when no usable GPU. Version pinned per Jarvis release, SHA-256 verified.
- Default model: abliterated Qwen3.5-4B, Q4_K_M (~2.7 GB). Before locking the default, test its tool calling against our schema; fall back to stock Qwen3.5-4B if unreliable.
- Users may enter a **Hugging Face repo name**. Jarvis lists its GGUF files, suggests Q4_K_M, shows sizes, rejects repos with no GGUF, and warns when a file exceeds free RAM/VRAM (no hard size cap). Gated repos are not supported at first.
- Every downloaded file is tracked in a manifest inside the data folder so it can be listed, deleted and purged.
- Loaded on wake word, unloaded after **10 minutes** idle (configurable, with an "always loaded" option).

## Speech output

- Default: the OS voice (`tts` crate).
- Optional, removable download: Kokoro-82M (~92 MB int8 + voices + ONNX runtime). Unloaded when idle.

## Tools (actions)

Fixed allowlist; no arbitrary shell.

| Tool | Notes |
|---|---|
| Web search | Opens `https://unduck.link?q=!<bang> <query>`. The model decides whether a bang is needed. Editable preset list maps spoken names to bangs (YouTube `!yt`, Reddit `!r`, GitHub `!gh`, Amazon `!a`, Wikipedia `!w`). |
| Open URL | Same browser rules as search. |
| Open app | Index of Start Menu shortcuts (Windows), `/Applications` (macOS), `.desktop` files (Linux). Fuzzy match; ask when unsure. |
| Open folder / recent file | e.g. "open my Downloads". |
| Volume / mute | |
| Media | Play/pause, next, previous via media keys. |
| Timers, alarms, reminders | Saved in SQLite; missed ones fire on next start marked late; native notifications. |
| Notes | Markdown files in `Documents/Jarvis Notes` (configurable). Searchable. |
| Weather | wttr.in. City setting first, IP location otherwise; named places always work. |
| Time, date, arithmetic | Answered by the model; no tool. |

Not doing: Wi-Fi/Bluetooth toggles, window control, screenshots, lock/sleep/brightness.

### Browser

- User-selectable browser, default = system default.
- **Always a private window:** Chrome/Brave/Chromium `--incognito`, Edge `--inprivate`, Firefox `--private-window`, Opera `--private`. Safari or unknown browsers open a normal window and say once that private mode is not supported. No AppleScript.

### Dictation

"Jarvis, type …" types the rest into the focused app with simulated key presses (`enigo`); clipboard paste (with the previous clipboard restored) is the fallback.

## Memory

- In-memory conversation context; starts fresh after **5 minutes** idle.
- History saved in SQLite and clearable.
- When a conversation ends, one model call writes a 1–2 sentence **summary plus 5–10 tags** (synonyms, topics, names). Notes get tags the same way.
- Search with SQLite **FTS5** over text, summaries and tags, exposed to the model as a `search_memory(query)` tool. No embeddings for now.

## Secrets, data and privacy

- Groq and Gemini keys entered in the app, stored in the OS keyring (`keyring` crate). Env var `JARVIS_GROQ_API_KEY` as a dev fallback.
- All app data in **one per-OS data folder**: settings, SQLite, models, llama-server, Kokoro, logs.
- No telemetry. Local logs rotate (5 × 5 MB) with keys redacted; "show logs" button in settings.

## First-run setup

The first screen lists what to have ready (Groq key required, Gemini key optional) with links. Then:

1. Groq key (required)
2. Gemini key (optional, privacy notice)
3. Mic test
4. Wake word (built-in "Hey Jarvis", or train a custom/personal one)
5. Offer local model download
6. Start at login (default on)

Steps after the Groq key can be skipped. Quitting mid-setup must never leave a bad state:

- Each step is saved as soon as it completes; the next launch resumes at the first unfinished step.
- Keys are validated with a test request before saving.
- Downloads go to `*.part` files and are renamed only after the checksum passes; partial downloads resume; unknown `.part` files are removed on start.
- Start-at-login is enabled only in the final step.

## Process cleanup

Quit stops mic, TTS and `llama-server`. For crashes:

- Windows: children in a Job Object with kill-on-close.
- Linux: children started with parent-death signal.
- macOS: child PIDs written to a file; leftovers killed on next start.

## Packaging, updates, uninstall

- `cargo-packager`: NSIS (Windows), `.dmg` (macOS), `.deb` and AppImage (Linux). **No code signing**; README explains SmartScreen and Gatekeeper.
- `jarvis --purge` deletes the data folder, keyring entries and the start-at-login entry. **Notes are kept**, and the uninstaller says so.
  - Windows: the NSIS uninstaller runs purge.
  - macOS and AppImage: an **Uninstall Jarvis** button in settings.
  - `.deb`: the removal script runs purge.
- **Self-update** via `cargo-packager`'s updater from GitHub Releases, verified with a minisign signature (private key in a GitHub secret). Downloads in the background, then asks "Update ready, restart now?".
  - Windows: silent NSIS install over the old version. **The upgrade must never run purge**; test this.
  - macOS: replace the `.app`. AppImage: replace itself. `.deb`: show a "new version" link instead.
  - Settings and database are versioned with migrations, so updates never need setup again. A failed update leaves the old version and data untouched.

## CI

GitHub Actions builds and tests on Windows, macOS and Linux for every push; tags build installers and release artifacts.

## Milestones

0. **Do first:** check whether `tract` can run openWakeWord or we need `ort`, which decides whether ONNX Runtime ships natively. Then the wake-word bake-off (issue #2). Then confirm the non-focusing panel on a real Windows machine.
1. **Windows MVP:** tray, built-in "Hey Jarvis" wake word, hotkey, VAD, Groq, Gemini, OS voice, search with bangs in a private window, open app/URL, volume, panel.
2. Then: local model fallback, dictation, timers, notes, memory, weather, Kokoro, wake-word training, installer and updater, then macOS, Wayland and X11.

## Naming and licence

"Jarvis" for now (rename before any wide public release, since it is a Marvel character). MIT licence.
