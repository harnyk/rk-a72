# Protocol documentation

How the RK A72 talks HID, at three levels of detail. Start at the top and drill down only
as far as you need:

1. **This page** — the command set: what opcodes exist, what each one reads or writes, and
   how they compose into the features this repo exposes (keymap, LED colors, macros,
   profile). If you just want "what can be read/written," stop here.
2. [`hid-frame.md`](hid-frame.md) — the wire-level shape shared by every command: the
   519-byte Feature Report, its 7-byte header, and the two request layouts (the common one
   `build_request` produces, and the two macro-paging exceptions that don't fit it).
3. [`payloads.md`](payloads.md) — the byte layout *inside* each command's payload: the
   126-slot KeyMatrix, the planar LED color buffer, the 128-byte profile buffer, and the
   macro table's header/name/action encoding.

All three describe the wired RK A72 (`258a:0216`) only, reverse-engineered from USB
captures of the vendor's own configurator — see the root [README](../../README.md) for the
usual reverse-engineering caveats (no vendor spec, only one device verified against).

## Transport

Everything below rides on a single USB HID Feature Report, ID 9, sent/read via
`send_feature_report()`/`get_feature_report()` — plain synchronous request/response, no
interrupt transfers, no fragmentation at the HID layer (the macro table's own paging,
described below, is an application-level split, not a HID one).

## Commands

| Opcode | Value | Direction | Reads/writes | Payload size | Notes |
|---|---|---|---|---|---|
| `GetKeyMatrix`  | 131 | device → host | one layer's key mapping table | 504 bytes (126 × 4) | layer selected via the request's `byte1` field |
| `SetKeyMatrix`  | 3   | host → device | one layer's key mapping table | 504 bytes | same addressing as `GetKeyMatrix` |
| `GetProfile`    | 132 | device → host | the 128-byte profile/settings block | 128 bytes | LED mode selection lives inside at offset 9 |
| `SetProfile`    | 4   | host → device | the 128-byte profile/settings block | 128 bytes | used here only to flip into SelfDefine LED mode |
| `GetLedColors`  | 134 | device → host | per-key custom RGB | 378 bytes (126 × 3, planar) | only meaningful once the profile is in SelfDefine mode |
| `SetLedColors`  | 6   | host → device | per-key custom RGB | 378 bytes | same planar layout |
| `GetMacros`     | 133 | device → host | the whole macro table | up to 4096 bytes, paged | one request per 512-byte page, 8 pages |
| `SetMacros`     | 5   | host → device | the whole macro table | up to 4096 bytes, paged | fire-and-forget per page, no response read |

Byte layouts for each payload are in [`payloads.md`](payloads.md); the request/response
framing that carries them is in [`hid-frame.md`](hid-frame.md).

## What this repo actually uses each command for

- **Keymap** (`KeyMatrixRepository`, `rk-a72 export-hcl`/`import-hcl`/`get-keymap`/
  `set-keymap`) — `GetKeyMatrix`/`SetKeyMatrix` against layer 0 (`normal`) and layer 1
  (`fn`). Layer 2 (`fn2`) is addressable on the wire but has no factory mapping and isn't
  exercised by any CLI command today.
- **LED colors** (`LedColorRepository`) — `GetLedColors`/`SetLedColors` for the per-key RGB
  buffer, plus `GetProfile`/`SetProfile` to flip the device into SelfDefine mode first (a
  color write is accepted but displays nothing otherwise — confirmed on real hardware).
- **Macros** (`MacroRepository`) — `GetMacros`/`SetMacros`, paged.

## Provenance

Every field described in these three documents was worked out from USB captures of the
vendor's own (browser-based) configurator talking to a real, physical RK A72, then
confirmed by round-tripping the same bytes back through this crate — see
`rk-a72-keymap/tests/hardware_roundtrip.rs`, `led_roundtrip.rs`, and `macro_roundtrip.rs`.
Nothing here comes from a vendor spec; none exists publicly.
