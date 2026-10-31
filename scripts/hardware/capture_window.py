#!/usr/bin/env python3
"""Capture one whole command window with print_time markers, to validate time-window slicing.

Sequence (all through Moonraker's HTTP API; long commands are polled, so nginx's 60 s limit
does not matter):
  position -> start accelerometers -> positioning move to the test point -> M400 ->
  marker p1 (move end, print_time) -> TEST_RESONANCES (writes Klipper's own raw_data file too)
  -> M400 -> marker p2 -> stop accelerometers, copy the CSVs.

With --same-point the toolhead is already at the test point before the window opens, so the
positioning move inside the window is only the small drift left by the previous sweep (a repeat
command). Without it the toolhead starts away from the point (--from).

Writes <out>/<name>_<chip>.csv, <out>/<name>_klipper_<chip>.csv and <out>/<name>.markers.json,
where <chip> is the last word of the Klipper object name (``lis2dw ebb36`` -> ``ebb36``).
"""

import argparse
import json
import subprocess
import threading
import time
import urllib.parse
import urllib.request

DEFAULT_CHIPS = ["adxl345", "lis2dw ebb36"]


def call(host, path, data=None, timeout=90):
    url = f"http://{host}{path}"
    body = None if data is None else urllib.parse.urlencode(data).encode()
    with urllib.request.urlopen(
        urllib.request.Request(url, data=body), timeout=timeout
    ) as r:
        return json.load(r)


def gcode(host, script, timeout=90):
    return call(host, "/printer/gcode/script", {"script": script}, timeout)


def toolhead(host):
    q = call(host, "/printer/objects/query?toolhead&motion_report")["result"]
    return {"status": q["status"]["toolhead"], "eventtime": q["eventtime"]}


def wait_idle(host, settle_s=1.0, timeout_s=400):
    """Block until the toolhead queue has drained (estimated_print_time passed print_time)."""
    deadline = time.time() + timeout_s
    idle_since = None
    while time.time() < deadline:
        st = toolhead(host)["status"]
        if st["estimated_print_time"] > st["print_time"] + 0.05:
            idle_since = idle_since or time.time()
            if time.time() - idle_since >= settle_s:
                return
        else:
            idle_since = None
        time.sleep(0.5)
    raise TimeoutError("toolhead did not become idle")


def run_long(host, script):
    """Send a command that may outlast nginx's 60 s limit; completion is detected by polling."""

    def send():
        try:
            gcode(host, script, timeout=600)
        except OSError as exc:  # a 504 from nginx still means the command was queued
            print("note: long command returned early:", exc)

    threading.Thread(target=send, daemon=True).start()
    time.sleep(3.0)  # let it start before polling
    wait_idle(host)


def chip_names(klipper_object):
    """(type, name) of a Klipper accelerometer object: ``lis2dw ebb36`` -> (lis2dw, ebb36)."""
    parts = klipper_object.split()
    return parts[0], parts[-1]


def measure_file(klipper_object, name):
    """Path ACCELEROMETER_MEASURE writes (Klipper omits the type when it equals the name)."""
    kind, chip = chip_names(klipper_object)
    return (
        f"/tmp/{kind}-{name}.csv" if kind == chip else f"/tmp/{kind}-{chip}-{name}.csv"
    )


def main():
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("--host", required=True)
    ap.add_argument("--user", default="pi", help="SSH user on the printer host")
    ap.add_argument(
        "--chip",
        action="append",
        dest="chips",
        help="Klipper accelerometer object name, repeatable "
        f"(default: {', '.join(repr(c) for c in DEFAULT_CHIPS)})",
    )
    ap.add_argument("--out", required=True)
    ap.add_argument("--name", required=True)
    ap.add_argument("--axis", default="X")
    ap.add_argument("--from", dest="start", default="110,110,10", help="X,Y,Z start")
    ap.add_argument(
        "--to", dest="end", default="150.5,154,50", help="the test_resonances point"
    )
    ap.add_argument("--freq", default="20,40", help="FREQ_START,FREQ_END")
    ap.add_argument("--hz-per-sec", default="2")
    ap.add_argument(
        "--same-point",
        action="store_true",
        help="start at the test point (repeat command)",
    )
    args = ap.parse_args()
    chips = args.chips or DEFAULT_CHIPS
    tx, ty, tz = args.end.split(",")
    fx, fy, fz = (tx, ty, tz) if args.same_point else args.start.split(",")
    f0, f1 = args.freq.split(",")
    markers = {"name": args.name, "axis": args.axis, "same_point": args.same_point}

    gcode(args.host, f"G90\nG1 X{fx} Y{fy} Z{fz} F6000\nM400")  # outside the window
    time.sleep(2.0)  # let the toolhead ring down before the window opens
    markers["p0"] = toolhead(args.host)
    pos0 = markers["p0"]["status"]["position"][:3]
    target = [float(tx), float(ty), float(tz)]
    at_point = max(abs(a - b) for a, b in zip(pos0, target)) < 0.01
    if args.same_point and not at_point:
        raise SystemExit(f"--same-point, but the toolhead is at {pos0}, not {target}")
    if not args.same_point and at_point:
        raise SystemExit("the toolhead already is at the test point; pass --same-point")

    for chip in chips:
        gcode(args.host, f"ACCELEROMETER_MEASURE CHIP={chip_names(chip)[1]}")
    gcode(args.host, f"G1 X{tx} Y{ty} Z{tz} F6000\nM400")
    markers["p1_move_end"] = toolhead(args.host)
    run_long(
        args.host,
        f"TEST_RESONANCES AXIS={args.axis} FREQ_START={f0} FREQ_END={f1} "
        f"HZ_PER_SEC={args.hz_per_sec} SWEEPING_PERIOD=0 OUTPUT=raw_data "
        f'NAME={args.name} CHIPS="{",".join(chips)}"\nM400',
    )
    markers["p2_sweep_end"] = toolhead(args.host)
    for chip in chips:
        gcode(
            args.host,
            f"ACCELEROMETER_MEASURE CHIP={chip_names(chip)[1]} NAME={args.name}",
        )
    with open(f"{args.out}/{args.name}.markers.json", "w") as fh:
        json.dump(markers, fh, indent=1)

    host = f"{args.user}@{args.host}"
    axis = args.axis.lower()
    for chip in chips:
        name = chip_names(chip)[1]
        files = {
            f"/tmp/raw_data_{axis}_{name}_{args.name}.csv": f"{args.name}_klipper_{name}.csv",
            measure_file(chip, args.name): f"{args.name}_{name}.csv",
        }
        for src, dst in files.items():
            subprocess.run(
                ["scp", "-q", f"{host}:{src}", f"{args.out}/{dst}"], check=True
            )
    print("done", args.name)


if __name__ == "__main__":
    main()
