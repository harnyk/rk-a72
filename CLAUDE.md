# rk-a72

A Cargo workspace at the repo root: `rk-a72-keymap` (protocol codec and device session
library), `rk-a72-cli` (the `rk-a72` command-line tool), and `rk-a72-gui` (egui editor).

Supported hardware is the wired RK A72 (`258a:0216`) only — see `SUPPORTED_VENDOR_ID` in
`rk-a72-keymap/src/protocol.rs`. Every byte layout was verified against that one device,
so don't widen the accepted vid/pid without hardware to verify against.
