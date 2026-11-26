//! Runs the headless assistant on its own thread with the real adapters.
//! The UI sends it inputs and polls the outputs it emits.

use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use std::time::SystemTime;

use jarvis_core::{Assistant, Input, Message, Output, Ports, Speaker, Store, ports::SystemClock};
use jarvis_llm::OpenAiChat;
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
