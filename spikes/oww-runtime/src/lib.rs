//! Throwaway spike for issue #3: run openWakeWord's three ONNX models in Rust.
//!
//! `WakeStream` is a straight port of openWakeWord 0.6.0's streaming path
//! (`AudioFeatures._streaming_features` + `Model.predict`) for 1280-sample
//! (80 ms) chunks. The ONNX runtime sits behind `Runtime` so tract and ort can
//! be compared on identical glue code.

use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};

use anyhow::Result;

mod tract_rt;

pub const CHUNK: usize = 1280;
const MEL_BINS: usize = 32;
const EMB_WINDOW: usize = 76;
const EMB_DIM: usize = 96;
const CLS_FRAMES: usize = 16;
const RAW_MAX: usize = 16000 * 10;
const MEL_MAX: usize = 10 * 97;
const FEAT_MAX: usize = 120;

#[derive(Clone, Copy, Debug)]
pub enum Backend {
    Tract,
}

/// Runs one ONNX model on an f32 tensor and returns its flattened first output.
pub trait Runtime {
    fn run(&mut self, input: &[f32], shape: &[usize]) -> Result<Vec<f32>>;
}

fn load(backend: Backend, path: PathBuf) -> Result<Box<dyn Runtime>> {
    match backend {
        Backend::Tract => Ok(Box::new(tract_rt::TractModel::new(path))),
    }
}

pub struct WakeStream {
    mel: Box<dyn Runtime>,
    emb: Box<dyn Runtime>,
    cls: Box<dyn Runtime>,
    raw: VecDeque<i16>,
    mel_buf: VecDeque<[f32; MEL_BINS]>,
    feat_buf: VecDeque<[f32; EMB_DIM]>,
    frames_scored: usize,
}

impl WakeStream {
    pub fn new(backend: Backend, models: &Path) -> Result<Self> {
        let mut s = Self {
            mel: load(backend, models.join("melspectrogram.onnx"))?,
            emb: load(backend, models.join("embedding_model.onnx"))?,
            cls: load(backend, models.join("hey_jarvis_v0.1.onnx"))?,
            raw: VecDeque::with_capacity(RAW_MAX),
            mel_buf: std::iter::repeat_n([1.0; MEL_BINS], EMB_WINDOW).collect(),
            feat_buf: VecDeque::new(),
            frames_scored: 0,
        };
        // openWakeWord seeds this with embeddings of random noise; we use 4 s of
        // silence (the Python reference does the same, see scripts/reference.py).
        let spec = s.melspec(&vec![0i16; 16000 * 4])?;
        let mut i = 0;
        while i + EMB_WINDOW <= spec.len() {
            let e = s.embed(&spec[i..i + EMB_WINDOW])?;
            s.feat_buf.push_back(e);
            i += 8;
        }
        Ok(s)
    }

    /// Feeds one 80 ms chunk and returns the hey_jarvis score for it.
    pub fn push(&mut self, chunk: &[i16]) -> Result<f32> {
        anyhow::ensure!(chunk.len() == CHUNK, "chunks must be {CHUNK} samples");
        self.raw.extend(chunk);
        while self.raw.len() > RAW_MAX {
            self.raw.pop_front();
        }
        let take = (CHUNK + 160 * 3).min(self.raw.len());
        let tail: Vec<i16> = self.raw.iter().skip(self.raw.len() - take).copied().collect();
        for row in self.melspec(&tail)? {
            self.mel_buf.push_back(row);
        }
        while self.mel_buf.len() > MEL_MAX {
            self.mel_buf.pop_front();
        }
        let window: Vec<[f32; MEL_BINS]> =
            self.mel_buf.iter().skip(self.mel_buf.len() - EMB_WINDOW).copied().collect();
        let e = self.embed(&window)?;
        self.feat_buf.push_back(e);
        while self.feat_buf.len() > FEAT_MAX {
            self.feat_buf.pop_front();
        }

        let feats: Vec<f32> = self
            .feat_buf
            .iter()
            .skip(self.feat_buf.len() - CLS_FRAMES)
            .flatten()
            .copied()
            .collect();
        let score = self.cls.run(&feats, &[1, CLS_FRAMES, EMB_DIM])?[0];
        self.frames_scored += 1;
        // Model.predict zeroes the first 5 frames.
        Ok(if self.frames_scored <= 5 { 0.0 } else { score })
    }

    fn melspec(&mut self, samples: &[i16]) -> Result<Vec<[f32; MEL_BINS]>> {
        let x: Vec<f32> = samples.iter().map(|&s| s as f32).collect();
        let out = self.mel.run(&x, &[1, x.len()])?;
        Ok(out
            .chunks_exact(MEL_BINS)
            .map(|r| std::array::from_fn(|i| r[i] / 10.0 + 2.0))
            .collect())
    }

    fn embed(&mut self, window: &[[f32; MEL_BINS]]) -> Result<[f32; EMB_DIM]> {
        let x: Vec<f32> = window.iter().flatten().copied().collect();
        let out = self.emb.run(&x, &[1, EMB_WINDOW, MEL_BINS, 1])?;
        Ok(out[..EMB_DIM].try_into()?)
    }
}

/// Caches one compiled plan per input shape (models have dynamic dims).
pub(crate) struct PlanCache<P> {
    pub path: PathBuf,
    pub plans: HashMap<Vec<usize>, P>,
}

pub fn read_clip(path: &Path) -> Result<Vec<i16>> {
    let mut r = hound::WavReader::open(path)?;
    Ok(r.samples::<i16>().collect::<Result<_, _>>()?)
}
