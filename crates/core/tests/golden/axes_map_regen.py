#!/usr/bin/env python3
"""Regenerate axes-map golden JSON by running Shake&Tune's AxesMapComputation at the pinned commit.

Usage: axes_map_regen.py --shaketune DIR

The script generates nine synthetic cases, runs AxesMapComputation from the reference checkout on
each, and writes the golden as a zstd-compressed JSON file (axes_map.json.zst).  A seeded numpy
generator makes the output deterministic: running this script twice produces identical compressed
bytes (given the same zstd version and options).
"""

import argparse
import importlib.util
import json
import subprocess
import sys
import types
from pathlib import Path

import numpy as np

PIN = "354ee4be6db218715239f7b9411c2c37fdea23dd"
GOLDEN = Path(__file__).resolve().parent

# Physics constants
SAMPLE_RATE = 1600  # Hz
ACCEL = 1500.0  # mm/s²
MAX_VEL = 80.0  # mm/s
DISTANCE = 30.0  # mm
DURATION = 2.2  # seconds per segment (≈ 3520 samples at 1600 Hz)
PRE_REST = 0.5  # seconds of quiet before the move
GRAVITY_MAG = 9810.0  # mm/s²
GRAVITY_MACHINE = np.array([0.0, 0.0, -GRAVITY_MAG])  # down = −machine Z
RINGING_AMP = 0.05 * ACCEL  # 5% of accel, 60 Hz structural ringing
RINGING_FREQ = 60.0  # Hz
NOISE_STD_NORMAL = 100.0  # mm/s² Gaussian noise for most cases
NOISE_STD_NOISY = 600.0  # mm/s² for the noisy_warning case (> 0.3 × ACCEL = 450)


# ---------------------------------------------------------------------------
# Git helpers
# ---------------------------------------------------------------------------


def _git(repo: Path, *args: str) -> str:
    return subprocess.run(
        ["git", "-C", str(repo), *args], check=True, capture_output=True, text=True
    ).stdout.strip()


# ---------------------------------------------------------------------------
# Import shims for the Shake&Tune reference code
# ---------------------------------------------------------------------------


def _setup_stubs(shaketune_dir: Path) -> None:
    """Pre-inject stub modules so the reference code's package imports work without
    installing every dependency (e.g. zstandard, Klipper, etc.).

    Strategy:
    - Stub the shaketune *package* hierarchy (injecting lightweight ModuleType objects
      with __path__ set) so none of the complex __init__.py files are executed.
    - Stub only the *leaf* helper modules whose transitive deps are unavailable:
      - shaketune.helpers.accelerometer  (requires zstandard)
      - shaketune.helpers.console_output (suppress stdout during test generation)
    - Stub the Python-level 'zstandard' package so accelerometer.py can be imported
      if the import machinery ever reaches it.
    - Load the three real computation modules directly via spec_from_file_location so
      their actual logic runs unchanged.
    """
    st = shaketune_dir / "shaketune"

    # -- zstandard (C extension, not installed in this environment) --
    if "zstandard" not in sys.modules:
        sys.modules["zstandard"] = types.ModuleType("zstandard")

    # -- Stub each package level so its __init__.py is never executed --
    for pkg_name, pkg_dir in [
        ("shaketune", st),
        ("shaketune.helpers", st / "helpers"),
        ("shaketune.graph_creators", st / "graph_creators"),
        ("shaketune.graph_creators.computations", st / "graph_creators" / "computations"),
    ]:
        m = types.ModuleType(pkg_name)
        m.__path__ = [str(pkg_dir)]  # type: ignore[attr-defined]
        m.__package__ = pkg_name
        m.__spec__ = None  # type: ignore[assignment]
        sys.modules[pkg_name] = m

    # -- shaketune.helpers.accelerometer: only Measurement is needed --
    accel_mod = types.ModuleType("shaketune.helpers.accelerometer")
    accel_mod.Measurement = dict  # type: ignore[attr-defined]  # TypedDict ≅ dict
    sys.modules["shaketune.helpers.accelerometer"] = accel_mod

    # -- shaketune.helpers.console_output: suppress stdout --
    class _SilentOutput:
        @staticmethod
        def print(*_a, **_kw) -> None:  # noqa: A003
            pass

    console_mod = types.ModuleType("shaketune.helpers.console_output")
    console_mod.ConsoleOutput = _SilentOutput  # type: ignore[attr-defined]
    sys.modules["shaketune.helpers.console_output"] = console_mod


def _load_module(full_name: str, file_path: Path, package: str):
    """Load a single .py file into sys.modules under full_name."""
    spec = importlib.util.spec_from_file_location(full_name, file_path)
    assert spec is not None and spec.loader is not None
    mod = importlib.util.module_from_spec(spec)
    mod.__package__ = package
    sys.modules[full_name] = mod
    spec.loader.exec_module(mod)  # type: ignore[attr-defined]
    return mod


def load_shaketune(shaketune_dir: Path):
    """Verify the checkout, install stubs, and return AxesMapComputation class."""
    actual_pin = _git(shaketune_dir, "rev-parse", "HEAD")
    if actual_pin != PIN:
        sys.exit(f"Shake&Tune checkout is not at pinned commit {PIN!r} (got {actual_pin!r})")
    if _git(shaketune_dir, "status", "--porcelain"):
        sys.exit("Shake&Tune checkout is dirty")

    _setup_stubs(shaketune_dir)
    st = shaketune_dir / "shaketune"

    _load_module(
        "shaketune.graph_creators.base_models",
        st / "graph_creators" / "base_models.py",
        "shaketune.graph_creators",
    )
    _load_module(
        "shaketune.graph_creators.computation_results",
        st / "graph_creators" / "computation_results.py",
        "shaketune.graph_creators",
    )
    amc = _load_module(
        "shaketune.graph_creators.computations.axes_map_computation",
        st / "graph_creators" / "computations" / "axes_map_computation.py",
        "shaketune.graph_creators.computations",
    )
    return amc.AxesMapComputation


# ---------------------------------------------------------------------------
# Synthetic data generation
# ---------------------------------------------------------------------------


def _make_segment(
    rng: np.random.Generator,
    chip_motion_dir,  # 3-vector in chip frame, or None for noise-only
    noise_std: float,
    gravity_chip: np.ndarray,
    t_base: float = 1000.0,
    forward_remap=None,  # optional (3×3) matrix: transforms chip data before writing
) -> np.ndarray:
    """Return a (N, 4) array of [t, x, y, z] samples at SAMPLE_RATE Hz.

    Models a trapezoid velocity move (or pure noise for virtual axes) in chip space,
    then adds a rotated gravity vector, 60 Hz structural ringing, and Gaussian noise.
    If *forward_remap* is set the finished chip-frame signal is multiplied by it
    (emulating Klipper's configured axes_map remapping before delivering data).
    """
    n = int(DURATION * SAMPLE_RATE)
    t = np.arange(n) / SAMPLE_RATE  # relative time within segment

    chip_accel = np.zeros((n, 3))

    if chip_motion_dir is not None:
        # Trapezoid velocity profile -----------------------------------------------
        t_accel = MAX_VEL / ACCEL  # accel/decel phase duration
        dist_accel = MAX_VEL ** 2 / (2.0 * ACCEL)  # distance during each accel phase
        dist_const = DISTANCE - 2.0 * dist_accel  # coasting distance
        t_const = dist_const / MAX_VEL  # coasting phase duration

        t0 = PRE_REST
        t1 = t0 + t_accel  # end of accel phase
        t2 = t1 + t_const  # end of coasting phase
        t3 = t2 + t_accel  # end of decel phase

        profile = np.where(
            (t >= t0) & (t < t1), ACCEL,
            np.where((t >= t2) & (t < t3), -ACCEL, 0.0),
        )
        chip_accel += profile[:, np.newaxis] * np.asarray(chip_motion_dir)

    # Gravity (constant DC offset in chip frame) -----------------------------------
    chip_accel += gravity_chip[np.newaxis, :]

    # 60 Hz structural ringing (independent phase per chip axis) -------------------
    phase = rng.uniform(0.0, 2.0 * np.pi, 3)
    chip_accel += RINGING_AMP * np.sin(2.0 * np.pi * RINGING_FREQ * t[:, np.newaxis] + phase)

    # Gaussian noise ---------------------------------------------------------------
    chip_accel += rng.normal(0.0, noise_std, (n, 3))

    # Optional Klipper-style axes_map remapping ------------------------------------
    if forward_remap is not None:
        chip_accel = chip_accel @ forward_remap.T  # (n,3) → (n,3)

    # Build sample matrix: columns are [t, x, y, z] --------------------------------
    t_abs = t_base + t
    samples = np.column_stack([t_abs, chip_accel])
    return np.round(samples, 6)


def _seg_dict(samples: np.ndarray) -> dict:
    """Split an (N,4) sample matrix into separate lists for JSON."""
    return {
        "t": samples[:, 0].tolist(),
        "x": samples[:, 1].tolist(),
        "y": samples[:, 2].tolist(),
        "z": samples[:, 3].tolist(),
    }


def _per_seg_dict(r: dict) -> dict:
    """Reshape one _process_single_axis result into the JSON schema shape."""
    pv = r["peak_velocities"]
    return {
        "direction_vector": r["direction_vector"].tolist(),
        "actual_direction": r["actual_direction"].tolist(),
        "angle_error": r["angle_error"],
        "confidence": r["confidence"],
        "noise_level": r["noise_level"],
        "peak_velocities": [pv["x"], pv["y"], pv["z"]],
        "gravity": r["gravity_magnitude"],
    }


# ---------------------------------------------------------------------------
# Case builder
# ---------------------------------------------------------------------------

_MEAS_NAMES = [
    "axesmap_X_20241201_120000",
    "axesmap_Y_20241201_120001",
    "axesmap_Z_20241201_120002",
]


def _build_case(AxesMapComputation, segments, accel: float, configured_map=None) -> dict:
    """Run AxesMapComputation on *segments*, collect per-segment and overall results."""
    measurements = []
    for name, seg in zip(_MEAS_NAMES, segments):
        measurements.append({"name": name, "samples": [tuple(row) for row in seg.tolist()]})

    comp = AxesMapComputation(
        measurements=measurements,
        accel=accel,
        st_version="TEST",
        current_axes_map=configured_map,
    )
    result = comp.compute()

    # Per-segment raw results (before any extrapolation; matches what compute() calls
    # internally before modifying direction_vectors for virtual axes).
    #
    # Calling-convention note (pinned commit 354ee4be): at this commit compute() feeds
    # each segment to _process_single_axis via _parse_measurements(), which does
    # `np.array(measurement['samples'])` – producing the same (N, 4) ndarray shape we
    # pass here directly.  If you update the pin, re-verify that _process_single_axis
    # still receives an (N, 4) ndarray before re-running this generator.
    raw_per_seg = [comp._process_single_axis(seg) for seg in segments]

    # Generation-time equivalence check: our direct per-segment calls must agree with
    # what compute() produced internally for every *measured* (non-extrapolated) axis.
    # For a virtual axis compute() replaces the direction_vector with a cross-product,
    # so that entry legitimately differs and is excluded from the comparison.
    for i, (raw, computed_dv) in enumerate(zip(raw_per_seg, result.direction_vectors)):
        if i == result.extrapolated_axis:
            # Virtual axis: compute() overwrites this entry with a cross-product.
            continue
        if not np.array_equal(raw["direction_vector"], computed_dv):
            raise RuntimeError(
                f"Axis {i}: direction_vector from direct _process_single_axis call "
                f"{raw['direction_vector']} != result.direction_vectors[{i}] {computed_dv}. "
                "The calling convention may have changed at the new pin; "
                "re-verify before regenerating the golden."
            )

    per_seg = [_per_seg_dict(r) for r in raw_per_seg]

    expected = {
        "per_segment": per_seg,
        "extrapolated_axis": result.extrapolated_axis,
        "rotation_matrix": result.rotation_matrix.tolist(),
        "euler_deg": list(result.euler_angles),
        "formatted": result.formatted_direction_vector,
        "quality": result.quality_status,
        "gravity": result.gravity,
        "noise_level": result.noise_level,
    }

    return {
        "accel": accel,
        "configured_map": configured_map,
        "segments": [_seg_dict(s) for s in segments],
        "expected": expected,
    }


# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--shaketune", required=True, type=Path, metavar="DIR",
                    help="Path to the Shake&Tune git checkout at the pinned commit")
    args = ap.parse_args()

    shaketune_dir = args.shaketune.expanduser().resolve()
    AxesMapComputation = load_shaketune(shaketune_dir)

    # Single seeded generator – consume in a fixed order for byte-identical output.
    rng = np.random.default_rng(42)

    cases: dict = {}

    # ------------------------------------------------------------------
    # Rotation matrix convention
    # R_chip_from_machine[:, i] = chip-frame direction when machine axis i moves
    # gravity_chip = R_chip_from_machine @ GRAVITY_MACHINE
    # ------------------------------------------------------------------

    # 1. toolhead_identity ------------------------------------------------
    R = np.eye(3)
    g = R @ GRAVITY_MACHINE
    segs = [_make_segment(rng, R[:, i], NOISE_STD_NORMAL, g) for i in range(3)]
    cases["toolhead_identity"] = _build_case(AxesMapComputation, segs, ACCEL)

    # 2. toolhead_x_negz_y ------------------------------------------------
    # Machine X → chip X, Machine Y → chip −Z, Machine Z → chip Y
    R = np.array([[1, 0, 0], [0, 0, 1], [0, -1, 0]], dtype=float)
    g = R @ GRAVITY_MACHINE
    segs = [_make_segment(rng, R[:, i], NOISE_STD_NORMAL, g) for i in range(3)]
    cases["toolhead_x_negz_y"] = _build_case(AxesMapComputation, segs, ACCEL)

    # 3. toolhead_tilt_5deg -----------------------------------------------
    # 5° rotation around machine Y → small angle errors, still detects "x, y, z"
    ang = np.radians(5.0)
    c, s = np.cos(ang), np.sin(ang)
    R = np.array([[c, 0, s], [0, 1, 0], [-s, 0, c]])
    g = R @ GRAVITY_MACHINE
    segs = [_make_segment(rng, R[:, i], NOISE_STD_NORMAL, g) for i in range(3)]
    cases["toolhead_tilt_5deg"] = _build_case(AxesMapComputation, segs, ACCEL)

    # 4. configured_map ---------------------------------------------------
    # Chip orientation: machine X → chip −Z, machine Y → chip Y, machine Z → chip X
    # Klipper is configured with axes_map = "−z, y, x", which is the CORRECT map for
    # this chip, so the detected formatted result should equal the configured_map.
    #
    # forward_matrix for axes_map "−z, y, x":
    #   i=0 (−z): forward[0, 2] = −1
    #   i=1 ( y): forward[1, 1] = +1
    #   i=2 ( x): forward[2, 0] = +1
    R = np.array([[0, 0, 1], [0, 1, 0], [-1, 0, 0]], dtype=float)
    g = R @ GRAVITY_MACHINE
    fwd = np.array([[0, 0, -1], [0, 1, 0], [1, 0, 0]], dtype=float)
    segs = [_make_segment(rng, R[:, i], NOISE_STD_NORMAL, g, forward_remap=fwd) for i in range(3)]
    cases["configured_map"] = _build_case(AxesMapComputation, segs, ACCEL, configured_map="-z, y, x")

    # 5. virtual_z --------------------------------------------------------
    # Z segment is noise-only (no motion); extrapolated from X×Y cross-product
    R = np.eye(3)
    g = R @ GRAVITY_MACHINE
    segs = [
        _make_segment(rng, R[:, 0], NOISE_STD_NORMAL, g),   # X: normal
        _make_segment(rng, R[:, 1], NOISE_STD_NORMAL, g),   # Y: normal
        _make_segment(rng, None,    NOISE_STD_NORMAL, g),   # Z: noise only
    ]
    cases["virtual_z"] = _build_case(AxesMapComputation, segs, ACCEL)

    # 6. virtual_x --------------------------------------------------------
    # X segment is noise-only; extrapolated from Y×Z cross-product
    segs = [
        _make_segment(rng, None,    NOISE_STD_NORMAL, g),   # X: noise only
        _make_segment(rng, R[:, 1], NOISE_STD_NORMAL, g),   # Y: normal
        _make_segment(rng, R[:, 2], NOISE_STD_NORMAL, g),   # Z: normal
    ]
    cases["virtual_x"] = _build_case(AxesMapComputation, segs, ACCEL)

    # 7. duplicate_axis ---------------------------------------------------
    # Both machine X and machine Y fire chip X → algorithm can't distinguish them
    # → formatted == "unable to determine correctly!"
    segs = [
        _make_segment(rng, np.array([1.0, 0.0, 0.0]), NOISE_STD_NORMAL, g),  # X → chip X
        _make_segment(rng, np.array([1.0, 0.0, 0.0]), NOISE_STD_NORMAL, g),  # Y → chip X (!)
        _make_segment(rng, np.array([0.0, 0.0, 1.0]), NOISE_STD_NORMAL, g),  # Z → chip Z
    ]
    cases["duplicate_axis"] = _build_case(AxesMapComputation, segs, ACCEL)

    # 8. mirrored ---------------------------------------------------------
    # Chip X axis is physically reversed; raw rotation_matrix has det = −1
    R = np.array([[-1, 0, 0], [0, 1, 0], [0, 0, 1]], dtype=float)
    g = R @ GRAVITY_MACHINE
    segs = [_make_segment(rng, R[:, i], NOISE_STD_NORMAL, g) for i in range(3)]
    cases["mirrored"] = _build_case(AxesMapComputation, segs, ACCEL)

    # 9. noisy_warning ----------------------------------------------------
    # Very high Gaussian noise (600 mm/s²) triggers the noise-level warning
    # (threshold: 0.3 × ACCEL = 450 mm/s²); direction is still detectable
    R = np.eye(3)
    g = R @ GRAVITY_MACHINE
    segs = [_make_segment(rng, R[:, i], NOISE_STD_NOISY, g) for i in range(3)]
    cases["noisy_warning"] = _build_case(AxesMapComputation, segs, ACCEL)

    # ------------------------------------------------------------------
    # Validate the invariant required by the brief
    # ------------------------------------------------------------------
    dup_fmt = cases["duplicate_axis"]["expected"]["formatted"]
    if dup_fmt != "unable to determine correctly!":
        sys.exit(
            f"FAIL: duplicate_axis.expected.formatted = {dup_fmt!r}; "
            "expected \"unable to determine correctly!\""
        )

    # ------------------------------------------------------------------
    # Serialise and compress
    # ------------------------------------------------------------------
    golden = {"pin": PIN, "cases": cases}
    text = json.dumps(golden, allow_nan=False, indent=1) + "\n"
    out = GOLDEN / "axes_map.json.zst"
    subprocess.run(
        ["zstd", "-q", "-19", "-T1", "-f", "-o", str(out)],
        input=text.encode(),
        check=True,
    )
    print(f"wrote {out}")
    print(f"cases ({len(cases)}): {', '.join(cases)}")
    print(f"duplicate_axis.expected.formatted = {dup_fmt!r}  ✓")


if __name__ == "__main__":
    main()
