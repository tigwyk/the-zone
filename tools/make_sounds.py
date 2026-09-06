"""Synthesises the three cues in assets/audio. Kept in the repo as source of truth
for how they were made; the .wav files themselves are what the game loads."""
import math, os, struct, random

RATE = 22050
os.makedirs('assets/audio', exist_ok=True)


def write(name, samples):
    data = b''.join(struct.pack('<h', max(-32767, min(32767, int(s * 32767)))) for s in samples)
    with open(f'assets/audio/{name}.wav', 'wb') as f:
        f.write(b'RIFF' + struct.pack('<I', 36 + len(data)) + b'WAVEfmt ')
        f.write(struct.pack('<IHHIIHH', 16, 1, 1, RATE, RATE * 2, 2, 16))
        f.write(b'data' + struct.pack('<I', len(data)) + data)
    print(name, len(data) + 44, 'bytes')


def env(i, n, attack=0.002):
    """Fast attack, exponential decay: a click, not a beep."""
    a = int(attack * RATE)
    if i < a:
        return i / a
    return math.exp(-4.0 * (i - a) / max(1, n - a))


# A dry tick for moving the cursor. Short enough to feel like a keyswitch.
n = int(0.020 * RATE)
write('move', [0.18 * env(i, n) * math.sin(2 * math.pi * 1200 * i / RATE) for i in range(n)])

# Two steps up for confirming. Still quiet; this fires a lot.
n = int(0.070 * RATE)
confirm = []
for i in range(n):
    freq = 660 if i < n // 2 else 990
    confirm.append(0.16 * env(i % (n // 2), n // 2) * math.sin(2 * math.pi * freq * i / RATE))
write('confirm', confirm)

# Taking damage: a low thud with noise on it. The only cue that is meant to land.
random.seed(7)
n = int(0.180 * RATE)
hurt = []
low = 0.0
for i in range(n):
    noise = random.uniform(-1.0, 1.0)
    low += (noise - low) * 0.06          # one-pole low pass, so it thuds
    tone = math.sin(2 * math.pi * 90 * i / RATE)
    hurt.append(0.34 * env(i, n, 0.001) * (0.65 * tone + 0.9 * low))
write('hurt', hurt)
