#!/usr/bin/env python3
"""Write shaketune.json by running Shake&Tune's own functions at the pinned commit.

The inputs are the normalised PSDs and the per-shaper results already stored in the Klipper
goldens, so the expected values depend only on Shake&Tune's code, not on our Welch step.

Usage: shaketune.py --shaketune DIR   (DIR is a checkout of Frix-x/klippain-shaketune at PIN)
"""

import argparse
import importlib.util
import json
import subprocess
import sys
import types
from typing import NamedTuple
from pathlib import Path

import numpy as np

PIN = "354ee4be6db218715239f7b9411c2c37fdea23dd"
HERE = Path(__file__).resolve().parent
MAX_FREQ = 200.0
PEAKS_DETECTION_THRESHOLD = 0.05
PEAKS_EFFECT_THRESHOLD = 0.12
MAX_VIBRATIONS = 5.0


def load_common(root: Path):
    path = root / "shaketune" / "helpers" / "common_func.py"
    spec = importlib.util.spec_from_file_location("st_common_func", path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def read_golden(path: Path) -> dict:
    return json.loads(subprocess.run(["zstd", "-d", "-c", str(path)], check=True, capture_output=True).stdout)


def peaks_of(common, freq_bins, psd_sum) -> dict:
    freqs = np.array(freq_bins)
    psd = np.array(psd_sum)
    mask = freqs <= MAX_FREQ
    freqs, psd = freqs[mask], psd[mask]
    detect = PEAKS_DETECTION_THRESHOLD * psd.max()
    effect = PEAKS_EFFECT_THRESHOLD * psd.max()
    _, peaks, peak_freqs = common.detect_peaks(psd, freqs, detect)
    return {
        "peaks": [
            {"freq": float(f), "level": float(psd[p]), "above_effect": bool(psd[p] > effect)}
            for p, f in zip(peaks, peak_freqs)
        ],
        "detect_threshold": float(detect),
        "effect_threshold": float(effect),
    }


def dataset_entry(common, name, freq_bins, psd_sum) -> dict:
    fr, zeta, _, _ = common.compute_mechanical_parameters(np.array(psd_sum), np.array(freq_bins))
    return {
        "name": name,
        "fr": float(fr),
        "zeta": None if zeta is None else float(zeta),
        **peaks_of(common, freq_bins, psd_sum),
    }


def performance_of(golden: dict) -> dict:
    best = golden["best"]["name"]
    best_accel = golden["fit"][best]["final"]["max_accel"]
    pick, pick_accel = None, 0.0
    for name, entry in golden["fit"].items():
        final = entry["final"]
        if pick_accel < final["max_accel"] and final["vibrs"] * 100 < MAX_VIBRATIONS:
            pick, pick_accel = name, final["max_accel"]
    shown = pick is not None and pick != best and pick_accel >= best_accel
    return {"name": pick, "shown": shown}


def edge_case(common, name, psd) -> dict:
    freq_bins = [i * 1.5625 for i in range(len(psd))]
    return {"name": name, "freq_bins": freq_bins, "psd": [float(v) for v in psd], **peaks_of(common, freq_bins, psd)}


def rounding_noise_cases(common) -> dict:
    """Spectra where the padded mean and the convolution of the first five bins differ by one ulp.

    That makes a "peak" at smoothed index 1 or 2 possible, which Shake&Tune then refines to a
    negative index that Python wraps to the end of the array. Found by a seeded search so the
    inputs are reproducible.
    """
    import random

    rng = random.Random(1)
    found = {}
    kernel = np.ones(5) / 5
    while len(found) < 2:
        d = np.array([rng.uniform(0.5, 1.5) for _ in range(5)] + [rng.uniform(0.0, 0.2) for _ in range(58)] + [2.0])
        s = np.concatenate(([np.mean(d[:5])] * 2, np.convolve(d, kernel, mode="valid")))
        if not (s[1] < s[2] > s[3]):
            continue
        _, peaks, _ = common.detect_peaks(d, np.arange(len(d)) * 1.0, PEAKS_DETECTION_THRESHOLD * d.max())
        if min(peaks) < 0 and "ulp_noise_wraps_to_the_end" not in found:
            found["ulp_noise_wraps_to_the_end"] = d
        elif len(set(peaks.tolist())) < len(peaks) and "ulp_noise_and_duplicate_peaks" not in found:
            found["ulp_noise_and_duplicate_peaks"] = d
    return found


def edge_psds(common) -> dict:
    spike_in_bin_1 = [0.1 + 0.01 * (i % 3) for i in range(64)]
    spike_in_bin_1[1] = 10.0
    close = [0.0] * 64
    close[20], close[24] = 1.0, 0.9
    at_the_end = [0.0] * 64
    at_the_end[62], at_the_end[63] = 1.0, 1.0
    return {
        "peak_in_bin_1": spike_in_bin_1,
        "two_peaks_four_bins_apart_merge": close,
        "a_spike_in_the_last_two_bins": at_the_end,
        **rounding_noise_cases(common),
    }


def load_spectrogram_module(root: Path):
    path = root / "shaketune" / "helpers" / "spectrogram.py"
    spec = importlib.util.spec_from_file_location("st_spectrogram", path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def read_samples(path: Path) -> np.ndarray:
    """A Klipper accelerometer CSV (plain or zstd) as an (N, 4) array of t, x, y, z."""
    if path.suffix == ".zst":
        text = subprocess.run(["zstd", "-d", "-c", str(path)], check=True, capture_output=True, text=True).stdout
    else:
        text = path.read_text()
    rows = [[float(v) for v in line.split(",")] for line in text.splitlines() if line and not line.startswith("#")]
    return np.array(rows)


SPECTROGRAM_ROWS = 20


def spectrogram_entry(module, data: np.ndarray) -> dict:
    """Shake&Tune's compute_spectrogram, with the matrix cut at 200 Hz and kept for the first rows."""
    pdata, times, freqs = module.compute_spectrogram(data)
    keep = freqs <= MAX_FREQ
    return {
        "n_segments": int(pdata.shape[1]),
        "times": [float(t) for t in times],
        "freq_bins": [float(f) for f in freqs[keep]],
        "rows": [[float(v) for v in pdata[keep, i]] for i in range(SPECTROGRAM_ROWS)],
        "floor": float(np.percentile(pdata, 5)),
        "max": float(pdata.max()),
    }


BELT_GRID_STEP = 1.5625
BELT_GRID_LEN = 161


def resonators(freqs, peaks, width=3.0):
    """Sum of Lorentzian lines, one (centre, height) pair per resonator."""
    return sum(h / (1.0 + ((freqs - f0) / width) ** 2) for f0, h in peaks)


def belt_case(name, rng, peaks_a, peaks_b, noise_b=0.02, max_freq=200.0):
    freqs = np.arange(BELT_GRID_LEN) * BELT_GRID_STEP
    floor = 0.01

    def spectrum(peaks, noise):
        return resonators(freqs, peaks) + floor + noise * rng.random(len(freqs))

    return {
        "name": name,
        "max_freq": max_freq,
        "freqs_a": freqs.tolist(),
        "psd_a": spectrum(peaks_a, 0.02).tolist(),
        "freqs_b": freqs.tolist(),
        "psd_b": spectrum(peaks_b, noise_b).tolist(),
    }


def belts_cases() -> list:
    rng = np.random.default_rng(20261113)
    two = [(55.0, 1.0), (110.0, 0.6)]
    cases = [
        belt_case("matched_one_peak", rng, [(55.0, 1.0)], [(55.0, 0.9)]),
        belt_case("matched_two_peaks", rng, two, [(55.0, 0.9), (110.0, 0.55)]),
        belt_case("shifted_peak_6hz", rng, [(55.0, 1.0)], [(61.0, 0.9)]),
        belt_case("extra_peak_on_b", rng, two, two + [(160.0, 0.5)]),
        belt_case("weak_noisy_b", rng, two, [(55.0, 0.3), (110.0, 0.18)], noise_b=0.04),
        belt_case("max_freq_100", rng, two, [(55.0, 0.9), (110.0, 0.55)], max_freq=100.0),
    ]
    anti = belt_case("anticorrelated", rng, [(60.0, 1.0)], [], noise_b=0.01)
    peak = resonators(np.array(anti["freqs_a"]), [(60.0, 1.0)])
    anti["psd_b"] = (1.2 - peak + 0.01 * rng.random(BELT_GRID_LEN)).tolist()
    cases.append(anti)
    flat = belt_case("flat_b", rng, two, [])
    flat["psd_b"] = [0.5] * BELT_GRID_LEN
    cases.append(flat)
    by_name = {c["name"]: c for c in cases}
    # same inputs as matched_two_peaks, only max_freq differs
    for key in ("freqs_a", "psd_a", "freqs_b", "psd_b"):
        by_name["max_freq_100"][key] = by_name["matched_two_peaks"][key]
    return cases


class _SignalData(NamedTuple):
    freqs: np.ndarray
    psd: np.ndarray
    peaks: np.ndarray
    paired_peaks: list = []
    unpaired_peaks: list = []


class _Record:
    def __init__(self, **kwargs):
        self.__dict__.update(kwargs)


class _Calibration:
    def __init__(self, freq_bins, psd):
        self.freq_bins = freq_bins
        self._psd = psd

    def get_psd(self, axis):
        assert axis == "all"
        return self._psd


def load_belts_computation(root: Path, common):
    """Shake&Tune's belts_computation.py, run unmodified, with its relative imports stubbed.

    The stubbed accelerometer step reads the case's arrays, passed as the measurement's
    "samples" (row 0 the frequencies, row 1 the PSD).
    """

    def module(name, **attrs):
        mod = types.ModuleType(name)
        mod.__dict__.update(attrs)
        mod.__path__ = []
        sys.modules[name] = mod
        return mod

    base = "st_pin"
    module(base)
    module(f"{base}.helpers")
    module(f"{base}.helpers.accelerometer", Measurement=dict)
    module(f"{base}.helpers.common_func", detect_peaks=common.detect_peaks)
    module(f"{base}.helpers.console_output", ConsoleOutput=_Record(print=lambda *_: None))
    module(
        f"{base}.graph_creators",
        get_shaper_calibrate_module=lambda: (None, None),
        process_accelerometer_data_compat=lambda _shaper, data: _Calibration(data[0], data[1]),
    )
    module(f"{base}.graph_creators.base_models", GraphMetadata=_Record)
    module(f"{base}.graph_creators.computation_results", BeltsResult=_Record, SignalData=_SignalData)
    module(f"{base}.graph_creators.computations")
    path = root / "shaketune" / "graph_creators" / "computations" / "belts_computation.py"
    name = f"{base}.graph_creators.computations.belts_computation"
    spec = importlib.util.spec_from_file_location(name, path)
    mod = importlib.util.module_from_spec(spec)
    sys.modules[name] = mod
    spec.loader.exec_module(mod)
    return mod


def belts_entry(computation_module, case: dict) -> dict:
    samples = [
        np.array([case["freqs_a"], case["psd_a"]]),
        np.array([case["freqs_b"], case["psd_b"]]),
    ]
    measurements = [
        {"name": "belt_A", "samples": samples[0]},
        {"name": "belt_B", "samples": samples[1]},
    ]
    result = computation_module.BeltsComputation(
        measurements, "corexy", case["max_freq"], None, None, "pinned"
    ).compute()
    a, b = result.signal1, result.signal2
    similarity = float(result.similarity_factor)
    return {
        **case,
        "expected": {
            "freqs": a.freqs.tolist(),
            "psd_a": a.psd.tolist(),
            "psd_b": b.psd.tolist(),
            "peaks_a": [int(p) for p in a.peaks],
            "peaks_b": [int(p) for p in b.peaks],
            "pairs": [[int(p1[0]), int(p2[0])] for p1, p2 in a.paired_peaks],
            "unpaired_a": [int(p) for p in a.unpaired_peaks],
            "unpaired_b": [int(p) for p in b.unpaired_peaks],
            "similarity": None if np.isnan(similarity) else similarity,
            "health": result.mhi,
        },
    }


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--shaketune", required=True, type=Path)
    root = parser.parse_args().shaketune
    head = subprocess.run(["git", "-C", str(root), "rev-parse", "HEAD"], capture_output=True, text=True).stdout.strip()
    if head != PIN:
        sys.exit(f"shaketune checkout is at {head}, expected {PIN}")
    common = load_common(root)

    cases = {}
    for path in sorted(HERE.glob("*.json.zst")):
        golden = read_golden(path)
        name = path.name.removesuffix(".json.zst")
        cases[name] = {
            "datasets": [
                dataset_entry(common, d["name"], d["normalized"]["freq_bins"], d["normalized"]["psd_sum"])
                for d in golden["datasets"]
            ],
            "performance": performance_of(golden),
        }

    edge_cases = [
        edge_case(common, name, psd) for name, psd in edge_psds(common).items()
    ]

    spectrogram_module = load_spectrogram_module(root)
    data_dir = HERE.parent / "data"
    spectrograms = {
        "chip_a": spectrogram_entry(spectrogram_module, read_samples(data_dir / "synthetic" / "chip_a.csv")),
        "ws_x_adxl345": spectrogram_entry(spectrogram_module, read_samples(data_dir / "real" / "ws_x_adxl345.csv.zst")),
    }

    belts_module = load_belts_computation(root, common)
    with np.errstate(all="ignore"):
        belts = [belts_entry(belts_module, case) for case in belts_cases()]

    out = {"pin": PIN, "cases": cases, "edge_cases": edge_cases, "spectrograms": spectrograms, "belts": belts}
    (HERE / "shaketune.json").write_text(json.dumps(out, indent=1) + "\n")


if __name__ == "__main__":
    main()
