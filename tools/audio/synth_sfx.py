"""Synthesize period SFX in-house (stdlib only, no samples — legally
bulletproof per docs/research/audio.md): a teletype burst for event
popups, an EBS-style two-tone attention signal (853+960 Hz, the real
frequencies — pure tones are not copyrightable), and the Brink music
layer (the design strips music back to a bare pulse at Brink).

Usage: python3 tools/audio/synth_sfx.py
Writes assets/audio/ui/teletype.wav and alert.wav, and
assets/audio/music/brink_pulse.wav (mono 22050 Hz).
"""
import math
import os
import random
import struct
import wave

RATE = 22050
OUT = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "..", "assets", "audio", "ui")

random.seed(1950)  # reproducible assets


def write_wav(name, samples):
    path = os.path.join(OUT, name)
    with wave.open(path, "w") as w:
        w.setnchannels(1)
        w.setsampwidth(2)
        w.setframerate(RATE)
        frames = b"".join(
            struct.pack("<h", max(-32767, min(32767, int(s * 32767)))) for s in samples
        )
        w.writeframes(frames)
    print(f"wrote {path} ({len(samples)/RATE:.2f}s)")


def teletype(duration=1.9, strikes_per_sec=11.0):
    """Mechanical print head: sharp filtered-noise strikes with a faint
    carriage hum underneath and slight rhythm jitter."""
    n = int(duration * RATE)
    out = [0.0] * n
    # Strike train.
    t = 0.06
    while t < duration - 0.15:
        start = int(t * RATE)
        strike_len = int(0.014 * RATE)
        # Two-part strike: hammer (bright noise) + platen thump (low).
        prev = 0.0
        for i in range(strike_len):
            env = math.exp(-i / (0.0025 * RATE))
            noise = random.uniform(-1, 1)
            # crude high-pass: difference of noise
            hp = noise - prev
            prev = noise
            out[start + i] += 0.55 * env * hp
        thump_len = int(0.02 * RATE)
        for i in range(thump_len):
            env = math.exp(-i / (0.006 * RATE))
            out[start + i] += 0.18 * env * math.sin(2 * math.pi * 130 * i / RATE)
        t += 1.0 / strikes_per_sec * random.uniform(0.82, 1.22)
    # Motor hum bed + gentle fade in/out.
    for i in range(n):
        hum = 0.02 * math.sin(2 * math.pi * 50 * i / RATE)
        fade = min(1.0, i / (0.05 * RATE), (n - i) / (0.25 * RATE))
        out[i] = (out[i] + hum) * fade
    return out


def brink_pulse(target_duration=24.0, bpm=45.0):
    """Brink-band music bed: sub-bass thumps at a slow clock rate under
    one thin, tremolo'd high sine. Loop-clean by construction — a whole
    number of pulse periods, each thump fully decayed inside its period,
    and integer tone/tremolo cycles across the loop."""
    period = 60.0 / bpm
    n_periods = max(1, round(target_duration / period))
    duration = n_periods * period
    n = int(duration * RATE)
    out = [0.0] * n
    for p in range(n_periods):
        start = int(p * period * RATE)
        for i in range(int(0.7 * RATE)):
            if start + i >= n:
                break
            t = i / RATE
            env = math.exp(-t * 7.0) * min(1.0, t / 0.008)
            out[start + i] += 0.55 * env * math.sin(2 * math.pi * 42.0 * t)
    # The one high string-like tone: integer cycles so the loop seam is
    # phase-continuous; tremolo starts and ends at zero.
    freq = round(1244.0 * duration) / duration
    trem_hz = max(1, round(duration / 7.0)) / duration
    for i in range(n):
        t = i / RATE
        trem = 0.5 + 0.5 * math.sin(2 * math.pi * trem_hz * t - math.pi / 2)
        out[i] += 0.045 * trem * math.sin(2 * math.pi * freq * t)
    return [max(-1.0, min(1.0, s)) for s in out]


def alert(duration=1.4):
    """EBS attention signal: 853 Hz + 960 Hz, band-limited feel."""
    n = int(duration * RATE)
    out = []
    for i in range(n):
        t = i / RATE
        v = 0.30 * math.sin(2 * math.pi * 853 * t) + 0.30 * math.sin(2 * math.pi * 960 * t)
        fade = min(1.0, i / (0.02 * RATE), (n - i) / (0.30 * RATE))
        out.append(v * fade)
    return out


if __name__ == "__main__":
    os.makedirs(OUT, exist_ok=True)
    write_wav("teletype.wav", teletype())
    write_wav("alert.wav", alert())
    os.makedirs(os.path.join(OUT, "..", "music"), exist_ok=True)
    write_wav(os.path.join("..", "music", "brink_pulse.wav"), brink_pulse())
