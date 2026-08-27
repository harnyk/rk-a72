# `rk-a72-tui` — design spec

Status: approved for implementation planning.

## Purpose

A terminal UI, built on `ratatui`, that lets a user browse and edit their RK A72's keymap
and per-key LED colors interactively, as a fuller alternative to the one-shot `rk-a72
set-keymap`/`get-keymap` CLI commands — without reimplementing anything `rk-a72-cli`
already does well (HCL import/export stays there; see [Non-goals](#non-goals)).

## Scope (v1)

- **Keymap tab**: view and edit all three layers (`normal`/`fn`/`fn2`) of the KeyMatrix,
  one key at a time, via a visual pseudo-graphical layout of the A72.
- **LED tab**: view and edit per-key custom RGB colors, same visual layout.
- **Macros**: out of scope for v1. No tab, no placeholder — added as a later iteration once
  keymap+LED prove the interaction model out.

## Non-goals

- **No HCL.** The TUI never imports or exports HCL. `rk-a72 export-hcl`/`import-hcl` already
  do this well and remain the tool for anyone who wants a portable config file. Duplicating
  that here would mean two divergent implementations of the same concern.
- **No offline file editing.** The TUI has exactly one source of truth: the connected
  device. It does not open, edit, or save any file itself.
- **No live/streaming device writes.** Every edit is buffered in memory until an explicit
  Save.

## Architecture

New crate `rk-a72-tui`, workspace member alongside `rk-a72-cli`, depending on
`rk-a72-keymap` directly (same relationship `rk-a72-cli` has today) plus `ratatui` +
`crossterm` for rendering/input.

### Startup

1. `find_wired_device` (from `rk-a72-keymap::session`) locates the A72. **Not found → print
   the same style of error `rk-a72-cli` uses ("connect via USB") and exit immediately** — no
   TUI screen is shown, no retry/wait loop.
2. Open a `WiredSession`, then read the full device state before drawing anything:
   - `KeyMatrixRepository::read_layer` for layers 0/1/2 (normal/fn/fn2)
   - `LedColorRepository::read_colors`
3. Compute each keymap slot's **customized** flag once at load, by diffing the read layer
   against `KeyboardModel::factory_slot_maps` for the connected model (`RK_A72` today,
   resolved the same way the CLI resolves it — by vid/pid via `KeyboardModel::for_ids`).
4. Enter the main event loop with a fully populated `AppState`.

### State model

One in-memory document, no file involved anywhere:

```rust
struct AppState {
    // as read from the device at startup — never mutated after load; the diff baseline
    device_keymap: HashMap<u8 /*layer*/, HashMap<u16 /*slot*/, u32 /*raw*/>>,
    device_led: Vec<u8>,                              // 378 bytes, as read

    // working copy — what the user is editing; starts as a clone of the above
    working_keymap: HashMap<u8, HashMap<u16, u32>>,
    working_led: Vec<u8>,

    // derived, recomputed on every edit (see State precedence below)
    factory_keymap: HashMap<u8, HashMap<u16, u32>>,    // from KeyboardModel::factory_slot_maps
}
```

- **dirty** (a keymap slot or LED slot): `working != device` for that slot.
- **customized** (keymap slots only — LED has no factory baseline to diff against):
  `device != factory` for that slot.
- No HCL types anywhere in this state — `working_keymap`/`device_keymap` are the same
  `{slot -> raw}` shape the rest of the crate already uses (`patch_buffer`'s input shape).

### Save

A single explicit action (keybinding TBD in the implementation plan, e.g. `Ctrl+S`):
1. For each layer with at least one dirty keymap slot: build the full 504-byte buffer from
   `working_keymap[layer]` (via the existing `patch_buffer` starting from a copy of
   `device_keymap[layer]`'s buffer) and `KeyMatrixRepository::write_layer`.
2. If any LED slot is dirty: `LedColorRepository::enter_self_define()` (once, if not already
   done this session) then `write_colors(working_led)`.
3. On success, set `device_keymap`/`device_led` = clone of the just-written `working_*`,
   which clears all dirty flags (customized flags are recomputed against the new
   `device_keymap`, same as at startup).
4. On failure (HID error), leave `working_*` untouched (nothing is lost) and show the error;
   dirty flags remain so the user can retry Save.

There is no per-key or per-tab save — Save always flushes every dirty slot across both tabs
in one pass, since both ultimately go through the same `WiredSession`.

## Navigation

- Top-level tabs: **Keymap**, **LED**. Switch with `Tab` or digit keys `1`/`2`.
- Inside Keymap, a second-level layer switch (`normal`/`fn`/`fn2`) — exact keybinding is an
  implementation detail (candidate: `[`/`]` or `Shift+Tab`/`Tab` at a sub-level), but the
  three layers are peers, not nested navigation.
- Within a tab, arrow keys move a cursor over the visual keyboard layout (below).

## Visual keyboard layout

### Geometry

A72-specific key geometry (per-key column/row position and box width in terminal cells)
lives **locally in `rk-a72-tui`**, not in `rk-a72-keymap::model`. It's a UI concern, not a
protocol or device fact — `KeyboardModel` stays free of anything visual, consistent with how
`model.rs` already separates protocol facts (`protocol.rs`) from per-device facts
(`model.rs`) from string-projection (`layout.rs`). A geometry table looks like:

```rust
struct KeyGeometry { name: &'static str, col: u16, row: u16, width: u16 }
static A72_GEOMETRY: &[KeyGeometry] = &[ /* one entry per named key in A72_KEYS */ ];
```

Every name in this table must resolve against the connected `KeyboardModel`'s key set (a
mismatch is a bug to catch in a test, mirroring how `model.rs` already tests its own
slot/name tables for consistency) — but the table itself, its authoring, and its values are
out of scope for this design; laying out ~81 named keys into a coherent ANSI-ish grid is
implementation work, not an architectural decision.

### Key box rendering

Confirmed via the visual companion (see prior conversation) — fixed-width, non-stretching
boxes; a key's box width is set once by its geometry entry and never grows to fit its
current label, matching "the keyboard shouldn't be rubber, it has strict geometry."

**Keymap tab** — rounded single border, key name centered inside:

```
╭─────╮      ╭───────╮
│  A  │      │ Enter │
╰─────╯      ╰───────╯
```

Selected key gets a double border instead of single (independent of color):

```
╔═════╗
║  A  ║
╚═════╝
```

Text/border color encodes state, by priority (dirty wins over customized):

| State | Meaning | Color |
|---|---|---|
| clean | `working == factory` for this slot | default/dim |
| customized | `device != factory`, `working == device` | green |
| dirty | `working != device` (regardless of customized) | yellow |

**LED tab** — no border at all (a bordered box at this size read as oversized in the visual
check); a key is a solid block filled with its actual RGB color via ANSI truecolor. The fill
IS the color — there's no separate "customized" concept for LED (no factory baseline to diff
against, unlike keymap). An unsaved (dirty) color gets a small corner marker (`●`) in a
fixed accent color, not a border, so it doesn't compete with the fill for legibility. The
selected key is the **only** key that gets a border on this tab — a thin single-line outline
— since color-fill is otherwise the entire visual language here.

### Action detail (Keymap tab)

Pressing `Enter` on a selected key opens a modal dialog to edit its action for the current
layer. The dialog has tabs for the four action shapes the KeyMatrix write path understands
(mirroring `Action`'s cases in `model.rs`: `Key`, `Mods`, `Label`, and raw passthrough):

- **Key** — a KeyBoard symbol (autocompleted from `KeyMappingCodec::list_keycode_symbols`)
  plus optional modifiers (from `list_modifier_names`)
- **Label** — a non-KeyBoard action, chosen from `KeyMappingCodec::list_labels`
- **Raw** — a raw 4-byte hex value, manually entered

`Tab`/arrow keys switch between the dialog's type tabs; each tab has its own input fields.
Confirming writes the resulting raw `u32` into `working_keymap[current_layer][slot]`
in-memory (not to the device) — this is exactly what marks the slot dirty.

### Color input (LED tab)

Selecting a key and entering color-edit mode shows three hex inputs (R/G/B, each two hex
digits, 00–FF) plus a live color-swatch preview via ANSI truecolor, confirmed via the visual
check:

- `Tab` or Left/Right arrow moves focus between the three inputs
- Typing hex digits edits the focused input directly, with strict validation (only `0-9a-fA-F`,
  clamped to two digits)
- Up/Down arrow increments/decrements the focused input's value by 1 (clamped to 00/FF)
- Confirming writes the three bytes into `working_led` at this slot's R/G/B offsets (see
  `docs/protocol/payloads.md`'s planar LED layout) — again in-memory only, marking the slot
  dirty.

## Error handling

- **No device at startup**: print an error and exit (see Startup above) — no interactive
  screen.
- **Device I/O failure during Save**: shown to the user (status line or a dismissible
  modal — implementation detail), working state is preserved, dirty flags remain so Save can
  be retried. The TUI does not attempt automatic reconnect/retry.
- **Device unplugged mid-session** (outside of Save): out of scope for v1 detection; the
  next HID operation (e.g. the next Save) surfaces the failure through the same path as any
  other I/O error. No background polling for device presence.

## Testing

- `rk-a72-tui` unit tests cover pure logic with no device/terminal I/O: dirty/customized
  diffing, the Save buffer-patching logic (reusing `patch_buffer`), hex-input validation,
  and geometry-table consistency against the connected `KeyboardModel`'s key set (mirroring
  the existing `key_slots_are_unique_and_in_frame` style of test in `model.rs`).
- No hardware-only TUI test is added in v1 — manual verification against a connected A72 is
  the acceptance path for the interactive rendering/input loop itself, consistent with how
  `rk-a72-keymap`'s own hardware tests are `#[ignore]`d and manually run.

## Open questions for the implementation plan (not architectural)

- Exact keybindings beyond what's specified above (Save, layer switch, dialog confirm/cancel).
- The ~81-entry A72 geometry table's actual column/row/width values (visual layout
  authoring, not a design decision).
- Whether the digit-row 5-wide boxes shown in the visual check need narrowing for the A72's
  full ~15-column widest row to fit comfortably in an 80-column terminal, or whether a
  minimum terminal width is simply documented as a requirement.
