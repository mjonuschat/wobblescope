#!/usr/bin/env python3
"""Regenerate golden JSON by running Klipper's own shaper_calibrate at the pinned commit.

Usage: regen.py --klipper DIR
"""

import argparse
import hashlib
import importlib
import io
import json
import subprocess
import sys
from pathlib import Path

import numpy as np

PIN = "0cee841e31d701595b3e509a0b502f6e320f52a8"
ROOT = Path(__file__).resolve().parents[2]
DATA = ROOT / "crates/core/tests/data"
GOLDEN = ROOT / "crates/core/tests/golden"
AUTOTUNE = ["zv", "mzv", "ei", "2hump_ei", "3hump_ei"]
SHAPER_NAMES = ["zv", "mzv", "zvd", "ei", "2hump_ei", "3hump_ei"]
SOURCES = [
    "klippy/extras/shaper_calibrate.py",
    "klippy/extras/shaper_defs.py",
    "klippy/mathutil.py",
]

DEFAULTS = {
    "scv": 5.0,
    "damping_ratio": None,
    "max_smoothing": None,
    "max_vibrations": None,
    "test_damping_ratios": None,
    "max_freq": None,
}

# case name -> (input files, params overrides)
CASES = {
    "chip_a": (["synthetic/chip_a.csv"], {}),
    "chip_b": (["synthetic/chip_b.csv"], {}),
    "chip_a+chip_b": (["synthetic/chip_a.csv", "synthetic/chip_b.csv"], {}),
    "chip_a.max_smoothing": (["synthetic/chip_a.csv"], {"max_smoothing": 0.1}),
    "chip_a.scv10_damp05": (
        ["synthetic/chip_a.csv"],
        {"scv": 10.0, "damping_ratio": 0.05, "test_damping_ratios": [0.05, 0.1]},
    ),
    "chip_a.max_vibrs": (["synthetic/chip_a.csv"], {"max_vibrations": 0.05}),
}

for _axis in ("x", "y"):
    _a = f"real/ws_{_axis}_adxl345.csv.zst"
    _e = f"real/ws_{_axis}_ebb36.csv.zst"
    CASES[f"real_{_axis}_adxl345"] = ([_a], {})
    CASES[f"real_{_axis}_ebb36"] = ([_e], {})
    CASES[f"real_{_axis}_both"] = ([_a, _e], {})


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def git(klipper: Path, *args: str) -> str:
    return subprocess.run(
        ["git", "-C", str(klipper), *args], check=True, capture_output=True, text=True
    ).stdout.strip()


def load_klipper(klipper: Path):
    if git(klipper, "rev-parse", "HEAD") != PIN:
        sys.exit("Klipper checkout is not at the pinned commit")
    if git(klipper, "status", "--porcelain"):
        sys.exit("Klipper checkout is dirty")
    sys.path.insert(0, str(klipper / "klippy"))
    shaper_calibrate = importlib.import_module(".shaper_calibrate", "extras")
    shaper_defs = importlib.import_module(".shaper_defs", "extras")
    mathutil = importlib.import_module("mathutil")
    for mod in (shaper_calibrate, shaper_defs, mathutil):
        if not Path(mod.__file__).resolve().is_relative_to(klipper.resolve()):
            sys.exit(f"{mod.__name__} resolved outside the Klipper checkout")
    return shaper_calibrate, shaper_defs


def psd_dict(cd):
    return {
        "freq_bins": cd.freq_bins.tolist(),
        "psd_x": cd.psd_x.tolist(),
        "psd_y": cd.psd_y.tolist(),
        "psd_z": cd.psd_z.tolist(),
        "psd_sum": cd.psd_sum.tolist(),
    }


def stem(rel):
    return Path(rel).name.removesuffix(".zst").removesuffix(".csv")


def read_csv(path):
    if path.suffix == ".zst":
        raw = subprocess.run(
            ["zstd", "-dc", str(path)], check=True, capture_output=True
        ).stdout
        return np.loadtxt(io.BytesIO(raw), comments="#", delimiter=",")
    return np.loadtxt(path, comments="#", delimiter=",")


def parse_log(sc, rel):
    """Mirror calibrate_shaper.py:parse_log for a raw CSV; also capture the raw PSD."""
    name = stem(rel)
    data = read_csv(DATA / rel)
    helper = sc.ShaperCalibrate(printer=None)
    cd = helper.process_accelerometer_data(name, data)
    raw = psd_dict(cd)
    cd.normalize_to_frequencies()
    return cd, {"name": name, "raw": raw, "normalized": psd_dict(cd)}


def result_dict(r, arrays=False):
    d = {
        "freq": float(r.freq),
        "vibrs": float(r.vibrs),
        "smoothing": float(r.smoothing),
        "score": float(r.score),
        "max_accel": float(r.max_accel),
    }
    if arrays:
        d["freq_bins"] = r.freq_bins.tolist()
        d["vals"] = r.vals.tolist()
    return d


def run_case(sc, sd, files, overrides):
    params = {**DEFAULTS, **overrides}
    parsed = [parse_log(sc, f) for f in files]
    cd = parsed[0][0]
    for other, _ in parsed[1:]:
        cd.add_data(other)
    helper = sc.ShaperCalibrate(printer=None)
    kw = {
        "damping_ratio": params["damping_ratio"],
        "scv": params["scv"],
        "max_smoothing": params["max_smoothing"],
        "max_vibrations": params["max_vibrations"],
        "test_damping_ratios": params["test_damping_ratios"],
        "max_freq": params["max_freq"],
    }
    best, all_shapers = helper.find_best_shaper(
        cd, shapers=None, shaper_freqs=None, logger=None, **kw
    )
    fit = {}
    for name in AUTOTUNE:
        res = helper.fit_shaper(name, cd, None, **kw)
        final = next(s for s in all_shapers if s.name == name)
        fit[name] = {
            "selected": result_dict(res[0]),
            "candidates": [result_dict(r) for r in res[1:]],
            "final": result_dict(final, arrays=True),
        }
    ranking = [
        {
            "name": s.name,
            "freq": float(s.freq),
            "score": float(s.score),
            "vibrs": float(s.vibrs),
            "smoothing": float(s.smoothing),
        }
        for s in all_shapers
    ]
    return {
        "inputs": [Path(f).name for f in files],
        "params": params,
        "datasets": [p[1] for p in parsed],
        "fit": fit,
        "best": {"name": best.name, "freq": float(best.freq)},
        "ranking": ranking,
        "best_index": next(i for i, s in enumerate(all_shapers) if s is best),
    }


def shapers_json(sd):
    rows = []
    for name in SHAPER_NAMES:
        for freq in (40.0, 65.5):
            for damping in (0.1, 0.05):
                a, t = sd.init_shaper(name, freq, damping)
                rows.append(
                    {
                        "name": name,
                        "freq": freq,
                        "damping_ratio": damping,
                        "a": [float(v) for v in a],
                        "t": [float(v) for v in t],
                    }
                )
    return rows


def write(name, obj):
    """Writes a case golden as zstd-compressed JSON (shapers.json stays plain: it is tiny)."""
    GOLDEN.mkdir(parents=True, exist_ok=True)
    text = json.dumps(obj, allow_nan=False, indent=1) + "\n"
    if name == "shapers":
        (GOLDEN / f"{name}.json").write_text(text)
    else:
        subprocess.run(
            ["zstd", "-q", "-19", "-T1", "-f", "-o", str(GOLDEN / f"{name}.json.zst")],
            input=text.encode(),
            check=True,
        )
    print("wrote", name)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--klipper", required=True, type=Path)
    args = ap.parse_args()
    klipper = args.klipper.expanduser().resolve()
    sc, sd = load_klipper(klipper)

    provenance = {
        "klipper_commit": PIN,
        "sources": {p: sha256(klipper / p) for p in SOURCES},
    }
    inputs = {}
    for path in sorted([*DATA.glob("*/*.csv"), *DATA.glob("*/*.csv.zst")]):
        inputs[str(path.relative_to(DATA))] = sha256(path)
    provenance["inputs"] = inputs

    for case, (files, overrides) in CASES.items():
        out = run_case(sc, sd, files, overrides)
        write(case, {"klipper_commit": PIN, "provenance": provenance, **out})
    write("shapers", shapers_json(sd))


if __name__ == "__main__":
    main()
