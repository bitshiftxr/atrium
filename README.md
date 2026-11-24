# Atrium

Control your Home Assistant devices from the Omarchy bar, grouped by area.
Lights, thermostats, media players, covers, fans and locks, each with the
controls it supports.

> Not affiliated with or endorsed by the Home Assistant project.

![The Atrium panel on the bar, and its settings window](preview.png)

## Features

- Switch rooms from a dropdown, and pin the rooms you use most
- Turn every light, switch and fan in a room on or off at once
- Room order and hidden rooms are taken from your Home Assistant dashboard the first time you connect
- Shows the same devices your Home Assistant dashboard shows
- Pin individual devices to a tab of their own
- Works from the keyboard, and from commands you can put on a keybind

## Requirements

- Omarchy 4.0 or newer
- `secret-tool` (libsecret) with a running keyring

## Install

```sh
omarchy plugin add https://github.com/bitshiftxr/atrium.git --enable
```

That is the whole install. The plugin's QML talks to a small daemon, `atriumd`,
which is not committed to the repository. The first time the panel starts,
`bin/atriumd-launcher` downloads the prebuilt `atriumd` for your machine
(x86_64 or aarch64) from this repository's GitHub releases. It checks the
download's size and SHA256 against [`daemon-release.env`](daemon-release.env),
which is committed alongside the plugin, and refuses to run anything that does
not match. The verified binary is kept in
`${XDG_DATA_HOME:-~/.local/share}/atrium/<release>/`, outside the plugin folder,
and is checked against the pin again on every start, so later starts work
offline. A cached binary that no longer matches is downloaded again.

If the download fails or does not match the pin, nothing is run. The panel
shows an error banner beginning `atriumd could not start:` with the reason, for
example `the atriumd v0.1.0 download does not match its pinned SHA256; refusing
to run it`, or `could not download atriumd v0.1.0: ...` when you are offline.
The same text appears on the Connection pane in settings. Atrium retries on its
own, waiting longer between attempts up to five minutes, and the banner clears
once the daemon starts.

`omarchy plugin update` brings in a new `daemon-release.env` along with the
rest of the plugin, and the next start downloads the release it names. Older
releases' folders under `~/.local/share/atrium/` can be deleted.

## Bar position

```sh
omarchy bar move io.github.bitshiftxr.atrium --section left
```

## Connecting to Home Assistant

Open the panel and press `s`, then enter your Home Assistant address and an
access token. Tokens live in Home Assistant under your profile → Security →
Long-lived access tokens.

The settings window has five panes:

- **Connection** takes the address and the token, and shows the connection state.
- **Areas** lists every room, how many devices are in it, and lets you show,
  hide, reorder and pin each one.
- **Entities** pins individual devices to a tab in front of your rooms.
- **Keyboard** lists the shortcuts and commands.
- **Activity** is a log of the last 200 actions and problems, filterable down to
  problems only.

## What you can control

| Device | Controls |
| --- | --- |
| Lights | On/off, brightness, warmth, and colour |
| Switches, fans, humidifiers, sirens, remotes, toggle helpers | On/off, and fan speed |
| Locks | Lock and unlock |
| Scenes, scripts, buttons | Activate |
| Media players | Previous, play/pause, next, volume, mute |
| Covers | Open, stop, close, and position |
| Thermostats | Mode, target temperature or range, fan and preset |
| Sensors and everything else | Current state |

The switch beside the room name turns the room's lights, switches, fans,
humidifiers and toggle helpers on or off together. It shows on only when all of
them are on, and switching it off then turns them all off. Otherwise, including
when some are on and some off, it shows off, and switching it on turns them all
on. It follows the devices as they change. It acts only on the devices
the panel shows for that room, skips anything unavailable, and never touches
locks, covers, alarm panels, thermostats or media players. Rooms with nothing it
could switch have no switch.

Controls come from what each device reports. A light that cannot change colour
has no colour swatch; a cover that cannot report its position has no slider.

## Keyboard

With the panel open: `j`/`k` or `↑`/`↓` move between devices, `h`/`l` or
`←`/`→` change room, `enter` or `space` switches a device on or off, `a`
switches the whole room on or off, `e` opens its controls, `r` refreshes, `s`
opens settings, `tab` moves to the next bar panel, and `esc` closes.

## Commands

```sh
omarchy-shell atrium toggle                      # open or close the panel
omarchy-shell atrium open
omarchy-shell atrium close
omarchy-shell atrium refresh                     # re-read from Home Assistant
omarchy-shell atrium toggleEntity light.desk
omarchy-shell atrium activate scene.movie_night
omarchy-shell atrium toggleArea Kitchen          # the room switch, by name
omarchy-shell atrium areas                       # room names, one per line
omarchy-shell atrium rooms                       # settings, on the Areas pane
omarchy-shell atrium settings                    # settings, on the Connection pane
omarchy-shell atrium status                      # state and counts, as JSON
```

`toggleEntity` and `activate` refuse locks, covers and alarm panels; see
[Security](#security).

## Settings file

Your settings are saved to `~/.config/atrium/config.json`: the Home Assistant
address, your room preferences, what you have pinned, and the
`allowSensitiveIpc` switch described below. `ATRIUM_CONFIG` overrides the path.

## Security

Your access token is stored in your system keyring.

Certificates are checked against the system trust store. The connection is a
direct WebSocket to Home Assistant, so proxy variables in your environment have
no effect. An address with no `https://` is treated as secure; a plain `http://`
address works but warns you first, because it sends your token unencrypted.

Locks, covers and alarm panels can only be operated from the panel. Every
process running as your user can reach the shell's IPC socket, so
`omarchy-shell atrium toggleEntity lock.front_door` is refused until you set
`"allowSensitiveIpc": true` in `~/.config/atrium/config.json`. Everything else
is actionable from a command.

Atrium can only perform the actions listed above, from the panel or from a
command. It reads only the device details those actions need, which does not
include camera tokens, media links or location.

## Development

The daemon is Rust, in `daemon/`. Building it needs Rust (the `rust` package
on Omarchy):

```sh
cd daemon
cargo test
cargo clippy --all-targets
```

To run the panel against a daemon you built, build it outside the plugin
folder, since the shell reloads on every file event inside it:

```sh
cargo build --release --locked --manifest-path daemon/Cargo.toml \
  --target-dir ~/.cache/atrium/target
```

Then start the shell with
`ATRIUM_DAEMON=~/.cache/atrium/target/release/atriumd` in its environment. The
panel runs that binary directly and skips the launcher.

To check the launcher on its own, `bin/atriumd-launcher --fetch` downloads and
verifies the pinned daemon if needed and prints its path.

```sh
omarchy plugin validate .
qmllint -I "$OMARCHY_PATH/shell" -I . Bridge.qml DaemonFailureTest.qml \
  EntityRow.qml Glyph.qml Service.qml Settings.qml controls/*.qml settings/*.qml
```

`Panel.qml` is left out of that list because qmllint 1.0 exits 255 on it with no
diagnostics.

```sh
./test-daemon-failure
```

Runs `Service.qml` under quickshell against five daemons — one missing, one that
exits on startup, the launcher refusing a download whose hash does not match
its pin, the real one through the launcher, and one that appears twelve seconds
in — and checks that the panel reports the failure and its cause, keeps
reporting it, and clears once the daemon works. It needs a Wayland session and
the network, and takes about two minutes, so it is not part of `cargo test`.

To see what the panel would draw without starting the shell:

```sh
ATRIUM_TOKEN='<your token>' bin/atriumd-launcher probe https://homeassistant.local:8123 --rows
```

`atriumd call <url> <entity> <action> [json]` resolves one action the way the
panel would and sends it. Both read the token from `ATRIUM_TOKEN` or stdin,
never from an argument, where it would be visible to every process on the
machine.

Two tests can run against a real Home Assistant. They are skipped unless
`ATRIUM_FIXTURE` points at a JSON file holding the areas, floors, devices,
entities, states and dashboard config of an instance. Keep that file out of the
repository; it lists every room and device in a home. The rest of the suite
needs no instance and no network.

## Cutting a release

The plugin's version and the daemon it runs are tied together only by
`daemon-release.env`. It always names a release whose assets are already
published, and CI downloads and verifies them on every pull request and push to
`main`. A new daemon reaches users in three steps:

1. Bump `"version"` in `manifest.json` (and `daemon/Cargo.toml` to match) and
   merge that to `main`.
2. Tag the merge commit with the same version and push the tag:

   ```sh
   git tag v0.2.0
   git push origin v0.2.0
   ```

   The `Release` workflow refuses a tag that does not match `manifest.json`. It
   builds `atriumd` for `x86_64-unknown-linux-gnu` and
   `aarch64-unknown-linux-gnu`, and attaches both, with `SHA256SUMS`, to the
   GitHub release for the tag.
3. The same workflow then downloads the published assets, writes their sizes
   and SHA256s into `daemon-release.env`, checks the result with the launcher,
   and opens a pull request named "Pin atriumd v0.2.0". Merge it. Until then,
   `main` keeps running the previous release's daemon.

Ask the marketplace to pick up the new version only after step 3 has merged.
If the panel and the daemon change together in a way the old daemon cannot
serve, keep the panel change on its branch until the pin PR is ready, so `main`
never pairs new QML with an old daemon.

The pin PR is opened with the workflow's own token, which needs "Allow GitHub
Actions to create and approve pull requests" turned on in the repository's
Actions settings. GitHub does not run CI on a pull request opened that way;
close and reopen it to run CI.

## Remove

```sh
omarchy plugin remove io.github.bitshiftxr.atrium
```

## License

MIT — see [`LICENSE`](LICENSE).
