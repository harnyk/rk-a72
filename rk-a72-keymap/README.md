# rk-a72-keymap

Protocol codec and device session library for the wired RK A72's "BeiYing" HID protocol.
Used by [`rk-a72-cli`](../rk-a72-cli/) but usable standalone as a library.

Verified against the wired RK A72 (`258a:0216`) only — see `SUPPORTED_VENDOR_ID` in
`protocol.rs`.

Covers only the wired KeyMatrix (keymap) portion of the protocol — reading and writing the
layer-0 / Fn-layer key mapping table (LED colors, macros, profile fields, etc are not
implemented here).

## What's here

- **`session`** — `WiredSession`, `find_wired_device`: opens a HID connection to the device
  and speaks the wired (Feature Report ID 9) request/response framing.
- **`protocol`** — the raw `GetKeyMatrix`/`SetKeyMatrix` wire format: packet
  build/decode, checksums.
- **`repository`** — `KeyMatrixRepository`: the decoded KeyMatrix as a layer0/Fn-layer table
  keyed by slot, with read/write helpers.
- **`codec`** — `KeyMappingCodec`, `DecodedMapping`: converts between the raw 32-bit mapping
  value on the wire and a structured form (KeyBoard symbol + modifiers, non-KeyBoard label,
  macro, or raw passthrough).
- **`layout`** — `PhysicalKeyboardLayout`: maps physical key names (`Esc`, `M1`, ...) to
  matrix slot numbers, from `data/physical_key_labels.json`.
- **`modifiers`** — `ModifierSet`: the `LCtrl+LShift`-style modifier name parsing/formatting.
- **`mapping_type`** — `KeyMappingType`: the type-byte discriminant (KeyBoard/Label/Macro/
  Custom/...) embedded in each raw mapping value.
- **`visual`** — display-only overrides for renamed symbolic names (see
  `data/visual_overrides.json`): some symbolic names were renamed for shell-safety, and
  this keeps the original glyph visible.
- **`yaml`** — `KeymapYamlSerializer`: the on-disk YAML format used by
  `rk-a72 export-keymap`/`import-keymap`.

## Data files (`data/`)

Static tables loaded at build/run time, not meant to be hand-edited without understanding
their source:

- `physical_key_labels.json` — matrix slot → physical key name.
- `key_mapping_table.json` — raw mapping value → label(s); `build.rs` validates at compile
  time that every non-KeyBoard/Macro/Custom label is unique (panics on ambiguity).
- `hid_keycode_table.json` — KeyBoard usage code → symbol name, plus non-US keycode/physical
  symbol overrides.
- `visual_overrides.json` — renamed-symbol → original-glyph display overrides.

## Testing

`tests/hardware_roundtrip.rs` talks to a real connected device (export → mutate → import →
re-export, assert round-trip) — requires actual hardware, not run in normal CI.
