# Example HCL config for `rk-a72 import-hcl` (issue #2 schema).
#
# What actually gets flashed today: ONLY the `layer` section, and within it only
# `key`(+`mods`) / `label` / `raw` actions — these map straight onto the KeyMatrix
# write path (SetKeyMatrix, opcode 3). Physical key names (Esc, M1, W, …), `key`
# symbols, `mod` names and `label` values are exactly what `rk-a72 list-keys` prints.
#
# What is parsed and validated but NOT flashed yet (import-hcl prints a note and skips):
#   - theme + lighting  → needs SetLedColors (opcode 6), not implemented in the core
#   - macro definitions → needs SetMacros  (opcode 5), not implemented in the core
# A `macro` action on a layer key is rejected for the same reason.

# ── Colour theme (CSS colours: HEX / rgb() / hsl() / names) ────────────────────
# Parsed & validated (resolved to RGB), but not flashed.
theme {
  main   = "#00FF00"
  accent = "rgb(0, 128, 255)"
  alert  = "red"
}

# ── Macros (ordered press / release / delay-in-ms steps) ───────────────────────
# Parsed & validated, but not flashed.
macro "copy" {
  events = [
    { press = "LCtrl" },
    { press = "C" },
    { delay = 50 },
    { release = "C" },
    { release = "LCtrl" },
  ]
}

# ── Layer mappings — THIS is what import-hcl flashes ───────────────────────────
# Only the "normal" and "fn" layers exist on the A72. Each key is its own repeatable
# `mapping "<PhysicalKey>" { ... }` block (v2 schema — a duplicate label within one
# layer is a hard error, not a silent overwrite). Note HCL's own grammar rule this
# relies on: a nested block's header must start on its own line, and attributes
# inside one block are newline- not comma-separated.
layer "fn" {
  mapping "Enter" {
    key = "Insert"                  # plain KeyBoard key
  }

  mapping "M1" {
    mods = ["LCtrl"]                # KeyBoard key + modifier(s)
    key  = "C"
  }

  mapping "Esc" {
    label = "Mute"                  # non-KeyBoard label (see list-keys)
  }

  mapping "W" {
    raw = "0x02000192"              # raw 4-byte slot value
  }

  # mapping "Del" { macro = "copy" } # ← would be rejected: macros not flashable yet
}

# ── Per-key RGB lighting ───────────────────────────────────────────────────────
# Parsed & validated (colours resolved via the theme aliases above or direct CSS),
# but not flashed.
lighting {
  effect     = "custom"
  brightness = 100   # 0..=100
  speed      = 5     # 0..=255
  colors = {
    "Esc"   = "alert"   # resolved via theme alias
    "W"     = "main"
    "Enter" = "gold"    # resolved directly as a CSS colour
  }
}
