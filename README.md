# Jarvis

A cross-platform voice assistant for the desktop (Windows, macOS, Linux), built in Rust with [GPUI](https://gpui.rs) via `gpui-kit`.

Say "Hey Jarvis" (or press the hotkey), ask something, and Jarvis answers out loud. It can search the web (with DuckDuckGo-style bangs, in a private window), open apps, URLs and folders, control volume and media, set timers and reminders, keep notes, and tell you the weather.

- Speech-to-text: Groq.
- Answers: Gemini (free API key) first, an optional local model through `llama-server` as fallback.
- Speech: your OS voice, or optional Kokoro.
- Wake word: openWakeWord ("Hey Jarvis") behind an energy gate, so idle CPU use stays near zero. Custom wake words via Rustpotter.

> **Status:** early skeleton. The app opens a window and a tray icon; nothing else works yet. See [docs/DESIGN.md](docs/DESIGN.md) for the plan.

## Building

Requires Rust 1.90+ (edition 2024).

```sh
cargo run -p jarvis-app
```

Linux build dependencies (Debian/Ubuntu):

```sh
sudo apt install clang libxkbcommon-dev libxkbcommon-x11-dev libwayland-dev \
  libvulkan-dev libfontconfig1-dev libfreetype-dev libasound2-dev \
  libx11-xcb-dev libxcb1-dev libzstd-dev
```

## Unsigned builds

Release builds are not code-signed.

- **Windows:** SmartScreen shows "Windows protected your PC". Click **More info**, then **Run anyway**.
- **macOS:** Gatekeeper blocks the first launch. Open **System Settings → Privacy & Security** and click **Open Anyway**, or run `xattr -dr com.apple.quarantine /Applications/Jarvis.app`.

## License

MIT
