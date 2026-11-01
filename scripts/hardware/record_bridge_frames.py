#!/usr/bin/env python3
"""Record Moonraker /klippysocket bridge frames for the daemon's test fixtures.

Read-only: it subscribes to each chip's dump endpoint, keeps the subscribe reply and the first
batches, and provokes the three subscribe errors. Nothing moves. Needs `pip install websockets`.

Usage: record_bridge_frames.py HOST[:PORT] OUT.json
"""
import asyncio
import json
import sys
import urllib.request

import websockets

CHIPS = {
    "adxl345": ("adxl345/dump_adxl345", "adxl345"),
    "lis2dw": ("lis2dw/dump_lis2dw", "ebb36"),
}
ROWS = 4  # rows kept per recorded batch


def token(host):
    with urllib.request.urlopen(f"http://{host}/access/oneshot_token", timeout=10) as r:
        return json.load(r)["result"]


async def session(host, method, params, batches):
    url = f"ws://{host}/klippysocket?token={token(host)}"
    async with websockets.connect(url) as ws:
        await ws.send(json.dumps({"id": 1, "method": method, "params": params}))
        reply = json.loads(await asyncio.wait_for(ws.recv(), 5))
        frames = []
        if "error" not in reply:
            while len(frames) < batches:
                frames.append(json.loads(await asyncio.wait_for(ws.recv(), 5)))
        return reply, frames


def trim(frame):
    data = frame["params"]["data"]
    return {"params": {**frame["params"], "data": data[:ROWS], "_rows_in_batch": len(data)}}


async def main(host, out):
    result = {"_note": f"Recorded from {host} by scripts/hardware/record_bridge_frames.py; "
                       f"each batch trimmed to {ROWS} rows (_rows_in_batch has the real count)."}
    for name, (method, sensor) in CHIPS.items():
        params = {"sensor": sensor, "response_template": {}}
        reply, frames = await session(host, method, params, 3)
        result[f"{name}_subscribe_reply"] = reply
        result[f"{name}_batches"] = [trim(f) for f in frames]
    errors = {
        "missing_sensor": ("adxl345/dump_adxl345", {"response_template": {}}),
        "bad_sensor": ("adxl345/dump_adxl345", {"sensor": "nope", "response_template": {}}),
        "unregistered_endpoint": ("adxl345/dump_nothing", {"sensor": "adxl345", "response_template": {}}),
    }
    for name, (method, params) in errors.items():
        result[name], _ = await session(host, method, params, 0)
    json.dump(result, open(out, "w"), indent=2)
    print(f"wrote {out}")


asyncio.run(main(sys.argv[1], sys.argv[2]))
