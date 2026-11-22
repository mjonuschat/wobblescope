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

This removes the service but keeps `wobblescope.toml` and the recorded sessions. To finish, delete `~/wobblescope` and the `[update_manager wobblescope]` section in `moonraker.conf`, then restart Moonraker.

## Running a test

Open `http://<printer>:7140/` in a browser. Each test starts from the settings in your Klipper config, and you can override them per test. Because a test moves the toolhead, home the printer and clear its path first. The emergency stop halts a running test at any time.

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
| `allowed_hosts` | none | Extra host names to accept, used only when `check_host_header` is on. |

## Improvements and bugfixes

Wobblescope follows [Shake&Tune](https://github.com/Frix-x/klippain-shaketune), so results for the same data are close. These are the places where it does something different, compared with Shake&Tune as of January 2026.

### Improvements

- The belt comparison ignores noise outside the tested frequencies.
- Belt peaks are labelled from the lowest frequency up.
- The axes map works with an accelerometer on the bed.
- The axes map now works out Z from the other two axes instead of failing.

### Bugfixes

- Belt peaks at the same frequency show their real amplitude difference, not 0 %.
- Belt peaks at the ends of the chart are no longer missed or marked twice.
- A vibrations profile that finds no quiet speed range finishes normally.

## Credits

- [Klippain Shake&Tune](https://github.com/Frix-x/klippain-shaketune) is the reference for the analysis, including belt comparison and the vibrations profile. Results for the same data are close to Shake&Tune's but not identical.
- [Klipper](https://github.com/Klipper3d/klipper) is the reference for the input shaper fitting.

## License

GPL-3.0-or-later. See `LICENSE`.
