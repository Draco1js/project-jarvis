//! Jarvis desktop app: GPUI window plus a system tray icon.
//!
//! Skeleton only. The assistant itself is wired in later milestones
//! (see docs/DESIGN.md).

mod tray;

use std::time::Duration;

use gpui_kit::{component::label::Label, *};

struct MainWindow;

impl Render for MainWindow {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .child(Label::new(
                "Jarvis skeleton: tray icon only, nothing else works yet.",
            ))
    }
}

fn open_main_window(cx: &mut App) {
    let bounds = Bounds::centered(None, size(px(520.), px(320.)), cx);
    let options = WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(bounds)),
        ..Default::default()
    };
    if let Err(err) = gpui_kit::open_window(options, cx, |_, cx| cx.new(|_| MainWindow)) {
        eprintln!("failed to open main window: {err:#}");
    }
}

fn main() {
    gpui_kit::application().run(|cx| {
        gpui_kit::init(cx);

        // The tray must live as long as the app; dropping it removes the icon.
        let tray = match tray::Tray::new() {
            Ok(tray) => Some(tray),
            Err(err) => {
                eprintln!("tray icon unavailable: {err:#}");
                None
            }
        };

        open_main_window(cx);

        // Poll tray menu events on the UI thread.
        cx.spawn(async move |cx| {
            let _tray = tray;
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(100))
                    .await;
                while let Some(cmd) = tray::poll() {
                    match cmd {
                        tray::Command::Show => cx.update(|cx| {
                            if cx.windows().is_empty() {
                                open_main_window(cx);
                            }
                            cx.activate(true);
                        }),
                        tray::Command::Quit => {
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
