# rk-a72-cli

`rk-a72` — command-line tool for reading and writing the keymap on a wired RK A72
keyboard, built on the [`rk-a72-keymap`](../rk-a72-keymap/) protocol library.

Supports the wired RK A72 (`258a:0216`) only — see the [root README](../README.md).

## Build

```sh
cargo build --release -p rk-a72
# binary at ../target/release/rk-a72
```

## Usage

```sh
rk-a72 export-hcl keymap.hcl               # keys that differ from factory default, as an HCL `layer` block
rk-a72 export-hcl --full keymap.hcl        # every populated slot, not just the ones that differ
rk-a72 import-hcl keymap.hcl               # merge an HCL config file (issue #2 schema) onto factory default and apply

rk-a72 list-keys                           # reference: physical key names, KeyBoard symbols, modifiers

rk-a72 get-keymap Esc                      # print the current mapping of one physical key, as an HCL `mapping` block
rk-a72 get-keymap Esc --layer fn

rk-a72 set-keymap Esc --symbol Grave       # set one key directly, without a config-file round-trip
rk-a72 set-keymap A --symbol B --mod LCtrl+LShift
rk-a72 set-keymap A --label Mute
rk-a72 set-keymap A --raw 0x020000e2
```

All device-targeting commands accept `--vid`/`--pid`, but only the wired A72's own IDs
(`258a`/`0216`, also the defaults) are accepted — any other value is rejected with an
error rather than attempting to write untested byte layouts to an unverified device.

### HCL config (`export-hcl` / `import-hcl`)

`import-hcl` accepts the HCL schema from issue #2 (`theme` / `macro` / `layer` / `lighting`).
The whole document is parsed and fully validated up front — bad CSS colours, unknown key
names, malformed macros, out-of-range brightness, etc. all fail before any hardware is
touched. See [`keymap.example.hcl`](keymap.example.hcl) for a worked example.

What actually gets **flashed** today is only the `layer` section, and within it only
`key`(+`mods`) / `label` / `raw` actions — these map straight onto the `SetKeyMatrix`
write path. Only the `normal` and `fn` layers exist on the A72.

Each key is its own repeatable `mapping "<PhysicalKey>" { ... }` block, not a single object
attribute — this is the v2 schema (issue #2), no backward compatibility with the earlier
`mappings = { "<PhysicalKey>" = {...} }` form:

```hcl
layer "fn" {
  mapping "M1" {
    mods = ["LCtrl"]
    key  = "C"
  }
  mapping "Esc" {
    label = "Mute"
  }
}
```

Two HCL grammar rules this relies on, worth knowing before hand-editing: a nested block's
header (`mapping "X" {`) must start on its own line — `layer "x" { mapping "y" { ... } }`
crammed onto one line is a parse error — and attributes inside one block are newline-
separated, never comma-separated. A duplicate `mapping` label within the same `layer` is a
hard error, not a silent overwrite.

`export-hcl` is the inverse: it reads the device's KeyMatrix and writes it back out as HCL
`layer` blocks in that same `key`/`label`/`raw` shape — round-trippable through `import-hcl`
with no manual editing needed. It only ever emits `layer` blocks: there's no `theme`, `macro`,
or `lighting` state on the device to read back, since the core can't write those yet either
(see below). `export-hcl keymap.hcl | rk-a72 import-hcl keymap.hcl` on an unmodified device is
a no-op.

Every `export-hcl` file starts with a self-documenting `#`-comment header: the `layer` block
syntax, a few canonical `key`/`mods`/`label`/`raw` examples, and the full reference lists
(every physical key name, KeyBoard symbol, modifier name, and non-KeyBoard label) — everything
needed to hand-edit the file, or for an AI code completer to infer valid values from context,
without cross-referencing `list-keys` or this README. It's inert HCL (all `#` comments) and
round-trips either way; edit it, trim it, or delete it freely.

The other sections are parsed and validated but **not yet flashable**, because the core has
no write path for them; `import-hcl` prints a note and skips each:

- `theme` + `lighting` (per-key RGB) — needs `SetLedColors` (opcode 6), un-reversed.
- `macro` definitions — need `SetMacros` (opcode 5), never exercised. A `macro` action on a
  layer key is rejected for the same reason.

Colours are still fully resolved during validation (theme aliases and CSS strings → RGB) and
macro event sequences fully parsed, so the front-end is ready the moment those opcodes land.

`export-hcl`/`import-hcl`/`get-keymap`/`set-keymap` HCL and CLI arguments are keyed
by physical key name (e.g. `Esc`, `M1`, `Mute`), not slot number — run `rk-a72 list-keys` to
see every valid physical key name, `key` symbol, and `mod` name. Each physical key has
`normal`/`fn` sub-entries for its un-Fn'd and Fn-held mappings.

### Shell completions

Completions are generated dynamically via `clap_complete`'s `CompleteEnv` — no separate
`generate-completions` subcommand. Follow your shell's standard hookup for a `clap`-based
dynamic completer (e.g. for bash: `source <(COMPLETE=bash rk-a72)`; see `clap_complete`
docs for other shells). Physical key names, `--label`/`--symbol` values, and `--mod`
segments (including multi-segment `LCtrl+LSh<Tab>` completion) are all completed live from
the same data tables `rk-a72-keymap` uses.

## Troubleshooting (Linux)

Without a udev rule granting your user access, opening the HID device will fail
(`Permission denied` / device not found). Add:

```
# /etc/udev/rules.d/70-rk-a72.rules
SUBSYSTEM=="hidraw", ATTRS{idVendor}=="258a", MODE="0666"
```

then `sudo udevadm control --reload-rules && sudo udevadm trigger`, and replug the keyboard.

## Troubleshooting (macOS)

**Gatekeeper won't let the binary run at all** ("can't be opened because Apple could
not verify it's free of malware" or similar). The release binary isn't code-signed or
notarized, so macOS quarantines anything downloaded through an app that uses its
quarantine-aware download APIs — this is what Safari/Chrome/Finder's "download" does,
not something specific to this binary. Two ways around it:

- Download via `curl` instead of a browser click — `curl` doesn't set the quarantine
  attribute, so the extracted binary runs with no prompt at all:
  ```sh
  curl -LO https://github.com/harnyk/rk-a72/releases/download/<tag>/rk-a72-<tag>-aarch64-apple-darwin.tar.gz
  tar -xzf rk-a72-<tag>-aarch64-apple-darwin.tar.gz
  ```
- Already downloaded it in a browser? Clear the attribute from the extracted binary
  directly, no re-download needed:
  ```sh
  xattr -d com.apple.quarantine rk-a72
  ```

**`rk-a72` needs `sudo` on macOS.** Confirmed on real hardware: every command fails with
`hid_open_path: failed to open IOHIDDevice from mach entry: (0xE00002C1) (iokit/common)
privilege violation` when run normally, and succeeds under `sudo`. Unlike Linux's udev
rule, there's no per-user permission grant that fixes this for a plain CLI binary —
macOS's HID access control for physical keyboards (Input Monitoring) doesn't show its
usual permission prompt for an unsigned, non-bundled executable, so there's currently no
way to run `rk-a72` as a normal user on macOS:

```sh
sudo rk-a72 export-hcl
sudo rk-a72 import-hcl keymap.hcl
```

If you find a way to avoid `sudo` (proper code signing + notarization + an
`Info.plist` might do it, but that's unverified and not currently set up in this
project's release process), please open an issue.
