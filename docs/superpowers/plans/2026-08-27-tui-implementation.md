# `rk-a72-tui` Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build `rk-a72-tui`, a `ratatui`-based terminal UI that lets a user browse and edit
their connected RK A72's keymap (all three layers) and per-key LED colors, buffering edits
in memory until an explicit Save writes them to the device.

**Architecture:** A new workspace crate depending on `rk-a72-keymap` directly (same
relationship `rk-a72-cli` has). Pure, unit-testable logic (dirty/customized diffing, Save
buffer construction, hex-input validation, geometry-table consistency) lives in small
standalone modules with no device or terminal dependency; the interactive rendering/input
loop is a thin layer on top of that logic, verified manually against real hardware since it
can't be meaningfully unit tested.

**Tech Stack:** Rust 2021, `ratatui` (terminal rendering), `crossterm` (terminal
backend/input), `hidapi` + `rk-a72-keymap` (device I/O, already in the workspace),
`anyhow` (error handling, matching `rk-a72-cli`'s existing choice).

**Spec:** `docs/superpowers/specs/2026-08-27-tui-design.md`

## Global Constraints

- No HCL anywhere in this crate — no import, no export, no `HclConfig`/`HclExporter` usage.
- No offline file editing — the only data source is the connected device via `WiredSession`.
- No live/streaming writes — every edit changes only `working_keymap`/`working_led` in
  memory; the device is written to only during Save.
- Device not found at startup → print an error and exit immediately, no TUI screen.
- Fixed-width key boxes — a key's rendered width is set once from its geometry entry and
  never grows to fit its current content.
- State color priority: dirty wins over customized (a slot that is both is rendered as dirty).

---

## File Structure

- `Cargo.toml` (modify) — add `rk-a72-tui` to `[workspace] members`.
- `rk-a72-tui/Cargo.toml` (create) — new crate manifest.
- `rk-a72-tui/src/state.rs` (create) — `AppState`, load-from-device, dirty/customized
  diffing, Save buffer construction. Pure logic, no I/O beyond the repository calls it's
  handed.
- `rk-a72-tui/src/geometry.rs` (create) — `KeyGeometry`, `A72_GEOMETRY` table, and the
  consistency test against `KeyboardModel`'s key set.
- `rk-a72-tui/src/color_input.rs` (create) — the three-hex-digit-pair RGB input widget's
  pure state machine (focus, digit entry, increment/decrement), no rendering.
- `rk-a72-tui/src/ui/mod.rs` (create) — top-level `draw(frame, &AppState, &UiState)`
  dispatch: tab bar + delegates to the active tab's render function.
- `rk-a72-tui/src/ui/keymap_tab.rs` (create) — keymap tab rendering (keyboard layout with
  state-colored boxes) and its action-edit modal.
- `rk-a72-tui/src/ui/led_tab.rs` (create) — LED tab rendering (borderless filled blocks) and
  its color-edit widget rendering.
- `rk-a72-tui/src/app.rs` (create) — `UiState` (active tab, active layer, cursor position,
  modal-open state), the event loop, and input dispatch.
- `rk-a72-tui/src/main.rs` (create) — CLI entry point: device discovery, session setup,
  `AppState` load, terminal init/teardown, hands off to `app::run`.

---

## Task 1: Workspace scaffolding for `rk-a72-tui`

**Files:**
- Modify: `Cargo.toml` (workspace root)
- Create: `rk-a72-tui/Cargo.toml`
- Create: `rk-a72-tui/src/main.rs` (stub)

**Interfaces:**
- Produces: a compiling, empty `rk-a72-tui` binary crate that later tasks build on.

- [ ] **Step 1: Add the crate to the workspace**

Edit `Cargo.toml` at the repo root:

```toml
[workspace]
resolver = "2"
members = ["rk-a72-keymap", "rk-a72-cli", "rk-a72-tui"]
```

- [ ] **Step 2: Create the crate manifest**

Create `rk-a72-tui/Cargo.toml`:

```toml
[package]
name = "rk-a72-tui"
version = "0.1.0"
edition = "2021"
license = "MIT"
description = "Interactive terminal UI for the wired RK A72 keyboard"
repository = "https://github.com/harnyk/rk-a72"

[[bin]]
name = "rk-a72-tui"
path = "src/main.rs"

[dependencies]
rk-a72-keymap = { path = "../rk-a72-keymap" }
ratatui = "0.29"
crossterm = "0.28"
anyhow = "1"
hidapi = { version = "2", default-features = false, features = ["linux-native-basic-udev"] }
```

- [ ] **Step 3: Create a stub `main.rs`**

Create `rk-a72-tui/src/main.rs`:

```rust
fn main() {
    println!("rk-a72-tui: not yet implemented");
}
```

- [ ] **Step 4: Verify the workspace builds**

Run: `cargo build --workspace`
Expected: builds successfully, including the new `rk-a72-tui` binary.

- [ ] **Step 5: Commit**

```bash
git add Cargo.toml rk-a72-tui/Cargo.toml rk-a72-tui/src/main.rs
git commit -m "Scaffold rk-a72-tui crate"
```

---

## Task 2: Geometry table and its consistency test

**Files:**
- Create: `rk-a72-tui/src/geometry.rs`
- Modify: `rk-a72-tui/src/main.rs` (add `mod geometry;`)

**Interfaces:**
- Consumes: `rk_a72_keymap::KeyboardModel::default_model()` (returns
  `&'static KeyboardModel`), `KeyboardModel::named_keys(&self) -> impl Iterator<Item = (u16, &'static str)>`.
- Produces: `pub struct KeyGeometry { pub name: &'static str, pub col: u16, pub row: u16, pub w: u16, pub h: u16 }`,
  `pub static A72_GEOMETRY: &[KeyGeometry]`, `pub fn geometry_for(name: &str) -> Option<&'static KeyGeometry>`.

This task creates the table with the **real, visually-authored coordinates** from
`tools/layout-editor.html` — the board's owner placed all 77 physically-present keys on the
tool's grid and exported them. Three names in `KeyboardModel`'s key set —
`Mute`, `IntlBackslash`, `Hash` — are deliberately absent: confirmed against the real
hardware, this specific A72 has no physical key at those matrix positions (the model
reserves the slot because the wider protocol family's scan matrix has it, not because this
board populates it — the same "some matrix positions have no keycap" situation
`model.rs`'s own doc comment already describes for unnamed slots). The consistency test
reflects that directly instead of requiring 100% coverage.

- [ ] **Step 1: Write the failing consistency tests**

Create `rk-a72-tui/src/geometry.rs`:

```rust
pub struct KeyGeometry {
    pub name: &'static str,
    pub col: u16,
    pub row: u16,
    pub w: u16,
    pub h: u16,
}

/// Model keys with no physical keycap on this specific A72 — confirmed against real
/// hardware. `KeyboardModel`'s key set includes them because the underlying protocol
/// family's scan matrix reserves the slot; this board just doesn't populate it. Kept as an
/// explicit, named exception (rather than a silent gap) so the consistency tests below
/// distinguish "deliberately unplaced" from "forgotten."
const KEYS_WITH_NO_PHYSICAL_KEYCAP: &[&str] = &["Mute", "IntlBackslash", "Hash"];

pub static A72_GEOMETRY: &[KeyGeometry] = &[];

pub fn geometry_for(name: &str) -> Option<&'static KeyGeometry> {
    A72_GEOMETRY.iter().find(|g| g.name == name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rk_a72_keymap::KeyboardModel;

    #[test]
    fn every_model_key_has_a_geometry_entry_or_is_a_known_unplaced_exception() {
        let model = KeyboardModel::default_model();
        for (_, name) in model.named_keys() {
            let placed = geometry_for(name).is_some();
            let known_exception = KEYS_WITH_NO_PHYSICAL_KEYCAP.contains(&name);
            assert!(
                placed || known_exception,
                "key {name:?} has no geometry entry in A72_GEOMETRY and isn't listed in \
                 KEYS_WITH_NO_PHYSICAL_KEYCAP — was it forgotten, or does this board really \
                 lack it?"
            );
        }
    }

    #[test]
    fn no_known_exception_actually_has_a_geometry_entry() {
        // Catches the table drifting out of sync the other way: if a "no physical keycap"
        // key later gets a geometry entry (e.g. corrected after further hardware
        // inspection), it must be removed from KEYS_WITH_NO_PHYSICAL_KEYCAP too.
        for &name in KEYS_WITH_NO_PHYSICAL_KEYCAP {
            assert!(
                geometry_for(name).is_none(),
                "{name:?} is listed as having no physical keycap, but has a geometry entry \
                 anyway — remove it from KEYS_WITH_NO_PHYSICAL_KEYCAP"
            );
        }
    }

    #[test]
    fn every_geometry_entry_matches_a_real_model_key() {
        let model = KeyboardModel::default_model();
        let known: std::collections::HashSet<&str> =
            model.named_keys().map(|(_, name)| name).collect();
        for g in A72_GEOMETRY {
            assert!(
                known.contains(g.name),
                "geometry entry {:?} names a key not in the model's key set",
                g.name
            );
        }
    }
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p rk-a72-tui geometry`
Expected: FAIL — `every_model_key_has_a_geometry_entry_or_is_a_known_unplaced_exception`
fails because `A72_GEOMETRY` is empty, so every key other than the 3 listed exceptions has
no entry.

- [ ] **Step 3: Fill in the real geometry table**

Replace the `A72_GEOMETRY` array in `rk-a72-tui/src/geometry.rs` with the 77-entry table
authored via `tools/layout-editor.html` (one entry per physically-present key; `Mute`,
`IntlBackslash`, and `Hash` are correctly absent — see `KEYS_WITH_NO_PHYSICAL_KEYCAP` above):

```rust
pub static A72_GEOMETRY: &[KeyGeometry] = &[
    KeyGeometry { name: "M5", col: 5, row: 5, w: 4, h: 4 },
    KeyGeometry { name: "M4", col: 5, row: 9, w: 4, h: 4 },
    KeyGeometry { name: "M3", col: 5, row: 13, w: 4, h: 4 },
    KeyGeometry { name: "M2", col: 5, row: 17, w: 4, h: 4 },
    KeyGeometry { name: "M1", col: 5, row: 21, w: 4, h: 4 },
    KeyGeometry { name: "Esc", col: 10, row: 5, w: 4, h: 4 },
    KeyGeometry { name: "Tab", col: 10, row: 9, w: 5, h: 4 },
    KeyGeometry { name: "CapsLock", col: 10, row: 13, w: 6, h: 4 },
    KeyGeometry { name: "LShift", col: 10, row: 17, w: 8, h: 4 },
    KeyGeometry { name: "LCtrl", col: 10, row: 21, w: 4, h: 4 },
    KeyGeometry { name: "Digit1", col: 14, row: 5, w: 4, h: 4 },
    KeyGeometry { name: "Q", col: 15, row: 9, w: 4, h: 4 },
    KeyGeometry { name: "A", col: 16, row: 13, w: 4, h: 4 },
    KeyGeometry { name: "LWin", col: 14, row: 21, w: 4, h: 4 },
    KeyGeometry { name: "Digit2", col: 18, row: 5, w: 4, h: 4 },
    KeyGeometry { name: "W", col: 19, row: 9, w: 4, h: 4 },
    KeyGeometry { name: "S", col: 20, row: 13, w: 4, h: 4 },
    KeyGeometry { name: "Z", col: 18, row: 17, w: 4, h: 4 },
    KeyGeometry { name: "LAlt", col: 18, row: 21, w: 4, h: 4 },
    KeyGeometry { name: "Digit3", col: 22, row: 5, w: 4, h: 4 },
    KeyGeometry { name: "E", col: 23, row: 9, w: 4, h: 4 },
    KeyGeometry { name: "D", col: 24, row: 13, w: 4, h: 4 },
    KeyGeometry { name: "X", col: 22, row: 17, w: 4, h: 4 },
    KeyGeometry { name: "Digit4", col: 26, row: 5, w: 4, h: 4 },
    KeyGeometry { name: "R", col: 27, row: 9, w: 4, h: 4 },
    KeyGeometry { name: "F", col: 28, row: 13, w: 4, h: 4 },
    KeyGeometry { name: "C", col: 26, row: 17, w: 4, h: 4 },
    KeyGeometry { name: "Digit5", col: 30, row: 5, w: 4, h: 4 },
    KeyGeometry { name: "T", col: 31, row: 9, w: 4, h: 4 },
    KeyGeometry { name: "G", col: 32, row: 13, w: 4, h: 4 },
    KeyGeometry { name: "V", col: 30, row: 17, w: 4, h: 4 },
    KeyGeometry { name: "SpaceL", col: 22, row: 21, w: 17, h: 4 },
    KeyGeometry { name: "Digit6", col: 34, row: 5, w: 4, h: 4 },
    KeyGeometry { name: "Y", col: 43, row: 9, w: 4, h: 4 },
    KeyGeometry { name: "H", col: 43, row: 13, w: 4, h: 4 },
    KeyGeometry { name: "B", col: 34, row: 17, w: 4, h: 4 },
    KeyGeometry { name: "Digit7", col: 46, row: 5, w: 4, h: 4 },
    KeyGeometry { name: "U", col: 47, row: 9, w: 4, h: 4 },
    KeyGeometry { name: "J", col: 47, row: 13, w: 4, h: 4 },
    KeyGeometry { name: "N", col: 44, row: 17, w: 4, h: 4 },
    KeyGeometry { name: "SpaceR", col: 41, row: 21, w: 17, h: 4 },
    KeyGeometry { name: "Digit8", col: 50, row: 5, w: 4, h: 4 },
    KeyGeometry { name: "I", col: 51, row: 9, w: 4, h: 4 },
    KeyGeometry { name: "K", col: 51, row: 13, w: 4, h: 4 },
    KeyGeometry { name: "M", col: 48, row: 17, w: 4, h: 4 },
    KeyGeometry { name: "Digit9", col: 54, row: 5, w: 4, h: 4 },
    KeyGeometry { name: "O", col: 55, row: 9, w: 4, h: 4 },
    KeyGeometry { name: "L", col: 55, row: 13, w: 4, h: 4 },
    KeyGeometry { name: "Comma", col: 52, row: 17, w: 4, h: 4 },
    KeyGeometry { name: "Digit0", col: 58, row: 5, w: 4, h: 4 },
    KeyGeometry { name: "P", col: 59, row: 9, w: 4, h: 4 },
    KeyGeometry { name: "Semicolon", col: 59, row: 13, w: 4, h: 4 },
    KeyGeometry { name: "Period", col: 56, row: 17, w: 4, h: 4 },
    KeyGeometry { name: "RAlt", col: 58, row: 21, w: 4, h: 4 },
    KeyGeometry { name: "Minus", col: 62, row: 5, w: 4, h: 4 },
    KeyGeometry { name: "BracketLeft", col: 63, row: 9, w: 4, h: 4 },
    KeyGeometry { name: "Quote", col: 63, row: 13, w: 4, h: 4 },
    KeyGeometry { name: "Slash", col: 60, row: 17, w: 4, h: 4 },
    KeyGeometry { name: "Equal", col: 66, row: 5, w: 4, h: 4 },
    KeyGeometry { name: "BracketRight", col: 67, row: 9, w: 4, h: 4 },
    KeyGeometry { name: "Fn1", col: 62, row: 21, w: 4, h: 4 },
    KeyGeometry { name: "Backspace", col: 70, row: 5, w: 8, h: 4 },
    KeyGeometry { name: "Backslash", col: 71, row: 9, w: 7, h: 4 },
    KeyGeometry { name: "Enter", col: 67, row: 13, w: 11, h: 4 },
    KeyGeometry { name: "RShift", col: 64, row: 17, w: 8, h: 4 },
    KeyGeometry { name: "Left", col: 70, row: 23, w: 4, h: 4 },
    KeyGeometry { name: "Up", col: 74, row: 19, w: 4, h: 4 },
    KeyGeometry { name: "Down", col: 74, row: 23, w: 4, h: 4 },
    KeyGeometry { name: "Del", col: 79, row: 5, w: 4, h: 4 },
    KeyGeometry { name: "PgUp", col: 79, row: 9, w: 4, h: 4 },
    KeyGeometry { name: "PgDn", col: 79, row: 13, w: 4, h: 4 },
    KeyGeometry { name: "Right", col: 78, row: 23, w: 4, h: 4 },
    KeyGeometry { name: "PrevTr", col: 0, row: 13, w: 4, h: 4 },
    KeyGeometry { name: "PlayPause", col: 0, row: 17, w: 4, h: 4 },
    KeyGeometry { name: "NextTr", col: 0, row: 21, w: 4, h: 4 },
    KeyGeometry { name: "Logo", col: 39, row: 0, w: 6, h: 4 },
    KeyGeometry { name: "VolumD", col: 0, row: 9, w: 4, h: 4 },
    KeyGeometry { name: "VolumI", col: 0, row: 5, w: 4, h: 4 },
];
```

- [ ] **Step 4: Wire the module into the crate**

Edit `rk-a72-tui/src/main.rs`, add at the top:

```rust
mod geometry;
```

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test -p rk-a72-tui geometry`
Expected: PASS — all 3 tests green.

- [ ] **Step 6: Commit**

```bash
git add rk-a72-tui/src/geometry.rs rk-a72-tui/src/main.rs
git commit -m "Add A72 key geometry table with the board's real layout coordinates"
```

---

## Task 3: `AppState` — load, dirty/customized diffing

**Files:**
- Create: `rk-a72-tui/src/state.rs`
- Modify: `rk-a72-tui/src/main.rs` (add `mod state;`)

**Interfaces:**
- Consumes: `rk_a72_keymap::{KeyboardModel, KeyMappingCodec}`;
  `KeyboardModel::factory_slot_maps(&self, codec: &KeyMappingCodec) -> HashMap<u8, HashMap<u16, u32>>`.
- Produces:
  - `pub struct AppState { pub device_keymap: HashMap<u8, HashMap<u16, u32>>, pub device_led: Vec<u8>, pub working_keymap: HashMap<u8, HashMap<u16, u32>>, pub working_led: Vec<u8>, pub factory_keymap: HashMap<u8, HashMap<u16, u32>> }`
  - `pub fn AppState::new(device_keymap: HashMap<u8, HashMap<u16, u32>>, device_led: Vec<u8>, factory_keymap: HashMap<u8, HashMap<u16, u32>>) -> AppState`
  - `pub fn AppState::keymap_slot_state(&self, layer: u8, slot: u16) -> SlotState` where
    `pub enum SlotState { Clean, Customized, Dirty }`
  - `pub fn AppState::led_slot_dirty(&self, slot: u16) -> bool`
  - `pub fn AppState::any_dirty(&self) -> bool`

This task covers state construction and read-only diffing only — Save (which mutates
`device_*` and talks to a `KeyMatrixRepository`/`LedColorRepository`) is Task 4.

- [ ] **Step 1: Write the failing tests**

Create `rk-a72-tui/src/state.rs`:

```rust
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlotState {
    Clean,
    Customized,
    Dirty,
}

pub struct AppState {
    pub device_keymap: HashMap<u8, HashMap<u16, u32>>,
    pub device_led: Vec<u8>,
    pub working_keymap: HashMap<u8, HashMap<u16, u32>>,
    pub working_led: Vec<u8>,
    pub factory_keymap: HashMap<u8, HashMap<u16, u32>>,
}

impl AppState {
    pub fn new(
        device_keymap: HashMap<u8, HashMap<u16, u32>>,
        device_led: Vec<u8>,
        factory_keymap: HashMap<u8, HashMap<u16, u32>>,
    ) -> Self {
        let working_keymap = device_keymap.clone();
        let working_led = device_led.clone();
        Self {
            device_keymap,
            device_led,
            working_keymap,
            working_led,
            factory_keymap,
        }
    }

    fn slot_value(map: &HashMap<u8, HashMap<u16, u32>>, layer: u8, slot: u16) -> u32 {
        map.get(&layer).and_then(|l| l.get(&slot)).copied().unwrap_or(0)
    }

    /// The current display state of one keymap slot, dirty taking priority over
    /// customized when a slot is both (working differs from device AND device differs
    /// from factory).
    pub fn keymap_slot_state(&self, layer: u8, slot: u16) -> SlotState {
        let working = Self::slot_value(&self.working_keymap, layer, slot);
        let device = Self::slot_value(&self.device_keymap, layer, slot);
        let factory = Self::slot_value(&self.factory_keymap, layer, slot);
        if working != device {
            SlotState::Dirty
        } else if device != factory {
            SlotState::Customized
        } else {
            SlotState::Clean
        }
    }

    /// Whether one LED slot's color differs between working and device state. LED has no
    /// factory baseline to diff against, unlike keymap.
    pub fn led_slot_dirty(&self, slot: u16) -> bool {
        let led_colors_slot_count = self.device_led.len() / 3;
        let (r_off, g_off, b_off) = (
            slot as usize,
            slot as usize + led_colors_slot_count,
            slot as usize + led_colors_slot_count * 2,
        );
        self.working_led.get(r_off) != self.device_led.get(r_off)
            || self.working_led.get(g_off) != self.device_led.get(g_off)
            || self.working_led.get(b_off) != self.device_led.get(b_off)
    }

    /// Whether anything at all — any keymap slot on any layer, or any LED slot — is dirty.
    /// Used to decide whether Save has anything to do.
    pub fn any_dirty(&self) -> bool {
        self.working_keymap != self.device_keymap || self.working_led != self.device_led
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn map(entries: &[(u8, u16, u32)]) -> HashMap<u8, HashMap<u16, u32>> {
        let mut out: HashMap<u8, HashMap<u16, u32>> = HashMap::new();
        for &(layer, slot, val) in entries {
            out.entry(layer).or_default().insert(slot, val);
        }
        out
    }

    #[test]
    fn new_clones_device_state_into_working_state() {
        let device_keymap = map(&[(0, 7, 0xAA)]);
        let device_led = vec![1, 2, 3];
        let factory_keymap = map(&[(0, 7, 0xAA)]);
        let state = AppState::new(device_keymap.clone(), device_led.clone(), factory_keymap);
        assert_eq!(state.working_keymap, device_keymap);
        assert_eq!(state.working_led, device_led);
    }

    #[test]
    fn keymap_slot_matching_factory_is_clean() {
        let keymap = map(&[(0, 7, 0xAA)]);
        let state = AppState::new(keymap.clone(), vec![], keymap);
        assert_eq!(state.keymap_slot_state(0, 7), SlotState::Clean);
    }

    #[test]
    fn keymap_slot_differing_from_factory_but_matching_device_is_customized() {
        let device_keymap = map(&[(0, 7, 0xBB)]);
        let factory_keymap = map(&[(0, 7, 0xAA)]);
        let state = AppState::new(device_keymap, vec![], factory_keymap);
        assert_eq!(state.keymap_slot_state(0, 7), SlotState::Customized);
    }

    #[test]
    fn keymap_slot_edited_this_session_is_dirty() {
        let device_keymap = map(&[(0, 7, 0xAA)]);
        let factory_keymap = map(&[(0, 7, 0xAA)]);
        let mut state = AppState::new(device_keymap, vec![], factory_keymap);
        state.working_keymap.get_mut(&0).unwrap().insert(7, 0xCC);
        assert_eq!(state.keymap_slot_state(0, 7), SlotState::Dirty);
    }

    #[test]
    fn dirty_wins_over_customized_when_a_slot_is_both() {
        // device already differs from factory (customized), AND the user has edited it
        // further this session (dirty) — dirty must win.
        let device_keymap = map(&[(0, 7, 0xBB)]);
        let factory_keymap = map(&[(0, 7, 0xAA)]);
        let mut state = AppState::new(device_keymap, vec![], factory_keymap);
        state.working_keymap.get_mut(&0).unwrap().insert(7, 0xCC);
        assert_eq!(state.keymap_slot_state(0, 7), SlotState::Dirty);
    }

    #[test]
    fn led_slot_unedited_is_not_dirty() {
        // 2 slots: R[2] G[2] B[2]
        let led = vec![10, 20, 30, 40, 50, 60];
        let state = AppState::new(HashMap::new(), led, HashMap::new());
        assert!(!state.led_slot_dirty(0));
        assert!(!state.led_slot_dirty(1));
    }

    #[test]
    fn led_slot_edited_is_dirty() {
        let led = vec![10, 20, 30, 40, 50, 60];
        let mut state = AppState::new(HashMap::new(), led, HashMap::new());
        state.working_led[0] = 99; // R of slot 0
        assert!(state.led_slot_dirty(0));
        assert!(!state.led_slot_dirty(1));
    }

    #[test]
    fn any_dirty_is_false_immediately_after_load() {
        let keymap = map(&[(0, 7, 0xAA)]);
        let led = vec![1, 2, 3];
        let state = AppState::new(keymap, led, HashMap::new());
        assert!(!state.any_dirty());
    }

    #[test]
    fn any_dirty_is_true_after_a_keymap_edit() {
        let keymap = map(&[(0, 7, 0xAA)]);
        let mut state = AppState::new(keymap, vec![], HashMap::new());
        state.working_keymap.get_mut(&0).unwrap().insert(7, 0xCC);
        assert!(state.any_dirty());
    }

    #[test]
    fn any_dirty_is_true_after_an_led_edit() {
        let led = vec![1, 2, 3];
        let mut state = AppState::new(HashMap::new(), led, HashMap::new());
        state.working_led[0] = 99;
        assert!(state.any_dirty());
    }
}
```

- [ ] **Step 2: Wire the module into the crate**

Edit `rk-a72-tui/src/main.rs`, add:

```rust
mod state;
```

- [ ] **Step 3: Run the tests to verify they pass**

Run: `cargo test -p rk-a72-tui state`
Expected: PASS — all 9 tests green. (Since this task writes the implementation directly
alongside its tests rather than in two passes, there's no separate "verify it fails" step —
the module didn't exist before this task.)

- [ ] **Step 4: Commit**

```bash
git add rk-a72-tui/src/state.rs rk-a72-tui/src/main.rs
git commit -m "Add AppState with dirty/customized keymap and LED diffing"
```

---

## Task 4: Save — buffer construction and device write

**Files:**
- Modify: `rk-a72-tui/src/state.rs`

**Interfaces:**
- Consumes: `rk_a72_keymap::{patch_buffer, KeyMatrixRepository, LedColorRepository}`;
  `patch_buffer(buffer: &mut [u8], slot_map: &HashMap<u16, u32>)`;
  `KeyMatrixRepository::write_layer(&self, layer: u8, buffer: &[u8]) -> HidResult<()>`;
  `LedColorRepository::write_colors(&self, buffer: &[u8]) -> HidResult<()>`;
  `LedColorRepository::enter_self_define(&self) -> HidResult<()>`.
- Produces: `pub fn AppState::dirty_layers(&self) -> Vec<u8>`,
  `pub fn AppState::build_layer_buffer(&self, layer: u8) -> Vec<u8>`,
  `pub fn AppState::save(&mut self, keymap_repo: &KeyMatrixRepository, led_repo: &LedColorRepository) -> hidapi::HidResult<()>`.

`build_layer_buffer` is the pure part (fully unit-testable — it only needs
`KEYMATRIX_BUFFER_LEN`, `working_keymap`, and `patch_buffer`); `save` is the impure part that
calls it and talks to the device, verified structurally here (does it call the right
repository methods, in the right order, only for dirty layers) and manually against real
hardware per the spec's testing section.

- [ ] **Step 1: Write the failing test for `dirty_layers` and `build_layer_buffer`**

Add to the `tests` module in `rk-a72-tui/src/state.rs`:

```rust
    #[test]
    fn dirty_layers_lists_only_layers_with_at_least_one_dirty_slot() {
        let device_keymap = map(&[(0, 7, 0xAA), (1, 8, 0xBB)]);
        let mut state = AppState::new(device_keymap, vec![], HashMap::new());
        state.working_keymap.get_mut(&0).unwrap().insert(7, 0xCC); // layer 0 dirty
        // layer 1 untouched
        assert_eq!(state.dirty_layers(), vec![0]);
    }

    #[test]
    fn dirty_layers_is_empty_when_nothing_changed() {
        let device_keymap = map(&[(0, 7, 0xAA)]);
        let state = AppState::new(device_keymap, vec![], HashMap::new());
        assert!(state.dirty_layers().is_empty());
    }

    #[test]
    fn build_layer_buffer_patches_working_slots_onto_a_zeroed_buffer() {
        use rk_a72_keymap::KEYMATRIX_BUFFER_LEN;
        let device_keymap = map(&[(0, 7, 0xAABBCCDD)]);
        let state = AppState::new(device_keymap, vec![], HashMap::new());
        let buf = state.build_layer_buffer(0);
        assert_eq!(buf.len(), KEYMATRIX_BUFFER_LEN);
        let slot_offset = 7 * 4;
        assert_eq!(&buf[slot_offset..slot_offset + 4], &[0xAA, 0xBB, 0xCC, 0xDD]);
    }

    #[test]
    fn build_layer_buffer_leaves_unmentioned_slots_zeroed() {
        use rk_a72_keymap::KEYMATRIX_BUFFER_LEN;
        let device_keymap = map(&[(0, 7, 0xAABBCCDD)]);
        let state = AppState::new(device_keymap, vec![], HashMap::new());
        let buf = state.build_layer_buffer(0);
        assert_eq!(buf.len(), KEYMATRIX_BUFFER_LEN);
        let other_offset = 8 * 4;
        assert_eq!(&buf[other_offset..other_offset + 4], &[0, 0, 0, 0]);
    }

    #[test]
    fn build_layer_buffer_reflects_working_not_device_state() {
        use rk_a72_keymap::KEYMATRIX_BUFFER_LEN;
        let device_keymap = map(&[(0, 7, 0xAABBCCDD)]);
        let mut state = AppState::new(device_keymap, vec![], HashMap::new());
        state.working_keymap.get_mut(&0).unwrap().insert(7, 0x11223344);
        let buf = state.build_layer_buffer(0);
        assert_eq!(buf.len(), KEYMATRIX_BUFFER_LEN);
        let slot_offset = 7 * 4;
        assert_eq!(&buf[slot_offset..slot_offset + 4], &[0x11, 0x22, 0x33, 0x44]);
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p rk-a72-tui state`
Expected: FAIL with "no method named `dirty_layers`/`build_layer_buffer` found".

- [ ] **Step 3: Implement `dirty_layers`, `build_layer_buffer`, and `save`**

Add to `rk-a72-tui/src/state.rs`, above the `#[cfg(test)]` module:

```rust
use rk_a72_keymap::{patch_buffer, KeyMatrixRepository, LedColorRepository, KEYMATRIX_BUFFER_LEN};

impl AppState {
    /// Layer numbers (0/1/2) that have at least one keymap slot dirty, in ascending order.
    pub fn dirty_layers(&self) -> Vec<u8> {
        let mut layers: Vec<u8> = self
            .working_keymap
            .keys()
            .filter(|&&layer| {
                let working = self.working_keymap.get(&layer).cloned().unwrap_or_default();
                let device = self.device_keymap.get(&layer).cloned().unwrap_or_default();
                working != device
            })
            .copied()
            .collect();
        layers.sort_unstable();
        layers
    }

    /// The full KEYMATRIX_BUFFER_LEN-byte buffer for one layer, built from
    /// `working_keymap[layer]` — every slot that layer's working map mentions is patched
    /// in; every other slot is zeroed, matching a freshly reset device's layout.
    pub fn build_layer_buffer(&self, layer: u8) -> Vec<u8> {
        let mut buffer = vec![0u8; KEYMATRIX_BUFFER_LEN];
        if let Some(slot_map) = self.working_keymap.get(&layer) {
            patch_buffer(&mut buffer, slot_map);
        }
        buffer
    }

    /// Writes every dirty keymap layer and, if any LED slot is dirty, the LED color
    /// buffer, to the device. On success, `device_keymap`/`device_led` become clones of
    /// the just-written `working_*` (clearing all dirty flags). On failure, `working_*` is
    /// left untouched so no in-progress edit is lost, and the error is returned for the
    /// caller to display — dirty flags remain so the caller can retry.
    pub fn save(
        &mut self,
        keymap_repo: &KeyMatrixRepository,
        led_repo: &LedColorRepository,
    ) -> hidapi::HidResult<()> {
        for layer in self.dirty_layers() {
            let buffer = self.build_layer_buffer(layer);
            keymap_repo.write_layer(layer, &buffer)?;
        }

        let led_dirty = (0..self.working_led.len() / 3).any(|slot| self.led_slot_dirty(slot as u16));
        if led_dirty {
            led_repo.enter_self_define()?;
            led_repo.write_colors(&self.working_led)?;
        }

        self.device_keymap = self.working_keymap.clone();
        self.device_led = self.working_led.clone();
        Ok(())
    }
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p rk-a72-tui state`
Expected: PASS — all tests (including the 4 new ones) green.

- [ ] **Step 5: Commit**

```bash
git add rk-a72-tui/src/state.rs
git commit -m "Add Save: dirty-layer buffer construction and device write"
```

---

## Task 5: Color input widget — pure state machine

**Files:**
- Create: `rk-a72-tui/src/color_input.rs`
- Modify: `rk-a72-tui/src/main.rs` (add `mod color_input;`)

**Interfaces:**
- Produces:
  - `pub enum ColorChannel { R, G, B }`
  - `pub struct ColorInput { pub r: u8, pub g: u8, pub b: u8, pub focused: ColorChannel }`
  - `pub fn ColorInput::new(r: u8, g: u8, b: u8) -> ColorInput`
  - `pub fn ColorInput::focus_next(&mut self)` / `pub fn ColorInput::focus_prev(&mut self)`
  - `pub fn ColorInput::increment_focused(&mut self)` / `pub fn ColorInput::decrement_focused(&mut self)`
  - `pub fn ColorInput::type_hex_digit(&mut self, digit: char)` — appends a hex digit to the
    focused channel's two-digit value (see behavior in tests below)
  - `pub fn ColorInput::rgb(&self) -> (u8, u8, u8)`

This is deliberately independent of any terminal/ratatui type so it's fully unit-testable;
`ui/led_tab.rs` (Task 8) renders it and forwards key events into it.

- [ ] **Step 1: Write the failing tests**

Create `rk-a72-tui/src/color_input.rs`:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorChannel {
    R,
    G,
    B,
}

pub struct ColorInput {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub focused: ColorChannel,
    /// Hex digits typed for the focused channel since it last gained focus or committed a
    /// full byte — at most 2. A fresh digit past the second replaces the buffer (matches
    /// "typing keeps overwriting the low nibble" hex-input convention).
    entry_buffer: String,
}

impl ColorInput {
    pub fn new(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b, focused: ColorChannel::R, entry_buffer: String::new() }
    }

    pub fn focus_next(&mut self) {
        self.focused = match self.focused {
            ColorChannel::R => ColorChannel::G,
            ColorChannel::G => ColorChannel::B,
            ColorChannel::B => ColorChannel::R,
        };
        self.entry_buffer.clear();
    }

    pub fn focus_prev(&mut self) {
        self.focused = match self.focused {
            ColorChannel::R => ColorChannel::B,
            ColorChannel::G => ColorChannel::R,
            ColorChannel::B => ColorChannel::G,
        };
        self.entry_buffer.clear();
    }

    fn focused_mut(&mut self) -> &mut u8 {
        match self.focused {
            ColorChannel::R => &mut self.r,
            ColorChannel::G => &mut self.g,
            ColorChannel::B => &mut self.b,
        }
    }

    pub fn increment_focused(&mut self) {
        let v = self.focused_mut();
        *v = v.saturating_add(1);
        self.entry_buffer.clear();
    }

    pub fn decrement_focused(&mut self) {
        let v = self.focused_mut();
        *v = v.saturating_sub(1);
        self.entry_buffer.clear();
    }

    /// Only `0-9a-fA-F` are accepted; anything else is ignored. Two digits set the
    /// channel's byte value directly; a third digit starts a fresh two-digit entry
    /// (the buffer never holds more than 2 characters).
    pub fn type_hex_digit(&mut self, digit: char) {
        if !digit.is_ascii_hexdigit() {
            return;
        }
        if self.entry_buffer.len() >= 2 {
            self.entry_buffer.clear();
        }
        self.entry_buffer.push(digit);
        if self.entry_buffer.len() == 2 {
            let value = u8::from_str_radix(&self.entry_buffer, 16).expect("validated hex digits");
            *self.focused_mut() = value;
        }
    }

    pub fn rgb(&self) -> (u8, u8, u8) {
        (self.r, self.g, self.b)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_starts_focused_on_r() {
        let input = ColorInput::new(1, 2, 3);
        assert_eq!(input.focused, ColorChannel::R);
        assert_eq!(input.rgb(), (1, 2, 3));
    }

    #[test]
    fn focus_next_cycles_r_g_b_r() {
        let mut input = ColorInput::new(0, 0, 0);
        assert_eq!(input.focused, ColorChannel::R);
        input.focus_next();
        assert_eq!(input.focused, ColorChannel::G);
        input.focus_next();
        assert_eq!(input.focused, ColorChannel::B);
        input.focus_next();
        assert_eq!(input.focused, ColorChannel::R);
    }

    #[test]
    fn focus_prev_cycles_r_b_g_r() {
        let mut input = ColorInput::new(0, 0, 0);
        input.focus_prev();
        assert_eq!(input.focused, ColorChannel::B);
        input.focus_prev();
        assert_eq!(input.focused, ColorChannel::G);
        input.focus_prev();
        assert_eq!(input.focused, ColorChannel::R);
    }

    #[test]
    fn increment_focused_channel_by_one() {
        let mut input = ColorInput::new(10, 0, 0);
        input.increment_focused();
        assert_eq!(input.r, 11);
    }

    #[test]
    fn increment_clamps_at_255() {
        let mut input = ColorInput::new(255, 0, 0);
        input.increment_focused();
        assert_eq!(input.r, 255);
    }

    #[test]
    fn decrement_focused_channel_by_one() {
        let mut input = ColorInput::new(10, 0, 0);
        input.decrement_focused();
        assert_eq!(input.r, 9);
    }

    #[test]
    fn decrement_clamps_at_zero() {
        let mut input = ColorInput::new(0, 0, 0);
        input.decrement_focused();
        assert_eq!(input.r, 0);
    }

    #[test]
    fn typing_two_hex_digits_sets_the_focused_channel() {
        let mut input = ColorInput::new(0, 0, 0);
        input.type_hex_digit('a');
        input.type_hex_digit('f');
        assert_eq!(input.r, 0xaf);
    }

    #[test]
    fn typing_uppercase_hex_digits_works() {
        let mut input = ColorInput::new(0, 0, 0);
        input.type_hex_digit('F');
        input.type_hex_digit('F');
        assert_eq!(input.r, 0xFF);
    }

    #[test]
    fn a_third_digit_starts_a_fresh_two_digit_entry() {
        let mut input = ColorInput::new(0, 0, 0);
        input.type_hex_digit('a');
        input.type_hex_digit('f');
        assert_eq!(input.r, 0xaf);
        input.type_hex_digit('0');
        input.type_hex_digit('1');
        assert_eq!(input.r, 0x01);
    }

    #[test]
    fn non_hex_characters_are_ignored() {
        let mut input = ColorInput::new(0, 0, 0);
        input.type_hex_digit('g');
        input.type_hex_digit('z');
        input.type_hex_digit(' ');
        assert_eq!(input.r, 0);
    }

    #[test]
    fn changing_focus_clears_the_pending_digit_entry() {
        let mut input = ColorInput::new(0, 0, 0);
        input.type_hex_digit('a'); // one digit typed, not yet committed
        input.focus_next();
        input.focus_prev();
        // back on R, but the single pending 'a' should have been cleared by the focus
        // change — typing one more digit should NOT combine with the stale 'a'.
        input.type_hex_digit('1');
        input.type_hex_digit('2');
        assert_eq!(input.r, 0x12);
    }

    #[test]
    fn typing_only_affects_the_focused_channel() {
        let mut input = ColorInput::new(0, 0, 0);
        input.focus_next(); // now on G
        input.type_hex_digit('5');
        input.type_hex_digit('5');
        assert_eq!(input.r, 0);
        assert_eq!(input.g, 0x55);
        assert_eq!(input.b, 0);
    }
}
```

- [ ] **Step 2: Wire the module into the crate**

Edit `rk-a72-tui/src/main.rs`, add:

```rust
mod color_input;
```

- [ ] **Step 3: Run the tests to verify they pass**

Run: `cargo test -p rk-a72-tui color_input`
Expected: PASS — all 13 tests green.

- [ ] **Step 4: Commit**

```bash
git add rk-a72-tui/src/color_input.rs rk-a72-tui/src/main.rs
git commit -m "Add ColorInput hex RGB entry state machine"
```

---

## Task 6: `UiState` — navigation and cursor logic

**Files:**
- Create: `rk-a72-tui/src/app.rs` (this task: the `UiState` struct and its pure
  navigation methods only; the event loop itself is Task 9)
- Modify: `rk-a72-tui/src/main.rs` (add `mod app;`)

**Interfaces:**
- Consumes: `crate::geometry::A72_GEOMETRY` (for cursor movement bounds — see Step 3).
- Produces:
  - `pub enum Tab { Keymap, Led }`
  - `pub enum Layer { Normal, Fn, Fn2 }` with `pub fn Layer::as_u8(self) -> u8`
  - `pub struct UiState { pub tab: Tab, pub layer: Layer, pub selected_key: &'static str }`
  - `pub fn UiState::new() -> UiState`
  - `pub fn UiState::switch_tab(&mut self, tab: Tab)`
  - `pub fn UiState::cycle_layer(&mut self)` (Normal -> Fn -> Fn2 -> Normal)
  - `pub fn UiState::move_cursor(&mut self, dx: i32, dy: i32)` — moves `selected_key` to
    whichever geometry entry is nearest in the given direction (see Step 3 for the exact
    nearest-neighbor rule)

- [ ] **Step 1: Write the failing tests for tab/layer switching**

Create `rk-a72-tui/src/app.rs`:

```rust
use crate::geometry::{geometry_for, A72_GEOMETRY};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Keymap,
    Led,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Layer {
    Normal,
    Fn,
    Fn2,
}

impl Layer {
    pub fn as_u8(self) -> u8 {
        match self {
            Layer::Normal => 0,
            Layer::Fn => 1,
            Layer::Fn2 => 2,
        }
    }
}

pub struct UiState {
    pub tab: Tab,
    pub layer: Layer,
    pub selected_key: &'static str,
}

impl UiState {
    pub fn new() -> Self {
        Self {
            tab: Tab::Keymap,
            layer: Layer::Normal,
            selected_key: A72_GEOMETRY.first().map(|g| g.name).unwrap_or("Esc"),
        }
    }

    pub fn switch_tab(&mut self, tab: Tab) {
        self.tab = tab;
    }

    pub fn cycle_layer(&mut self) {
        self.layer = match self.layer {
            Layer::Normal => Layer::Fn,
            Layer::Fn => Layer::Fn2,
            Layer::Fn2 => Layer::Normal,
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_starts_on_keymap_tab_normal_layer() {
        let ui = UiState::new();
        assert_eq!(ui.tab, Tab::Keymap);
        assert_eq!(ui.layer, Layer::Normal);
    }

    #[test]
    fn switch_tab_changes_the_active_tab() {
        let mut ui = UiState::new();
        ui.switch_tab(Tab::Led);
        assert_eq!(ui.tab, Tab::Led);
    }

    #[test]
    fn cycle_layer_goes_normal_fn_fn2_normal() {
        let mut ui = UiState::new();
        assert_eq!(ui.layer, Layer::Normal);
        ui.cycle_layer();
        assert_eq!(ui.layer, Layer::Fn);
        ui.cycle_layer();
        assert_eq!(ui.layer, Layer::Fn2);
        ui.cycle_layer();
        assert_eq!(ui.layer, Layer::Normal);
    }

    #[test]
    fn layer_as_u8_matches_keymatrix_layer_numbering() {
        assert_eq!(Layer::Normal.as_u8(), 0);
        assert_eq!(Layer::Fn.as_u8(), 1);
        assert_eq!(Layer::Fn2.as_u8(), 2);
    }
}
```

- [ ] **Step 2: Wire the module into the crate**

Edit `rk-a72-tui/src/main.rs`, add:

```rust
mod app;
```

- [ ] **Step 3: Run the tests to verify they pass, then add cursor-movement tests**

Run: `cargo test -p rk-a72-tui app`
Expected: PASS — the 4 tests above are green (this task's first pass has no cursor logic
yet, so there's nothing to fail against).

Now add cursor movement. Append to `UiState`'s `impl` block in `rk-a72-tui/src/app.rs`:

```rust
    /// Moves `selected_key` to the geometry entry whose center is nearest in the given
    /// direction from the current key's center, among entries strictly in that direction.
    /// (dx, dy) is a unit-ish direction, e.g. (1, 0) for right, (0, -1) for up. No-op if no
    /// entry exists in that direction.
    pub fn move_cursor(&mut self, dx: i32, dy: i32) {
        let Some(current) = geometry_for(self.selected_key) else { return };
        let (cx, cy) = (
            current.col as i32 + current.w as i32 / 2,
            current.row as i32 + current.h as i32 / 2,
        );

        let mut best: Option<(&'static str, i32)> = None;
        for g in A72_GEOMETRY {
            if g.name == self.selected_key {
                continue;
            }
            let (gx, gy) = (g.col as i32 + g.w as i32 / 2, g.row as i32 + g.h as i32 / 2);
            let (ddx, ddy) = (gx - cx, gy - cy);
            // Must be (weakly) in the requested direction and not directly opposite.
            let in_direction = if dx != 0 { ddx * dx > 0 } else { ddy * dy > 0 };
            if !in_direction {
                continue;
            }
            // Distance-squared, penalizing perpendicular offset so movement prefers
            // staying roughly in line over jumping to a far-off row/column.
            let perpendicular = if dx != 0 { ddy } else { ddx };
            let score = ddx * ddx + ddy * ddy + perpendicular * perpendicular * 4;
            if best.is_none() || score < best.unwrap().1 {
                best = Some((g.name, score));
            }
        }

        if let Some((name, _)) = best {
            self.selected_key = name;
        }
    }
```

- [ ] **Step 4: Write the failing tests for cursor movement**

Add to the `tests` module in `rk-a72-tui/src/app.rs`:

```rust
    #[test]
    fn move_cursor_right_moves_to_the_nearest_key_to_the_right() {
        // Real A72_GEOMETRY (Task 2) has Esc at (col 10, row 5) and Digit1 at
        // (col 14, row 5) — same row, adjacent columns.
        let mut ui = UiState::new();
        ui.selected_key = "Esc";
        ui.move_cursor(1, 0);
        assert_eq!(ui.selected_key, "Digit1");
    }

    #[test]
    fn move_cursor_left_moves_to_the_nearest_key_to_the_left() {
        let mut ui = UiState::new();
        ui.selected_key = "Digit1";
        ui.move_cursor(-1, 0);
        assert_eq!(ui.selected_key, "Esc");
    }

    #[test]
    fn move_cursor_with_no_key_in_that_direction_is_a_no_op() {
        // VolumI sits at col 0, row 5 — the leftmost key on its row in the real
        // A72_GEOMETRY table; nothing exists further left on that row.
        let mut ui = UiState::new();
        ui.selected_key = "VolumI";
        ui.move_cursor(-1, 0);
        assert_eq!(ui.selected_key, "VolumI");
    }

    #[test]
    fn move_cursor_down_moves_to_a_key_on_the_row_below() {
        // M5 is at (col 5, row 5); M4 is at (col 5, row 9) — same column, next row down.
        let mut ui = UiState::new();
        ui.selected_key = "M5";
        ui.move_cursor(0, 1);
        assert_eq!(ui.selected_key, "M4");
    }
```

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test -p rk-a72-tui app`
Expected: PASS — all 8 tests green.

- [ ] **Step 6: Commit**

```bash
git add rk-a72-tui/src/app.rs rk-a72-tui/src/main.rs
git commit -m "Add UiState: tab/layer switching and geometry-based cursor movement"
```

---

## Task 7: Keymap tab rendering and action-edit modal

**Files:**
- Create: `rk-a72-tui/src/ui/mod.rs`
- Create: `rk-a72-tui/src/ui/keymap_tab.rs`
- Modify: `rk-a72-tui/src/main.rs` (add `mod ui;`)

**Interfaces:**
- Consumes: `crate::state::{AppState, SlotState}`, `crate::app::{UiState, Layer}`,
  `crate::geometry::A72_GEOMETRY`, `ratatui::Frame`, `ratatui::layout::Rect`.
- Produces:
  - `pub fn ui::draw(frame: &mut ratatui::Frame, app: &AppState, ui: &UiState)`
  - `pub fn keymap_tab::render(frame: &mut ratatui::Frame, area: Rect, app: &AppState, ui: &UiState)`
  - `pub struct keymap_tab::ActionDialogState { pub selected_tab: keymap_tab::ActionKind, /* per-kind field state, see below */ }`
  - `pub enum keymap_tab::ActionKind { Key, Label, Raw }`

Rendering code is not unit-tested per the spec ("manual verification against a connected
A72 is the acceptance path for the interactive rendering/input loop") — this task's only
automated check is that the crate compiles with these functions wired in. The modal's
_input-handling_ logic (which raw `u32` a completed form produces) is pure and gets tested
in Task 9 alongside the rest of input dispatch, once the modal's field state shape is fixed
here.

- [ ] **Step 1: Create the module skeleton**

Create `rk-a72-tui/src/ui/mod.rs`:

```rust
pub mod keymap_tab;
pub mod led_tab;

use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Tabs};
use ratatui::Frame;

use crate::app::{Tab, UiState};
use crate::state::AppState;

pub fn draw(frame: &mut Frame, app: &AppState, ui: &UiState) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(0)])
        .split(frame.area());

    let titles = ["Keymap", "LED"].map(Line::from);
    let selected = match ui.tab {
        Tab::Keymap => 0,
        Tab::Led => 1,
    };
    let tabs = Tabs::new(titles)
        .select(selected)
        .highlight_style(Style::default().fg(Color::Yellow));
    frame.render_widget(tabs, chunks[0]);

    match ui.tab {
        Tab::Keymap => keymap_tab::render(frame, chunks[1], app, ui),
        Tab::Led => led_tab::render(frame, chunks[1], app, ui),
    }
}

pub fn key_box_area(col: u16, row: u16, w: u16, h: u16, origin: Rect) -> Rect {
    // 1 grid unit = 2 terminal columns wide, 1 terminal row tall (units are square in
    // "key fractions" but terminal character cells are roughly 1:2 w:h, so this keeps
    // rendered keys looking roughly square).
    Rect {
        x: origin.x + col * 2,
        y: origin.y + row,
        width: w * 2,
        height: h,
    }
}

pub fn status_line(text: &str) -> Paragraph {
    Paragraph::new(Span::raw(text))
}
```

- [ ] **Step 2: Implement the keymap tab renderer**

Create `rk-a72-tui/src/ui/keymap_tab.rs`:

```rust
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

use crate::app::UiState;
use crate::geometry::A72_GEOMETRY;
use crate::state::{AppState, SlotState};

use super::key_box_area;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActionKind {
    Key,
    Label,
    Raw,
}

/// Editable field state for the action-edit modal, opened when the user presses Enter on
/// a selected key. Holds one buffer per action kind so switching tabs inside the dialog
/// doesn't lose what was typed on another tab.
pub struct ActionDialogState {
    pub selected_tab: ActionKind,
    pub key_symbol: String,
    pub key_mods: Vec<String>,
    pub label: String,
    pub raw_hex: String,
}

impl ActionDialogState {
    pub fn new() -> Self {
        Self {
            selected_tab: ActionKind::Key,
            key_symbol: String::new(),
            key_mods: Vec::new(),
            label: String::new(),
            raw_hex: String::new(),
        }
    }

    pub fn next_tab(&mut self) {
        self.selected_tab = match self.selected_tab {
            ActionKind::Key => ActionKind::Label,
            ActionKind::Label => ActionKind::Raw,
            ActionKind::Raw => ActionKind::Key,
        };
    }

    pub fn prev_tab(&mut self) {
        self.selected_tab = match self.selected_tab {
            ActionKind::Key => ActionKind::Raw,
            ActionKind::Label => ActionKind::Key,
            ActionKind::Raw => ActionKind::Label,
        };
    }
}

fn state_color(state: SlotState) -> Color {
    match state {
        SlotState::Clean => Color::Gray,
        SlotState::Customized => Color::Green,
        SlotState::Dirty => Color::Yellow,
    }
}

pub fn render(frame: &mut Frame, area: Rect, app: &AppState, ui: &UiState) {
    for geo in A72_GEOMETRY {
        let box_area = key_box_area(geo.col, geo.row, geo.w, geo.h, area);
        if box_area.x >= area.x + area.width || box_area.y >= area.y + area.height {
            continue; // off-screen, skip rather than let ratatui panic on an invalid Rect
        }

        // Slot lookup: geometry only carries names; resolving a name to its KeyMatrix
        // slot for the active model is the same job PhysicalKeyboardLayout already does
        // (see rk-a72-keymap::layout) — the caller (Task 9's event loop / main.rs) is
        // expected to pass a resolved slot map alongside AppState in the fuller
        // integration; for rendering, slot_state is looked up the same way.
        let slot = crate::geometry::slot_for(geo.name).unwrap_or(0);
        let slot_state = app.keymap_slot_state(ui.layer.as_u8(), slot);
        let selected = geo.name == ui.selected_key;

        let border_set = if selected {
            ratatui::symbols::border::DOUBLE
        } else {
            ratatui::symbols::border::ROUNDED
        };
        let block = Block::default()
            .borders(Borders::ALL)
            .border_set(border_set)
            .style(Style::default().fg(state_color(slot_state)));
        let label = Paragraph::new(Line::from(Span::raw(geo.name))).block(block);
        frame.render_widget(label, box_area);
    }
}
```

- [ ] **Step 3: Add `slot_for` to the geometry module**

The renderer above needs a name→slot lookup for the connected model. Edit
`rk-a72-tui/src/geometry.rs`, add below `geometry_for`:

```rust
/// The KeyMatrix slot for a geometry key name, resolved against the default (only) A72
/// model. `None` if the name isn't one of the model's named keys — shouldn't happen for
/// any name actually in `A72_GEOMETRY` once its consistency tests pass, but callers get an
/// `Option` rather than a panic since this crosses from "known at compile time" (the
/// geometry table) to "known at runtime" (the model's key set) territory.
pub fn slot_for(name: &str) -> Option<u16> {
    rk_a72_keymap::KeyboardModel::default_model()
        .named_keys()
        .find(|(_, n)| *n == name)
        .map(|(slot, _)| slot)
}
```

- [ ] **Step 4: Wire the modules into the crate**

Edit `rk-a72-tui/src/main.rs`, add:

```rust
mod ui;
```

- [ ] **Step 5: Verify the crate compiles**

Run: `cargo build -p rk-a72-tui`
Expected: builds successfully with no errors (warnings about unused code are expected at
this point, since `main.rs` doesn't call any of this yet — that's Task 9).

- [ ] **Step 6: Commit**

```bash
git add rk-a72-tui/src/ui/mod.rs rk-a72-tui/src/ui/keymap_tab.rs rk-a72-tui/src/geometry.rs rk-a72-tui/src/main.rs
git commit -m "Add keymap tab rendering and action-edit dialog state"
```

---

## Task 8: LED tab rendering and color-edit widget

**Files:**
- Create: `rk-a72-tui/src/ui/led_tab.rs`
- Modify: `rk-a72-tui/src/ui/mod.rs` (already declares `pub mod led_tab;` from Task 7 —
  no change needed there)

**Interfaces:**
- Consumes: `crate::state::AppState`, `crate::app::UiState`, `crate::color_input::ColorInput`,
  `crate::geometry::{A72_GEOMETRY, slot_for}`, `ratatui::Frame`.
- Produces: `pub fn led_tab::render(frame: &mut ratatui::Frame, area: Rect, app: &AppState, ui: &UiState)`.

Same testing note as Task 7: rendering itself isn't unit-tested; the LED color buffer's
byte-offset math (which this renderer reads from) is already covered by Task 3/4's
`led_slot_dirty` tests.

- [ ] **Step 1: Implement the LED tab renderer**

Create `rk-a72-tui/src/ui/led_tab.rs`:

```rust
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::Span;
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

use crate::app::UiState;
use crate::geometry::{slot_for, A72_GEOMETRY};
use crate::state::AppState;

use super::key_box_area;

fn led_rgb(app: &AppState, slot: u16) -> (u8, u8, u8) {
    let count = app.working_led.len() / 3;
    let (i, r_off, g_off, b_off) = (slot as usize, slot as usize, slot as usize + count, slot as usize + count * 2);
    let _ = i;
    (
        app.working_led.get(r_off).copied().unwrap_or(0),
        app.working_led.get(g_off).copied().unwrap_or(0),
        app.working_led.get(b_off).copied().unwrap_or(0),
    )
}

pub fn render(frame: &mut Frame, area: Rect, app: &AppState, ui: &UiState) {
    for geo in A72_GEOMETRY {
        let box_area = key_box_area(geo.col, geo.row, geo.w, geo.h, area);
        if box_area.x >= area.x + area.width || box_area.y >= area.y + area.height {
            continue;
        }

        let Some(slot) = slot_for(geo.name) else { continue };
        let (r, g, b) = led_rgb(app, slot);
        let selected = geo.name == ui.selected_key;
        let dirty = app.led_slot_dirty(slot);

        let fg = if r as u16 + g as u16 + b as u16 > 380 { Color::Black } else { Color::White };
        let bg = Color::Rgb(r, g, b);

        let mut block = Block::default().style(Style::default().bg(bg).fg(fg));
        if selected {
            block = block.borders(Borders::ALL).border_style(Style::default().fg(Color::White));
        }

        let label = if dirty { format!("{}\u{25CF}", geo.name) } else { geo.name.to_string() };
        let widget = Paragraph::new(Span::raw(label)).block(block);
        frame.render_widget(widget, box_area);
    }
}
```

- [ ] **Step 2: Verify the crate compiles**

Run: `cargo build -p rk-a72-tui`
Expected: builds successfully.

- [ ] **Step 3: Commit**

```bash
git add rk-a72-tui/src/ui/led_tab.rs
git commit -m "Add LED tab rendering: borderless RGB-filled key blocks"
```

---

## Task 9: Event loop, input dispatch, and main entry point

**Files:**
- Modify: `rk-a72-tui/src/app.rs` (add the event loop and input-dispatch functions)
- Modify: `rk-a72-tui/src/main.rs` (replace the stub with the real entry point)

**Interfaces:**
- Consumes: everything produced by Tasks 1-8 (`AppState`, `UiState`, `ui::draw`,
  `geometry::slot_for`, `color_input::ColorInput`, `rk_a72_keymap::{find_wired_device,
  WiredSession, KeyMatrixRepository, LedColorRepository, KeyboardModel, KeyMappingCodec,
  SUPPORTED_VENDOR_ID, SUPPORTED_PRODUCT_ID}`, `crossterm` terminal setup APIs.
- Produces: `pub fn app::run(terminal: &mut ratatui::DefaultTerminal, app: &mut AppState, keymap_repo: &KeyMatrixRepository, led_repo: &LedColorRepository) -> anyhow::Result<()>`
  — the full interactive loop, exits when the user quits.

This is the task where everything gets wired together into a runnable binary. The dispatch
logic that decides "which key event maps to which action" is pure enough to unit-test (it's
a match on `crossterm::event::KeyEvent` producing an `Action` enum) even though driving the
actual terminal isn't; Step 1 covers that pure slice, Steps 3-6 wire up the impure loop
around it.

- [ ] **Step 1: Write the failing tests for input-to-action mapping**

Add near the top of `rk-a72-tui/src/app.rs` (after the existing `Layer`/`Tab`/`UiState`
code):

```rust
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Quit,
    SwitchTab(Tab),
    CycleLayer,
    MoveCursor(i32, i32),
    OpenActionDialog,
    Save,
    None,
}

/// Maps a raw key event to an application-level `Action`, independent of `UiState` — the
/// same key always maps to the same action regardless of which tab is active; it's the
/// caller's job to ignore actions that don't apply to the current tab (e.g. `OpenActionDialog`
/// is only acted on while `ui.tab == Tab::Keymap`).
pub fn dispatch_key(event: KeyEvent) -> Action {
    match (event.code, event.modifiers) {
        (KeyCode::Char('q'), KeyModifiers::NONE) => Action::Quit,
        (KeyCode::Char('s'), KeyModifiers::CONTROL) => Action::Save,
        (KeyCode::Char('1'), KeyModifiers::NONE) => Action::SwitchTab(Tab::Keymap),
        (KeyCode::Char('2'), KeyModifiers::NONE) => Action::SwitchTab(Tab::Led),
        (KeyCode::Tab, KeyModifiers::NONE) => Action::CycleLayer,
        (KeyCode::Left, KeyModifiers::NONE) => Action::MoveCursor(-1, 0),
        (KeyCode::Right, KeyModifiers::NONE) => Action::MoveCursor(1, 0),
        (KeyCode::Up, KeyModifiers::NONE) => Action::MoveCursor(0, -1),
        (KeyCode::Down, KeyModifiers::NONE) => Action::MoveCursor(0, 1),
        (KeyCode::Enter, KeyModifiers::NONE) => Action::OpenActionDialog,
        _ => Action::None,
    }
}

#[cfg(test)]
mod dispatch_tests {
    use super::*;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    #[test]
    fn q_quits() {
        assert_eq!(dispatch_key(key(KeyCode::Char('q'))), Action::Quit);
    }

    #[test]
    fn ctrl_s_saves() {
        let event = KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL);
        assert_eq!(dispatch_key(event), Action::Save);
    }

    #[test]
    fn plain_s_is_not_save() {
        assert_eq!(dispatch_key(key(KeyCode::Char('s'))), Action::None);
    }

    #[test]
    fn digit_1_switches_to_keymap_tab() {
        assert_eq!(dispatch_key(key(KeyCode::Char('1'))), Action::SwitchTab(Tab::Keymap));
    }

    #[test]
    fn digit_2_switches_to_led_tab() {
        assert_eq!(dispatch_key(key(KeyCode::Char('2'))), Action::SwitchTab(Tab::Led));
    }

    #[test]
    fn tab_cycles_layer() {
        assert_eq!(dispatch_key(key(KeyCode::Tab)), Action::CycleLayer);
    }

    #[test]
    fn arrow_keys_move_cursor() {
        assert_eq!(dispatch_key(key(KeyCode::Left)), Action::MoveCursor(-1, 0));
        assert_eq!(dispatch_key(key(KeyCode::Right)), Action::MoveCursor(1, 0));
        assert_eq!(dispatch_key(key(KeyCode::Up)), Action::MoveCursor(0, -1));
        assert_eq!(dispatch_key(key(KeyCode::Down)), Action::MoveCursor(0, 1));
    }

    #[test]
    fn enter_opens_action_dialog() {
        assert_eq!(dispatch_key(key(KeyCode::Enter)), Action::OpenActionDialog);
    }

    #[test]
    fn unmapped_key_is_none() {
        assert_eq!(dispatch_key(key(KeyCode::Char('z'))), Action::None);
    }
}
```

- [ ] **Step 2: Run the tests to verify they pass**

Run: `cargo test -p rk-a72-tui app`
Expected: PASS — all 9 new `dispatch_tests` green, plus the earlier `tests` module's 8
tests still green (17 total in `app.rs`).

- [ ] **Step 3: Implement the event loop**

Append to `rk-a72-tui/src/app.rs`:

```rust
use rk_a72_keymap::{KeyMatrixRepository, LedColorRepository};

use crate::state::AppState;

/// Runs the interactive loop until the user quits. Save errors are shown on the status
/// line rather than ending the loop, per the spec: working state is preserved on failure
/// and the user can retry.
pub fn run(
    terminal: &mut ratatui::DefaultTerminal,
    app: &mut AppState,
    keymap_repo: &KeyMatrixRepository,
    led_repo: &LedColorRepository,
) -> anyhow::Result<()> {
    let mut ui = UiState::new();
    let mut status: Option<String> = None;

    loop {
        terminal.draw(|frame| {
            crate::ui::draw(frame, app, &ui);
            if let Some(msg) = &status {
                let area = frame.area();
                let status_area = ratatui::layout::Rect {
                    x: area.x,
                    y: area.y + area.height.saturating_sub(1),
                    width: area.width,
                    height: 1,
                };
                frame.render_widget(crate::ui::status_line(msg), status_area);
            }
        })?;

        let event = crossterm::event::read()?;
        let crossterm::event::Event::Key(key_event) = event else { continue };
        if key_event.kind != crossterm::event::KeyEventKind::Press {
            continue;
        }

        match dispatch_key(key_event) {
            Action::Quit => return Ok(()),
            Action::SwitchTab(tab) => {
                ui.switch_tab(tab);
                status = None;
            }
            Action::CycleLayer => {
                if ui.tab == Tab::Keymap {
                    ui.cycle_layer();
                }
            }
            Action::MoveCursor(dx, dy) => ui.move_cursor(dx, dy),
            Action::OpenActionDialog => {
                // Full modal input handling (typing a symbol, choosing mods/labels) is UI
                // interaction verified manually against real hardware per the spec; this
                // task wires the dialog's *open* trigger through. The dialog's confirm
                // path writes into app.working_keymap via the same pattern build_layer_buffer
                // already exercises in tests (Task 4) — see ui::keymap_tab::ActionDialogState.
                status = Some(format!("editing {} (layer {:?}) — dialog UI pending", ui.selected_key, ui.layer));
            }
            Action::Save => match app.save(keymap_repo, led_repo) {
                Ok(()) => status = Some("Saved.".to_string()),
                Err(e) => status = Some(format!("Save failed: {e}")),
            },
            Action::None => {}
        }
    }
}
```

- [ ] **Step 4: Implement the real `main.rs`**

Replace the contents of `rk-a72-tui/src/main.rs`:

```rust
mod app;
mod color_input;
mod geometry;
mod state;
mod ui;

use anyhow::{Context, Result};
use hidapi::HidApi;
use rk_a72_keymap::{
    find_wired_device, KeyMappingCodec, KeyMatrixRepository, KeyboardModel, LedColorRepository,
    WiredSession, SUPPORTED_PRODUCT_ID, SUPPORTED_VENDOR_ID,
};

use state::AppState;

fn main() -> Result<()> {
    let api = HidApi::new().context("failed to initialize HID API")?;
    let device = find_wired_device(&api, SUPPORTED_VENDOR_ID, SUPPORTED_PRODUCT_ID)
        .context("No wired A72 device found. Is the keyboard connected via USB cable?")?;

    let session = WiredSession::open(&api, &device.path).context("failed to open device")?;
    let keymap_repo = KeyMatrixRepository::new(session);

    let session_for_led = WiredSession::open(&api, &device.path).context("failed to open device")?;
    let led_repo = LedColorRepository::new(session_for_led);

    let mut device_keymap = std::collections::HashMap::new();
    for layer in 0u8..3 {
        let buffer = keymap_repo.read_layer(layer).context("failed to read keymap layer")?;
        let mut slot_map = std::collections::HashMap::new();
        for (slot, chunk) in buffer.chunks_exact(4).enumerate() {
            let value = u32::from_be_bytes(chunk.try_into().unwrap());
            if value != 0 {
                slot_map.insert(slot as u16, value);
            }
        }
        device_keymap.insert(layer, slot_map);
    }

    let device_led = led_repo.read_colors().context("failed to read LED colors")?;

    let model = KeyboardModel::for_ids(SUPPORTED_VENDOR_ID, SUPPORTED_PRODUCT_ID)
        .expect("SUPPORTED_VENDOR_ID/SUPPORTED_PRODUCT_ID must resolve to a known model");
    let codec = KeyMappingCodec::new();
    let factory_keymap = model.factory_slot_maps(&codec);

    let mut app_state = AppState::new(device_keymap, device_led, factory_keymap);

    let mut terminal = ratatui::init();
    let result = app::run(&mut terminal, &mut app_state, &keymap_repo, &led_repo);
    ratatui::restore();

    result
}
```

- [ ] **Step 5: Verify the workspace builds**

Run: `cargo build --workspace`
Expected: builds successfully with no errors.

- [ ] **Step 6: Run the full test suite**

Run: `cargo test --workspace`
Expected: PASS — every test across `rk-a72-keymap`, `rk-a72-cli`, and `rk-a72-tui` green
(hardware-only tests remain `#[ignore]`d and are skipped, matching existing repo behavior).

- [ ] **Step 7: Manual smoke test against real hardware**

With an A72 connected via USB, run: `cargo run -p rk-a72-tui`
Expected:
- The TUI opens showing the Keymap tab with the keyboard layout drawn from the real
  `A72_GEOMETRY` table (Task 2), matching the board's actual physical arrangement. Confirm
  every key name renders in roughly the right place, arrow keys move the selection between
  boxes, `Tab` cycles the layer, `1`/`2` switch tabs, `Ctrl+S` shows "Saved." on the status
  line with nothing dirty, and `q` exits cleanly, restoring the terminal.

- [ ] **Step 8: Commit**

```bash
git add rk-a72-tui/src/app.rs rk-a72-tui/src/main.rs
git commit -m "Wire up the interactive event loop and main entry point"
```

---

## Follow-up (not part of this plan)

- Complete the action-edit modal's interactive rendering and the color-edit widget's
  interactive rendering (both have their pure logic in place — `ActionDialogState` in
  `ui/keymap_tab.rs` and `ColorInput` in `color_input.rs` — but Task 9 only wires the
  dialog's *open* trigger, not its on-screen form or confirm/cancel key handling, since that
  UI work is verified manually rather than by a task-level automated check).
- Macros tab (explicitly out of scope for this plan per the spec).
