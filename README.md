# Wobblescope

Multi-accelerometer resonance testing for Klipper and Kalico printers, with the analysis done in your browser.

## What it does

- **Input shaper calibration.** Runs a resonance test on several accelerometers at once and fits Klipper's shapers to each chip. It recommends one shaper per chip and one for all chips, and prints the `[input_shaper]` lines for `printer.cfg`.
- **Belt comparison.** Compares the two belts of a CoreXY or limited CoreXY printer, chip by chip.
- **Vibrations profile.** Records the toolhead at a range of speeds and angles and shows which speeds to avoid.
- **Axes map.** Finds which accelerometer axis follows which printer axis.
- **Live view.** Streams an accelerometer into a live spectrum and holds the excitation at one frequency.
- **History.** Keeps each session's raw samples for later re-analysis, saves results as files that open on any device, and imports Klipper's raw CSV files.

## Requirements

- A Klipper or Kalico printer with Moonraker and at least one ADXL345, LIS2DW or Beacon accelerometer.

## Installation

On the machine that runs Moonraker, run this as your normal user, not root:

```sh
curl -fsSL https://raw.githubusercontent.com/mjonuschat/wobblescope/main/install.sh | bash
```

The installer is safe to re-run and refuses to run during a print. Later releases appear in the update panel of Fluidd and Mainsail.

To install a specific release, pass `--version`:

```sh
curl -fsSL https://raw.githubusercontent.com/mjonuschat/wobblescope/main/install.sh | bash -s -- --version v0.2.0
```

### Opening it from Mainsail or Fluidd

If nginx serves your Mainsail or Fluidd, `setup` also publishes Wobblescope on the same address: `http://<printer>/wobblescope/` (add the port if your site uses one, for example `:81`). It changes:

- `/etc/nginx/snippets/wobblescope.conf`, a file `setup` owns.
- One line in each Mainsail or Fluidd site file that proxies your Moonraker, `include /etc/nginx/snippets/wobblescope.conf;` between `# BEGIN wobblescope` and `# END wobblescope` comments. A copy of the file as it was is kept as `<file>.before-wobblescope`.
- A "Wobblescope" button in Mainsail's sidebar, in `.theme/navi.json` next to `moonraker.conf`. An existing `navi.json` keeps its other entries. If it is not a JSON array, `setup` leaves it alone and says so.
- `trusted_proxies` in `wobblescope.toml`, which gets `127.0.0.1` and `::1` so the daemon sees your real clients behind nginx.

`setup` runs `nginx -t` first, puts everything back if nginx rejects the change, and then reloads nginx. It handles one Moonraker instance, the one in `wobblescope.toml`; sites that proxy another instance are left alone. Add `--no-nginx` to skip all of this. `setup --check` reports each of these pieces. `setup --remove` takes the include lines, the snippet and the sidebar entry out and leaves `wobblescope.toml` as it is.

### Repairing and checking

```sh
sudo ~/wobblescope/wobblescope setup
~/wobblescope/wobblescope setup --check
```

If Moonraker rejects setup as unauthorized, add `--api-key <key>`, or `--api-key -` to read the key from standard input.

### Removing

```sh
sudo ~/wobblescope/wobblescope setup --remove
```

This removes the service and the nginx link but keeps `wobblescope.toml` and the recorded sessions. To finish, delete `~/wobblescope` and the `[update_manager wobblescope]` section in `moonraker.conf`, then restart Moonraker.

## Running a test

Open `http://<printer>:7140/` in a browser, or `http://<printer>/wobblescope/` when `setup` found nginx. Each test starts from the settings in your Klipper config, and you can override them per test. Because a test moves the toolhead, home the printer and clear its path first. The emergency stop halts a running test at any time.

## Access

Moonraker's access rules apply to every request. Your browser's address must be in `trusted_clients`, and `cors_domains` and the login requirement are enforced as well.

The daemon connects to Moonraker as a client, so Moonraker must trust it too. If the daemon's address is missing from `trusted_clients`, set `moonraker_api_key`.

## Configuration

Settings live in `wobblescope.toml`, next to `moonraker.conf`. Edit the file, then restart the service:

```sh
sudo systemctl restart wobblescope
```

Every option is optional. The daemon refuses to start on an unknown key or a bad value and logs the reason, which `journalctl -u wobblescope` shows. `moonraker_api_key` sits in the file in plain text and travels unencrypted over `http`, so keep the file at mode `0600`.

For example, to keep up to 4 GiB of sessions and check the `Host` header:

```toml
retention_cap_mib = 4096
check_host_header = true
```

### Options

| Option | Default | Meaning |
|---|---|---|
| `listen` | `0.0.0.0:7140` | Address and port to listen on. |
| `moonraker_url` | `http://127.0.0.1:7125` | Where Moonraker is. For `https`, set `moonraker_ca_file` or `moonraker_insecure_skip_verify`. |
| `moonraker_api_key` | none | API key for the daemon's connection to Moonraker. |
| `storage_dir` | `wobblescope` next to Moonraker's `config` directory | Where sessions and results are stored. |
| `retention_cap_mib` | `2048` | Retention deletes sessions once the kept total exceeds this size. Pinned sessions stay. |
| `free_space_headroom_mib` | `256` | Free space that must remain after a test's estimated size. |
| `trusted_proxies` | none | Proxies whose `Forwarded`, `X-Forwarded-For` and `X-Real-IP` headers the daemon believes. |
| `check_host_header` | off | Refuses requests whose `Host` is not a name the daemon knows. |
| `level_command` | none | G-code command the Z-TILT or QGL button on the Printer page sends instead of `Z_TILT_ADJUST` or `QUAD_GANTRY_LEVEL`. |
| `allowed_hosts` | none | Extra host names to accept, used only when `check_host_header` is on. |

## Coming from Shake&Tune

Wobblescope builds on [Shake&Tune](https://github.com/Frix-x/klippain-shaketune), so results for the same data are close and the graphs look familiar. A few things differ on purpose, for example how the two belts are compared and how a recording without sweep settings is read. [How Wobblescope differs from Shake&Tune](docs/differences-from-shaketune.md) answers the common questions.

## Credits

- [Klippain Shake&Tune](https://github.com/Frix-x/klippain-shaketune) is where the analysis started, including belt comparison and the vibrations profile. Wobblescope tests its results against it and differs on purpose in a few places.
- [Klipper](https://github.com/Klipper3d/klipper) is the reference for the input shaper fitting.

## License

GPL-3.0-or-later. See `LICENSE`.
