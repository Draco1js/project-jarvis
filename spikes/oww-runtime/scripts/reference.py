"""openWakeWord Python reference: per-frame hey_jarvis scores for every clip.

Feeds each clip to `openwakeword.Model.predict` in 80 ms (1280-sample) chunks, the
way a live mic stream would, and writes `clip,frame,score` rows to stdout.

One deviation from stock openWakeWord: the preprocessor seeds its embedding buffer
with embeddings of *random* noise (np.random), which no other implementation can
reproduce. We reseed it with embeddings of 4 s of silence so the Rust port can match
every frame. This only affects the first 16 frames (1.28 s) of a stream.

Usage: python -I reference.py <models-dir> <clip.wav>...
"""
import sys
import wave
from pathlib import Path

import numpy as np
from openwakeword.model import Model

models = Path(sys.argv[1])
clips = sys.argv[2:]

print("clip,frame,score")
for clip in clips:
    m = Model(
        wakeword_models=[str(models / "hey_jarvis_v0.1.onnx")],
        melspec_model_path=str(models / "melspectrogram.onnx"),
        embedding_model_path=str(models / "embedding_model.onnx"),
        inference_framework="onnx",
    )
    pre = m.preprocessor
    pre.feature_buffer = pre._get_embeddings(np.zeros(16000 * 4, np.int16))
    with wave.open(clip) as r:
        assert r.getframerate() == 16000 and r.getnchannels() == 1 and r.getsampwidth() == 2
        audio = np.frombuffer(r.readframes(r.getnframes()), np.int16)
    name = Path(clip).stem
    for f in range(len(audio) // 1280):
        score = m.predict(audio[f * 1280:(f + 1) * 1280])["hey_jarvis_v0.1"]
        print(f"{name},{f},{float(score):.6f}")
