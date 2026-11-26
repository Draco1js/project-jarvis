//! Runs the headless assistant on its own thread with the real adapters.
//! The UI sends it inputs and polls the outputs it emits.

use std::path::Path;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use std::time::{Duration, SystemTime};

use jarvis_core::{
    Assistant, Input, Message, Notifier, Output, Ports, Reminders, Speaker, Store,
    ports::SystemClock,
};
use jarvis_llm::OpenAiChat;
use jarvis_store::reminders::SqliteReminders;
use jarvis_tools::weather::{WTTR_URL, Wttr};
use jarvis_tts::OsVoice;

/// How often the assistant is told time passed, so reminders go off.
const TICK: Duration = Duration::from_secs(1);

/// The city setting for weather. There is no settings file yet, so it is
/// read from this environment variable; unset means wttr.in's IP location.
pub const CITY_ENV: &str = "JARVIS_CITY";

/// Handle to the assistant thread.
pub struct Engine {
    inputs: Sender<Input>,
    outputs: Receiver<Output>,
}

impl Engine {
    pub fn start(gemini_key: Option<String>) -> Self {
        let (inputs, inbox) = mpsc::channel::<Input>();
        let ticks = inputs.clone();
        thread::Builder::new()
            .name("jarvis-tick".into())
            .spawn(move || {
                // The first tick goes at once, so reminders missed while
                // Jarvis was off go off at start.
                while ticks.send(Input::Tick).is_ok() {
                    thread::sleep(TICK);
                }
            })
            .expect("could not start the tick thread");
        let (outbox, outputs) = mpsc::channel::<Output>();
        thread::Builder::new()
            .name("jarvis-core".into())
            .spawn(move || {
                let speaker: Box<dyn Speaker> = match OsVoice::start() {
                    Ok(voice) => Box::new(voice),
                    Err(err) => {
                        log::warn!("{err:#}; replies will only be shown");
                        Box::new(Silent)
                    }
                };
                let mut assistant = Assistant::new(Ports {
                    chat: Box::new(OpenAiChat::gemini(gemini_key)),
                    speaker,
                    store: Box::new(NoHistory),
                    clock: Box::new(SystemClock),
                    reminders: open_reminders(),
                    notifier: Box::new(DesktopNotifier),
                    weather: Box::new(Wttr::new(WTTR_URL, std::env::var(CITY_ENV).ok())),
                });
                for input in inbox {
                    assistant.handle(input, &mut |output| {
                        if let Output::Problem(problem) = &output {
                            log::warn!("{problem}");
                        }
                        let _ = outbox.send(output);
                    });
                }
            })
            .expect("could not start the assistant thread");
        Self { inputs, outputs }
    }

    pub fn send(&self, input: Input) {
        let _ = self.inputs.send(input);
    }

    /// The next output, if one is waiting.
    pub fn poll(&self) -> Option<Output> {
        self.outputs.try_recv().ok()
    }
}

/// Reminders in the app database, or only in memory if it cannot be opened.
fn open_reminders() -> Box<dyn Reminders> {
    let opened = jarvis_store::database_path()
        .ok_or_else(|| "no data folder".to_string())
        .and_then(|path| {
            if let Some(dir) = path.parent() {
                std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
            }
            SqliteReminders::open(&path).map_err(|e| e.to_string())
        });
    match opened {
        Ok(reminders) => Box::new(reminders),
        Err(err) => {
            log::error!(
                "could not open the database ({err}); reminders will not survive a restart"
            );
            Box::new(
                SqliteReminders::open(Path::new(":memory:"))
                    .expect("in-memory SQLite always opens"),
            )
        }
    }
}

/// Native desktop notifications (toast, Notification Center, freedesktop).
struct DesktopNotifier;

impl Notifier for DesktopNotifier {
    fn notify(&mut self, title: &str, body: &str) {
        let shown = notify_rust::Notification::new()
            .appname("Jarvis")
            .summary(title)
            .body(body)
            .show();
        if let Err(err) = shown {
            log::warn!("could not show a notification: {err}");
        }
    }
}

/// Used when the OS has no voice.
struct Silent;

impl Speaker for Silent {
    fn speak(&mut self, _: &str) {}
}

/// History is not saved yet (SQLite history comes with the memory ticket, #11).
struct NoHistory;

impl Store for NoHistory {
    fn save_message(&mut self, _: SystemTime, _: &Message) {}
}
