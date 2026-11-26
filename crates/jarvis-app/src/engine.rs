//! Runs the headless assistant on its own thread with the real adapters.
//! The UI sends it inputs and polls the outputs it emits.

use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use std::time::SystemTime;

use jarvis_core::{Assistant, Input, Message, Output, Ports, Speaker, Store, ports::SystemClock};
use jarvis_llm::OpenAiChat;
use jarvis_tools::{ActionSettings, SystemActions};
use jarvis_tts::OsVoice;

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
                    actions: Box::new(system_actions()),
                    chat: Box::new(OpenAiChat::gemini(gemini_key)),
                    speaker,
                    store: Box::new(NoHistory),
                    clock: Box::new(SystemClock),
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

/// The computer actions, configured by `actions.json` in the data folder.
/// The file is written with the defaults on first run so it can be edited.
fn system_actions() -> SystemActions {
    let Some(path) = jarvis_store::data_dir().map(|dir| dir.join("actions.json")) else {
        log::warn!("no data folder on this system; using default action settings");
        return SystemActions::new(ActionSettings::default());
    };
    let settings = ActionSettings::load(&path);
    if !path.exists()
        && let Err(err) = settings.save(&path)
    {
        log::warn!("cannot write {}: {err}", path.display());
    }
    SystemActions::new(settings)
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
