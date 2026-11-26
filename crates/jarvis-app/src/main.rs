//! Jarvis desktop app: main window, system tray icon and the assistant engine.
//!
//! So far: type a question in the main window, Gemini answers, and the reply
//! is shown and spoken with the OS voice (see docs/DESIGN.md for the rest).

mod engine;
mod main_window;
mod tray;

use std::rc::Rc;
use std::time::Duration;

use gpui_kit::*;
use jarvis_store::logs::FileLogger;

use engine::Engine;
use main_window::{Conversation, MainWindow};

fn open_main_window(conversation: Entity<Conversation>, engine: Rc<Engine>, cx: &mut App) {
    let bounds = Bounds::centered(None, size(px(560.), px(420.)), cx);
    let options = WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(bounds)),
        titlebar: Some(TitlebarOptions {
            title: Some("Jarvis".into()),
            ..Default::default()
        }),
        ..Default::default()
    };
    let send = Rc::new(move |input| engine.send(input));
    if let Err(err) = gpui_kit::open_window(options, cx, |window, cx| {
        cx.new(|cx| MainWindow::new(conversation, send, window, cx))
    }) {
        log::error!("failed to open main window: {err:#}");
    }
}

fn init_logging(secrets: Vec<String>) {
    let Some(dir) = jarvis_store::logs_dir() else {
        eprintln!("no data folder on this system; logging to the terminal only");
        return;
    };
    match FileLogger::open(&dir, secrets).map(FileLogger::install) {
        Ok(Ok(())) => log::info!("Jarvis {} starting", env!("CARGO_PKG_VERSION")),
        Ok(Err(err)) => eprintln!("logger already installed: {err}"),
        Err(err) => eprintln!("cannot write logs to {}: {err}", dir.display()),
    }
}

fn main() {
    let gemini_key = jarvis_llm::gemini_key_from_env();
    init_logging(gemini_key.iter().cloned().collect());
    if gemini_key.is_none() {
        log::warn!("{} is not set", jarvis_llm::GEMINI_KEY_ENV);
    }
    let engine = Rc::new(Engine::start(gemini_key));

    gpui_kit::application().run(move |cx| {
        gpui_kit::init(cx);
        // Closing the main window only hides Jarvis; tray Quit is the way out.
        cx.set_quit_mode(QuitMode::Explicit);

        // The tray must live as long as the app; dropping it removes the icon.
        let tray = match tray::Tray::new() {
            Ok(tray) => Some(tray),
            Err(err) => {
                log::warn!("tray icon unavailable: {err:#}");
                None
            }
        };

        let conversation = cx.new(|_| Conversation::default());
        open_main_window(conversation.clone(), engine.clone(), cx);

        // Poll the assistant and the tray menu on the UI thread.
        cx.spawn(async move |cx| {
            let _tray = tray;
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(50))
                    .await;
                while let Some(output) = engine.poll() {
                    conversation.update(cx, |conversation, cx| {
                        conversation.apply(output);
                        cx.notify();
                    });
                }
                while let Some(cmd) = tray::poll() {
                    match cmd {
                        tray::Command::Show => cx.update(|cx| {
                            if cx.windows().is_empty() {
                                open_main_window(conversation.clone(), engine.clone(), cx);
                            }
                            cx.activate(true);
                        }),
                        tray::Command::Quit => {
                            log::info!("quit from tray");
                            cx.update(|cx| cx.quit());
                            return;
                        }
                    }
                }
            }
        })
        .detach();
    });
}
