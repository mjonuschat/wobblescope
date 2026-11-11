#!/usr/bin/env python3
"""Generate deterministic synthetic accelerometer CSVs in Klipper's raw format.

Run once; the output is committed. Not part of CI.
"""

import math
from pathlib import Path

import numpy as np

OUT = Path(__file__).resolve().parent.parent / "data" / "synthetic"
DURATION = 10.0
T0 = 4096.0
F_START, F_END = 5.0, 130.0


def chirp(fs: float) -> np.ndarray:
    n = int(DURATION * fs)
    t = np.arange(n) / fs
    k = (F_END - F_START) / DURATION
    return np.sin(2.0 * math.pi * (F_START * t + 0.5 * k * t * t))


def resonator(u: np.ndarray, fs: float, freq: float, zeta: float) -> np.ndarray:
    """Impulse-invariant damped second-order mode driven by u, one sample at a time."""
    dt = 1.0 / fs
    w = 2.0 * math.pi * freq
    wd = w * math.sqrt(1.0 - zeta * zeta)
    r = math.exp(-zeta * w * dt)
    a1 = 2.0 * r * math.cos(wd * dt)
    a2 = -r * r
    b = r * math.sin(wd * dt) / wd * dt
    y = np.zeros_like(u)
    for i in range(2, len(u)):
        y[i] = a1 * y[i - 1] + a2 * y[i - 2] + b * u[i - 1]
    return y


def make(name, fs, x_modes, y_modes, seed_offset):
    rng = np.random.default_rng(1 + seed_offset)
    u = chirp(fs) * (2.0 * math.pi * 60.0) ** 2 * 40.0
    n = len(u)
    noise = 30.0

    def axis(modes):
        y = np.zeros(n)
        for freq, zeta, gain in modes:
            y += gain * resonator(u, fs, freq, zeta)
        return y + rng.normal(0.0, noise, n)

    x = axis(x_modes)
    y = axis(y_modes)
    z = rng.normal(0.0, noise, n) + 9800.0
    t = T0 + np.arange(n) / fs
    path = OUT / f"{name}.csv"
    with open(path, "w") as f:
        f.write("#time,accel_x,accel_y,accel_z\n")
        f.writelines(
            "{:.5f},{:.5f},{:.5f},{:.5f}\n".format(*row) for row in zip(t, x, y, z)
        )
    print("wrote", path)


def main():
    OUT.mkdir(parents=True, exist_ok=True)
    make(
        "chip_a",
        3200.0,
        [(48.0, 0.05, 1.0), (112.0, 0.05, 0.5)],
        [(62.0, 0.05, 0.8)],
        0,
    )
    make(
        "chip_b",
        1600.0,
        [(51.0, 0.05, 0.7), (112.0, 0.05, 0.6)],
        [(60.0, 0.05, 1.0)],
        1,
    )


if __name__ == "__main__":
    main()
