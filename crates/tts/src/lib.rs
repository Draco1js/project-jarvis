//! Speech output: OS voice by default, optional Kokoro.
//!
//! [`OsVoice`] speaks with the operating system's voice through the `tts`
//! crate (SAPI/WinRT on Windows, AVFoundation on macOS, Speech Dispatcher on
//! Linux). The platform handle is not `Send` everywhere, so it lives on its
//! own thread and the [`Speaker`] port just sends it commands.

use std::sync::mpsc;
use std::thread;

use anyhow::{Context, Result, anyhow};
use jarvis_core::Speaker;

enum Command {
    Speak(String),
    Queue(String),
    Stop,
}

/// The OS voice. Cloning is cheap; all clones drive the same voice.
#[derive(Clone)]
pub struct OsVoice {
    commands: mpsc::Sender<Command>,
}

impl OsVoice {
    /// Starts the voice thread. Fails if the OS has no usable speech engine.
    pub fn start() -> Result<Self> {
        let (commands, inbox) = mpsc::channel::<Command>();
        let (ready_tx, ready) = mpsc::channel::<Result<()>>();
        thread::Builder::new()
            .name("jarvis-tts".into())
            .spawn(move || {
                let mut tts = match tts::Tts::default() {
                    Ok(tts) => {
                        let _ = ready_tx.send(Ok(()));
                        tts
                    }
                    Err(err) => {
                        let _ = ready_tx.send(Err(anyhow!("OS voice unavailable: {err}")));
                        return;
                    }
                };
                for command in inbox {
                    let result = match command {
                        Command::Speak(text) => tts.speak(text, true).map(|_| ()),
                        Command::Queue(text) => tts.speak(text, false).map(|_| ()),
                        Command::Stop => tts.stop().map(|_| ()),
                    };
                    if let Err(err) = result {
                        log::warn!("OS voice failed: {err}");
                    }
                }
            })
            .context("could not start the voice thread")?;
        ready
            .recv()
            .context("voice thread exited during start-up")??;
        Ok(Self { commands })
    }

    /// Stops whatever is being said.
    pub fn stop(&self) {
        let _ = self.commands.send(Command::Stop);
    }
}

impl Speaker for OsVoice {
    fn speak(&mut self, text: &str) {
        let _ = self.commands.send(Command::Speak(text.to_string()));
    }

    fn queue(&mut self, text: &str) {
        let _ = self.commands.send(Command::Queue(text.to_string()));
    }
}
