//! Runs the headless assistant on its own thread with the real adapters.
//! The UI sends it inputs and polls the outputs it emits.

use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::thread;
use std::time::{Duration, SystemTime};

use jarvis_core::{
    Assistant, Input, MemoryHit, Message, Output, Ports, Speaker, Store, Tagged, ports::SystemClock,
};
use jarvis_llm::OpenAiChat;
use jarvis_store::memory::{SqliteStore, default_notes_dir};
use jarvis_tts::OsVoice;

/// How often the assistant checks for an idle conversation to close.
const TICK: Duration = Duration::from_secs(5);

/// Overrides the notes folder (`Documents/Jarvis Notes`) until settings land.
pub const NOTES_DIR_ENV: &str = "JARVIS_NOTES_DIR";

/// Handle to the assistant thread.
pub struct Engine {
    inputs: Sender<Input>,
    outputs: Receiver<Output>,
}

impl Engine {
    pub fn start(gemini_key: Option<String>) -> Self {
        let (inputs, inbox) = mpsc::channel::<Input>();
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
                    store: open_store(),
                    clock: Box::new(SystemClock),
                });
                loop {
                    let input = match inbox.recv_timeout(TICK) {
                        Ok(input) => input,
                        Err(RecvTimeoutError::Timeout) => Input::Tick,
                        Err(RecvTimeoutError::Disconnected) => break,
                    };
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

/// Used when the OS has no voice.
struct Silent;

impl Speaker for Silent {
    fn speak(&mut self, _: &str) {}
}

fn notes_dir() -> Option<PathBuf> {
    std::env::var_os(NOTES_DIR_ENV)
        .filter(|dir| !dir.is_empty())
        .map(PathBuf::from)
        .or_else(default_notes_dir)
}

/// The SQLite history and notes store, or one that keeps nothing if the
/// data folder can't be used (Jarvis still answers, it just forgets).
fn open_store() -> Box<dyn Store> {
    let (Some(db), Some(notes)) = (jarvis_store::database_path(), notes_dir()) else {
        log::warn!("no data or documents folder; history and notes are off");
        return Box::new(NoHistory);
    };
    match SqliteStore::open(&db, notes.clone()) {
        Ok(store) => {
            log::info!("history in {}, notes in {}", db.display(), notes.display());
            Box::new(store)
        }
        Err(err) => {
            log::warn!(
                "cannot open history at {}: {err}; history and notes are off",
                db.display()
            );
            Box::new(NoHistory)
        }
    }
}

/// Used when the history database can't be opened.
struct NoHistory;

impl Store for NoHistory {
    fn save_message(&mut self, _: SystemTime, _: &Message) {}
    fn end_conversation(&mut self, _: SystemTime, _: &Tagged) {}
    fn clear_history(&mut self) {}
    fn search(&mut self, _: &str, _: usize) -> Vec<MemoryHit> {
        Vec::new()
    }
    fn save_note(&mut self, _: SystemTime, _: &str, _: &Tagged) -> Result<String, String> {
        Err("the notes folder could not be opened".into())
    }
}
