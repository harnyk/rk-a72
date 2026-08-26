# rk-a72

A Cargo workspace at the repo root: `rk-a72-keymap` (protocol codec and device session
library) and `rk-a72-cli` (the `rk-a72` command-line tool).

The keyboard is configured through a single text format, HCL (see `rk-a72-keymap/src/hcl.rs`
and the `import-hcl`/`export-hcl` CLI commands). The in-memory model is format-neutral —
per-layer `{slot -> raw u32}` maps plus the `PhysicalKey` enum and the semantic
`factory_default` table — and is not tied to any on-disk format.

Supported hardware is the wired RK A72 (`258a:0216`) only — see `SUPPORTED_VENDOR_ID` in
`rk-a72-keymap/src/protocol.rs`. Every byte layout was verified against that one device,
so don't widen the accepted vid/pid without hardware to verify against.
