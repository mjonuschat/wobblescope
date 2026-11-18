# Wobblescope

Multi-accelerometer resonance testing for Klipper and Kalico printers, with the analysis done in your browser.

Belt comparison needs Klipper v0.12.0 or newer, or Kalico v2026.01.00 or newer.

## Installation

Run this on the machine that runs Moonraker, as the normal user (not root):

```sh
curl -fsSL https://raw.githubusercontent.com/mjonuschat/wobblescope/main/install.sh | bash
```

The installer puts `wobblescope` in `~/wobblescope`, runs `sudo ~/wobblescope/wobblescope setup` to install the service, and adds `wobblescope` to the update panel in Fluidd and Mainsail, so new releases show up there. It refuses to run during a print and is safe to re-run. It checks the download against the release's `SHA256SUMS`.

- `--version v0.2.0` installs a specific release.

`setup` installs and enables the `wobblescope` service, creates `wobblescope.toml` next to `moonraker.conf` when there is none, and lists the service in `moonraker.asvc`. Run it by hand to repair or verify an installation:

```sh
sudo ~/wobblescope/wobblescope setup            # install or repair the service
~/wobblescope/wobblescope setup --check         # report each piece and change nothing
sudo ~/wobblescope/wobblescope setup --remove   # stop and remove the service and its service-list entry
```

If Moonraker refuses setup as unauthorized, add `--api-key <key>` (`--api-key -` reads the key from standard input). `--remove` keeps the binary, the `[update_manager wobblescope]` section, `wobblescope.toml` and the recorded sessions. Restart Moonraker after `setup` or `--remove` for the service menu to change.
