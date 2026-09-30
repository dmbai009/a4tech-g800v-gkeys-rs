# A4Tech G800V G-keys (Rust)

Makes the G-keys of an **A4Tech X7 G800V** keyboard work as standalone keys on Windows,
so they can be bound independently in games, OBS, Discord, AutoHotkey, etc.

A Rust rewrite of [a4tech-g800v-gkeys](https://github.com/dmbai009/a4tech-g800v-gkeys):
the same behaviour in a ~300 KB exe that runs as one process with about 1 MB of memory
(the Python build was 8 MB and two processes with ~24 MB).

Out of the box the G-keys only work by being bound (in the A4Tech app) to other keys or
combinations. However, the keyboard also reports every G-key press on a hidden
vendor-defined HID interface (usage page `0xFFA0`) as a bitmask. `gkeys.exe` listens to
that interface and emulates keys that don't exist on a regular keyboard.

## Default mapping

| G-key   | Emulated key |
|---------|--------------|
| G1–G7   | F13–F19      |
| G9–G13  | F20–F24      |
| G14     | Kana         |
| G15     | Convert      |
| G16     | NonConvert   |

There is no physical G8 key (its bit exists in the report but is never set).
Kana / Convert / NonConvert are Japanese keyboard keys: they do nothing on
Russian/English layouts but are seen by apps and games as distinct keys.

To change it, edit `mapping()` in `src/keys.rs` (combinations work too, e.g. `&[CTRL, F13]`)
and rebuild.

## Install

`gkeys.exe` needs no runtime and no admin rights.

1. Remove any bindings from the G-keys in the A4Tech app.
2. Double-click `gkeys.exe`. It copies itself to
   `%LOCALAPPDATA%\Programs\A4Tech G-keys\`, adds itself to autostart
   (`HKCU\...\Run`), registers in **Settings → Apps** and starts in the background.

To uninstall, either uninstall **A4Tech G-keys** in Settings → Apps, or run
`gkeys.exe` again and choose **Yes**. Choosing **No** there reinstalls, e.g. to
update to a newer exe. Uninstalling stops the background process and removes the
autostart entry, the Apps entry and the installed files.

This exe uses the same install location, registry entries and stop signal as the Python
version, so installing it over the Python build replaces it cleanly.

Command line: `gkeys.exe --install | --uninstall | --run [--quiet] [--console]`
(`--quiet` suppresses dialogs, `--console` prints every G-key press when run from a terminal).

Windows SmartScreen may warn about an unsigned exe: click **More info → Run anyway**.

## Build

Requires [Rust](https://rustup.rs) with the MSVC toolchain (Visual Studio Build Tools).

```
cargo test
cargo build --release
```

The result is `target\release\gkeys.exe`.

## Notes

- Games running as administrator only receive the keys if `gkeys.exe` runs elevated too.
- Games with kernel anti-cheat may ignore injected (software-generated) input.
- If the keyboard is unplugged while a G-key is held, all held keys are released and the
  program reconnects on its own when the keyboard is back.

Report format on page `0xFFA0`: `04 xx xx b3 b4 00 00 00 80`, where `b3` bits 0–7 are
G1–G8 and `b4` bits 0–7 are G9–G16. Bytes 1–2 change when bindings are edited in the
A4Tech app.

## License

[MIT](LICENSE)
