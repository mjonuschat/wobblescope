#!/usr/bin/env python3
"""Write the vibrations-profile fixtures by running Shake&Tune's VibrationsComputation.

Usage: vibrations_regen.py --shaketune DIR --klipper DIR

DIR for --shaketune is a checkout of Frix-x/klippain-shaketune at the pinned commit, DIR for
--klipper a checkout of Klipper at the pin used by regen.py. Shake&Tune takes each step's PSD from
Klipper's own `shaper_calibrate`, so both checkouts are needed.

A seeded numpy generator makes short synthetic Klipper-format recordings (two angles, eight speeds
per angle, two resonances and noise) for one Cartesian and one CoreXY case. The computation runs
on them and every intermediate array is written to tests/data/vibrations_{case}.json.zst, so that
later stages can be compared one by one. The per-step PSDs and energies are captured from the
arguments Shake&Tune passes to its own `_compute_*` methods, not recomputed here.
"""

import argparse
import importlib
import importlib.util
import json
import subprocess
import sys
import types
from pathlib import Path

import numpy as np

SHAKETUNE_PIN = "354ee4be6db218715239f7b9411c2c37fdea23dd"
KLIPPER_PIN = "0cee841e31d701595b3e509a0b502f6e320f52a8"
HERE = Path(__file__).resolve().parent
DATA = HERE.parent / "data"

# numpy 2 removed np.trapz, which Shake&Tune at the pin still calls. np.trapezoid is the same
# function under its new name.
if not hasattr(np, "trapz"):
    np.trapz = np.trapezoid

SAMPLE_RATE = 3200.0
DURATION = 1.5  # seconds per step
SPEEDS = [20.0, 30.0, 40.0, 50.0, 60.0, 70.0, 80.0, 90.0]
MAX_FREQ = 200.0
ACCEL = 3000.0
NOISE_STD = 40.0
# (kinematics, angles, seed, resonance frequencies in Hz, speed of the worst vibrations)
CASES = {
    "cartesian": ("cartesian", [0, 90], 11, (47.0, 118.0), 45.0),
    "corexy": ("corexy", [45, 135], 12, (52.0, 131.0), 40.0),
}


def git(repo: Path, *args: str) -> str:
    return subprocess.run(["git", "-C", str(repo), *args], check=True, capture_output=True, text=True).stdout.strip()


def check_checkout(repo: Path, pin: str, what: str) -> None:
    if git(repo, "rev-parse", "HEAD") != pin:
        sys.exit(f"{what} checkout is not at the pinned commit {pin}")
    if git(repo, "status", "--porcelain"):
        sys.exit(f"{what} checkout is dirty")


def load_module(full_name: str, path: Path, package: str):
    spec = importlib.util.spec_from_file_location(full_name, path)
    module = importlib.util.module_from_spec(spec)
    module.__package__ = package
    sys.modules[full_name] = module
    spec.loader.exec_module(module)
    return module


def load_shaketune(shaketune: Path, klipper: Path):
    """Import VibrationsComputation with Klipper's shaper_calibrate behind it.

    Package __init__ files are replaced by empty modules so that plotting and Klipper plugin code
    is not imported. The computation module, the helpers and the result types are the real ones.
    """
    check_checkout(shaketune, SHAKETUNE_PIN, "Shake&Tune")
    check_checkout(klipper, KLIPPER_PIN, "Klipper")
    st = shaketune / "shaketune"

    sys.path.insert(0, str(klipper / "klippy"))
    shaper_calibrate = importlib.import_module(".shaper_calibrate", "extras")
    for mod in (shaper_calibrate, importlib.import_module("mathutil")):
        if not Path(mod.__file__).resolve().is_relative_to(klipper.resolve()):
            sys.exit(f"{mod.__name__} resolved outside the Klipper checkout")

    sys.modules["zstandard"] = types.ModuleType("zstandard")
    for name, path in [
        ("shaketune", st),
        ("shaketune.helpers", st / "helpers"),
        ("shaketune.graph_creators", st / "graph_creators"),
        ("shaketune.graph_creators.computations", st / "graph_creators" / "computations"),
    ]:
        pkg = types.ModuleType(name)
        pkg.__path__ = [str(path)]
        pkg.__package__ = name
        sys.modules[name] = pkg

    # The two functions Shake&Tune's graph_creators/__init__.py offers to its computations, with
    # the call it makes: Klipper's process_accelerometer_data(name, data) at this Klipper pin.
    graph_creators = sys.modules["shaketune.graph_creators"]
    graph_creators.get_shaper_calibrate_module = lambda: (shaper_calibrate.ShaperCalibrate(printer=None), None)
    graph_creators.process_accelerometer_data_compat = lambda calibrate, data, name=None: (
        calibrate.process_accelerometer_data(name, data)
    )

    accelerometer = types.ModuleType("shaketune.helpers.accelerometer")
    accelerometer.Measurement = dict
    sys.modules["shaketune.helpers.accelerometer"] = accelerometer

    class SilentOutput:
        @staticmethod
        def print(*_args, **_kwargs) -> None:
            pass

    console = types.ModuleType("shaketune.helpers.console_output")
    console.ConsoleOutput = SilentOutput
    sys.modules["shaketune.helpers.console_output"] = console

    for name in ("common_func", "motors_config_parser"):
        load_module(f"shaketune.helpers.{name}", st / "helpers" / f"{name}.py", "shaketune.helpers")
    graph = st / "graph_creators"
    # base_models imports matplotlib only for type annotations, so a stub figure module will do.
    matplotlib = types.ModuleType("matplotlib")
    matplotlib.__path__ = []
    figure = types.ModuleType("matplotlib.figure")
    figure.Figure = object
    sys.modules.setdefault("matplotlib", matplotlib)
    sys.modules.setdefault("matplotlib.figure", figure)
    load_module("shaketune.graph_creators.base_models", graph / "base_models.py", "shaketune.graph_creators")
    load_module(
        "shaketune.graph_creators.computation_results", graph / "computation_results.py", "shaketune.graph_creators"
    )
    module = load_module(
        "shaketune.graph_creators.computations.vibrations_computation",
        graph / "computations" / "vibrations_computation.py",
        "shaketune.graph_creators.computations",
    )
    return module.VibrationsComputation


def synthetic_step(rng, angle: int, speed: float, freqs, worst_speed: float, angle_gain: float) -> np.ndarray:
    """One recording of t, x, y, z: two resonances whose strength peaks near `worst_speed`."""
    n = int(DURATION * SAMPLE_RATE)
    t = np.round(1000.0 + np.arange(n) / SAMPLE_RATE, 6)
    bump = 1.0 + 9.0 * np.exp(-(((speed - worst_speed) / 7.0) ** 2))
    amp = ACCEL * 0.02 * angle_gain * bump * (0.5 + speed / 100.0)
    cols = []
    for axis_gain in (1.0, 0.7, 0.3):
        signal = rng.normal(0.0, NOISE_STD, n)
        for freq, share in zip(freqs, (1.0, 0.6)):
            signal += amp * share * axis_gain * np.sin(2 * np.pi * freq * (t - 1000.0) + rng.uniform(0, 2 * np.pi))
        cols.append(np.round(signal, 1))
    return np.column_stack([t, *cols])


def case_steps(angles, seed: int, freqs, worst_speed: float):
    rng = np.random.default_rng(seed)
    steps = []
    for index, angle in enumerate(angles):
        for speed in SPEEDS:
            data = synthetic_step(rng, angle, speed, freqs, worst_speed, 1.0 + 0.4 * index)
            steps.append({"angle": angle, "speed": speed, "samples": data})
    return steps


def plain(value):
    """numpy values and tuples as JSON-ready Python values."""
    if isinstance(value, dict):
        return {str(k): plain(v) for k, v in value.items()}
    if isinstance(value, (list, tuple)):
        return [plain(v) for v in value]
    if isinstance(value, np.ndarray):
        return plain(value.tolist())
    if isinstance(value, (np.floating, np.integer)):
        return value.item()
    return value


def run_case(computation_class, name: str, kinematics: str, angles, seed: int, freqs, worst_speed: float) -> dict:
    steps = case_steps(angles, seed, freqs, worst_speed)
    measurements = [
        {"name": f"an{s['angle']}_0sp{int(s['speed'])}_0", "samples": s["samples"].tolist()} for s in steps
    ]
    computation = computation_class(measurements, kinematics, ACCEL, MAX_FREQ, None, "oracle")

    # The per-step PSDs and energies are locals of compute(); take them from the calls it makes.
    captured = {}
    spectrogram = computation._compute_dir_speed_spectrogram
    motor_profiles = computation._compute_motor_profiles

    def spy_spectrogram(measured_speeds, data, *args, **kwargs):
        captured["energies"] = data
        return spectrogram(measured_speeds, data, *args, **kwargs)

    def spy_motor_profiles(freqs_, psds, *args, **kwargs):
        captured["psds"] = psds
        return motor_profiles(freqs_, psds, *args, **kwargs)

    computation._compute_dir_speed_spectrogram = spy_spectrogram
    computation._compute_motor_profiles = spy_motor_profiles
    result = computation.compute()
    # Shake&Tune keeps the low-frequency flag in a local of compute(), so ask its own helper again
    # with the arguments compute() passes.
    common_func = sys.modules["shaketune.helpers.common_func"]
    *_, lowfreq_max = common_func.compute_mechanical_parameters(result.global_motor_profile, result.target_freqs, 30)

    return plain(
        {
            "kinematics": kinematics,
            "accel": ACCEL,
            "max_freq": MAX_FREQ,
            "steps": [
                {
                    "angle": s["angle"],
                    "speed": s["speed"],
                    "t": s["samples"][:, 0],
                    "x": s["samples"][:, 1],
                    "y": s["samples"][:, 2],
                    "z": s["samples"][:, 3],
                }
                for s in steps
            ],
            "target_freqs": result.target_freqs,
            "psds": [
                {"angle": a, "speed": sp, "psd": p} for a, by_speed in captured["psds"].items() for sp, p in by_speed.items()
            ],
            "energies": [
                {"angle": a, "speed": sp, "energy": e}
                for a, by_speed in captured["energies"].items()
                for sp, e in by_speed.items()
            ],
            "main_angles": result.main_angles,
            "all_angles": result.all_angles,
            "all_speeds": result.all_speeds,
            "spectrogram_data": result.spectrogram_data,
            "all_angles_energy": result.all_angles_energy,
            "sp_min_energy": result.sp_min_energy,
            "sp_max_energy": result.sp_max_energy,
            "sp_variance_energy": result.sp_variance_energy,
            "vibration_metric": result.vibration_metric,
            "num_peaks": result.num_peaks,
            "vibration_peaks": result.vibration_peaks,
            "good_speeds": result.good_speeds,
            "good_angles": result.good_angles,
            "motor_profiles": result.motor_profiles,
            "global_motor_profile": result.global_motor_profile,
            "symmetry_factor": result.symmetry_factor,
            "motor_fr": result.motor_fr,
            "motor_zeta": result.motor_zeta,
            "motor_res_idx": result.motor_res_idx,
            "motor_lowfreq_max": lowfreq_max,
        }
    )


def write(name: str, obj: dict) -> None:
    DATA.mkdir(parents=True, exist_ok=True)
    text = json.dumps(obj, allow_nan=False, separators=(",", ":")) + "\n"
    subprocess.run(
        ["zstd", "-q", "-19", "-T1", "-f", "-o", str(DATA / f"vibrations_{name}.json.zst")],
        input=text.encode(),
        check=True,
    )
    print("wrote", name)


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--shaketune", required=True, type=Path)
    ap.add_argument("--klipper", required=True, type=Path)
    args = ap.parse_args()
    computation_class = load_shaketune(args.shaketune.expanduser().resolve(), args.klipper.expanduser().resolve())
    for name, (kinematics, angles, seed, freqs, worst_speed) in CASES.items():
        write(name, run_case(computation_class, name, kinematics, angles, seed, freqs, worst_speed))


if __name__ == "__main__":
    main()
