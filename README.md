# rk-a72

Tools for talking to the wired Royal Kludge A72 keyboard directly over raw HID —
reading/writing the keymap, LED colors, macros, and other on-device settings without the
vendor's browser-based software.

> [!WARNING]
> **Use at your own risk.** This software talks to your keyboard's firmware over an
> undocumented, reverse-engineered protocol. It is not affiliated with, endorsed by, or
> supported by Royal Kludge. It may misconfigure or brick your device, and using it may
> void your warranty. It is provided "as is", without warranty of any kind — the author
> accepts no responsibility or liability for any damage, data loss, or malfunction
> resulting from its use. See [LICENSE](LICENSE).

**Supported device: the wired RK A72 (`258a:0216`) only.** Every byte layout here was
worked out and verified against that one keyboard. Other RK models speak the same protocol
family and might work, but none have been tested, so the tools refuse to talk to anything
else rather than writing guessed data to your keyboard — `--vid`/`--pid` exist but only
accept the A72's own IDs. If you have another model and want to help widen that, open an
issue.

The A72 uses USB vendor ID `0x258a` and is believed to be built on a SinoWealth
SH68F90A-family chip (marketed as BYK916) — see [Protocol background](#protocol-background)
below.

## Layout

A Cargo workspace with two crates:

- [`rk-a72-keymap`](rk-a72-keymap/) — protocol codec and device session library.
- [`rk-a72-cli`](rk-a72-cli/) — the `rk-a72` command-line tool built on it.

See each crate's own README for build/usage instructions.

## Quick start

Prebuilt `rk-a72` binaries for Windows, Linux, and macOS (Apple Silicon) are published
on the [Releases](../../releases) page for every tagged version. Or build from source:

```sh
cargo build --release
cargo run -p rk-a72 -- list-keys   # sanity check, no device needed
```

See [`rk-a72-cli/README.md`](rk-a72-cli/README.md) for full usage (exporting and
importing keymaps, per-key get/set, udev permissions on Linux, `sudo` requirement on
macOS, etc).

## Protocol background

RK's official configurator communicates with the keyboard using one of several protocol
families RK uses across its lineup (others include SparkLink, JuPeng, QiWang, Gcome,
HangSheng, RongYuan). Only the A72's own protocol family is implemented here, and only as
the A72 speaks it.

The USB vendor ID (`0x258a`) used by the A72 is registered to
**Sino Wealth Electronic Ltd.** — confirmed both by that registration and by the A72's own
USB string descriptors (`iManufacturer: "SINOWEALTH"`). The underlying chip is commonly
rebadged as **BYK916** (SinoWealth SH68F90A) by board vendors.

## See also

Other independent reverse-engineering efforts targeting the same `0x258a` (RK/SinoWealth)
device family. None of them list the A72's PIDs (`0216`/`0233`) or match this repo's report
IDs (9/19) byte-for-byte — the protocol appears to have evolved since these were written —
but they're useful prior art and cross-reference material:

- [rnayabed/rangoli](https://github.com/rnayabed/rangoli) — Qt GUI configurator; the most
  complete keyboard/PID compatibility list (~90 models) for the `0x258a` protocol family.
- [airblast-dev/kludged](https://github.com/airblast-dev/kludged) — Rust CLI/library, RK68
  (PID `005e`) RGB/animation control.
- [oddlyspaced/rkcu](https://github.com/oddlyspaced/rkcu) — Python CLI for the RK61 series.
- [ecornell/rk-kb-macro-editor](https://github.com/ecornell/rk-kb-macro-editor) — 519-byte
  Feature Report macro editor, reverse-engineered from the official web configurator.
- [vinc3m1/kludgeknight](https://github.com/vinc3m1/kludgeknight) — WebHID port of the
  Rangoli protocol, runs in-browser.
- [carlossless/sinowealth-kb-tool](https://github.com/carlossless/sinowealth-kb-tool) — ISP
  flasher for the underlying SinoWealth SH68F90A/BYK916 chip (firmware-level, not the
  runtime HID command protocol these tools and this repo speak).

## License

[MIT](LICENSE) © Mark Harnyk

This project is an independent, unofficial effort. "Royal Kludge" and "RK" are trademarks
of their respective owners; this project is not affiliated with or endorsed by them.
