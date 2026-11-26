"""Synthesize 16 kHz mono int16 test clips with Piper (positive and negative).

Usage: python -I make_clips.py <piper-voices-dir> <out-dir>
"""
import io
import sys
import wave

import numpy as np
from piper import PiperVoice
from scipy.signal import resample_poly

voices_dir, out_dir = sys.argv[1], sys.argv[2]
voices = ["en_US-lessac-medium", "en_US-ryan-high", "en_GB-alba-medium", "en_US-amy-medium"]
pos = ["Hey Jarvis.", "Hey Jarvis, what's the weather like today?"]
neg = [
    "The weather is lovely this afternoon.",
    "Hey Siri, play some music.",
    "Harvest the jars of jam in the garage.",
]


def synth(voice, text):
    buf = io.BytesIO()
    with wave.open(buf, "wb") as w:
        voice.synthesize_wav(text, w)
    buf.seek(0)
    with wave.open(buf) as r:
        sr = r.getframerate()
        a = np.frombuffer(r.readframes(r.getnframes()), np.int16).astype(np.float32)
    return resample_poly(a, 16000, sr)


def save(name, a):
    pad = np.zeros(16000, np.float32)  # 1 s silence either side
    a = np.clip(np.concatenate([pad, a, pad]), -32768, 32767).astype(np.int16)
    with wave.open(f"{out_dir}/{name}.wav", "wb") as w:
        w.setnchannels(1)
        w.setsampwidth(2)
        w.setframerate(16000)
        w.writeframes(a.tobytes())


for v in voices:
    pv = PiperVoice.load(f"{voices_dir}/{v}.onnx")
    for i, t in enumerate(pos):
        save(f"pos_{v}_{i}", synth(pv, t))
    for i, t in enumerate(neg):
        save(f"neg_{v}_{i}", synth(pv, t))
rng = np.random.default_rng(3)
save("neg_noise", rng.normal(0, 800, 16000 * 3))
save("neg_silence", np.zeros(16000 * 3))
