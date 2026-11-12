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

    freq_bins = [i * 1.5625 for i in range(64)]
    psd = [0.1 + 0.01 * (i % 3) for i in range(64)]
    psd[1] = 10.0
    edge = {"name": "peak_in_bin_1", "freq_bins": freq_bins, "psd": psd}
    edge.update(peaks_of(common, freq_bins, psd))

    out = {"pin": PIN, "cases": cases, "edge_cases": [edge]}
    (HERE / "shaketune.json").write_text(json.dumps(out, indent=1) + "\n")


if __name__ == "__main__":
    main()
