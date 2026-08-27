# rk-a72

A Cargo workspace at the repo root: `rk-a72-keymap` (protocol codec and device session
library) and `rk-a72-cli` (the `rk-a72` command-line tool).

The keyboard is configured through a single text format, HCL (see `rk-a72-keymap/src/hcl.rs`
and the `import-hcl`/`export-hcl` CLI commands). The in-memory model is format-neutral —
per-layer `{slot -> raw u32}` maps — and is not tied to any on-disk format.

Devices are modelled as data: `rk-a72-keymap/src/model.rs` holds a `KeyboardModel` per
device (USB ids, named keys, semantic factory-default table), selected by vid/pid from the
`MODELS` registry. Adding a same-protocol keyboard (e.g. one with a numpad) is a new
`KeyboardModel` const, nothing more. Protocol-level facts (byte grammar, buffer sizes,
raw↔meaning encoding) live in `protocol.rs`/`codec.rs` and are shared across models.

Supported hardware is the wired RK A72 (`258a:0216`) only — see `SUPPORTED_VENDOR_ID` in
`rk-a72-keymap/src/protocol.rs`. Every byte layout was verified against that one device,
so don't widen the accepted vid/pid without hardware to verify against.
