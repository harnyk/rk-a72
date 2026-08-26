//! HCL configuration front-end for the A72 keymap.
//!
//! This module parses the HCL schema proposed in issue #2 (theme / macro / layer /
//! lighting) into fully-validated Rust structs and compiles the parts the core can
//! actually flash into the same per-layer `{layer -> {slot -> value}}` slot maps the
//! KeyMatrix write path consumes.
//!
//! ## What compiles to device bytecode today
//!
//! - The `layer` section, and within it only actions the existing `SetKeyMatrix`
//!   write path can represent: a KeyBoard key (with optional modifiers), a non-KeyBoard
//!   `label`, or a `raw` value. Those go straight into the 126-slot KeyMatrix buffer.
//! - `lighting.colors` — per-key RGB, via [`HclConfig::compile_lighting`], into the
//!   378-byte planar `SetLedColors` buffer (opcode 6). Confirmed against real USB
//!   captures. Writing it to the device also requires putting the profile into
//!   SelfDefine mode first (`LedColorRepository::enter_self_define`), which lives in
//!   `repository.rs`, not here — this module only encodes bytes.
//!
//! ## What is parsed and validated but NOT flashable yet
//!
//! - `macro` definitions — writing the macro table needs `SetMacros` (opcode 5), also
//!   never attempted. Event sequences are still parsed and validated;
//!   [`HclConfig::compile_macros`] returns [`KeymapError::HclUnsupported`]. A `macro`
//!   action inside a `layer` is likewise rejected by [`HclConfig::layer_slot_maps`],
//!   because a KeyMatrix reference to a macro index is meaningless until the macro table
//!   itself can be flashed.

use std::collections::HashMap;

use indexmap::IndexMap;
use serde::Deserialize;

use hcl::eval::Evaluate;

use crate::codec::KeyMappingCodec;
use crate::error::KeymapError;
use crate::layout::PhysicalKeyboardLayout;
use crate::modifiers::ModifierSet;
use crate::protocol::{LED_COLORS_BUFFER_LEN, LED_COLORS_SLOT_COUNT};

const LAYER_NORMAL: u8 = 0;
const LAYER_FN: u8 = 1;
const LAYER_FN2: u8 = 2;

fn layer_number(name: &str) -> Option<u8> {
    match name {
        "normal" => Some(LAYER_NORMAL),
        "fn" => Some(LAYER_FN),
        "fn2" => Some(LAYER_FN2),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Raw layer — mirrors the HCL document 1:1. `deny_unknown_fields` turns typos in
// action/event/lighting keys into hard errors instead of silently-ignored fields.
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct RawConfig {
    #[serde(default)]
    theme: IndexMap<String, String>,
    #[serde(rename = "macro", default)]
    macros: IndexMap<String, RawMacro>,
    #[serde(rename = "layer", default)]
    layers: IndexMap<String, RawLayer>,
    #[serde(default)]
    lighting: Option<RawLighting>,
}

#[derive(Debug, Deserialize)]
struct RawMacro {
    #[serde(default = "default_repeat")]
    repeat: u8,
    #[serde(default)]
    events: Vec<RawEvent>,
}

fn default_repeat() -> u8 {
    1
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawEvent {
    press: Option<String>,
    release: Option<String>,
    delay: Option<u32>,
    #[serde(rename = "type")]
    type_text: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawLayer {
    // v2: each key mapping is its own `mapping "<PhysicalKey>" { ... }` block, repeatable —
    // not a v1 `mappings = { "<PhysicalKey>" = {...} }` object attribute (no back-compat,
    // per issue #2's v2 spec). hcl-rs resolves same-named labeled blocks into exactly the
    // `{label: body}` map shape `IndexMap<String, RawAction>` already expects, so nothing
    // downstream of this field needs to change — see `check_duplicate_mapping_labels` for
    // why duplicate labels need a pre-check rather than relying on deserialization to catch
    // them.
    #[serde(rename = "mapping", default)]
    mappings: IndexMap<String, RawAction>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawAction {
    mods: Option<Vec<String>>,
    key: Option<String>,
    #[serde(rename = "macro")]
    macro_ref: Option<String>,
    label: Option<String>,
    raw: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawLighting {
    effect: Option<String>,
    brightness: Option<u32>,
    speed: Option<u32>,
    #[serde(default)]
    colors: IndexMap<String, String>,
}

// ---------------------------------------------------------------------------
// Validated layer — the public, resolved representation.
// ---------------------------------------------------------------------------

/// A single key referenced by a macro press/release. Macro steps can target a normal
/// key (own HID usage code) or a modifier pressed on its own (Ctrl/Shift/…), so the two
/// are kept distinct rather than flattened to a bare number.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MacroKey {
    Key(u16),
    Modifier(ModifierSet),
}

/// One ordered step of a macro. `Delay` is in milliseconds, as written in the HCL.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MacroEvent {
    Press(MacroKey),
    Release(MacroKey),
    Delay(u32),
}

/// A validated macro definition. The event list preserves the source order, which is
/// exactly what a future `SetMacros` encoder will replay.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MacroDef {
    pub name: String,
    pub repeat: u8,
    pub events: Vec<MacroEvent>,
}

/// One resolved layer action, in the same three flavours the KeyMatrix write path
/// understands, plus a `Macro` variant that is parsed but not yet flashable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LayerAction {
    /// A KeyBoard key with optional modifiers — encodes directly to a slot value.
    Keyboard { key_code: u16, modifiers: ModifierSet },
    /// A non-KeyBoard label (Media/Mouse/…) — encodes directly to a slot value.
    Label(u32),
    /// A raw 4-byte slot value.
    Raw(u32),
    /// A reference to a `macro` block by name. Cannot be flashed until the macro table
    /// can be written; carried through so the front-end is complete.
    Macro(String),
}

impl LayerAction {
    /// The 4-byte KeyMatrix slot value, when this action can be represented as one
    /// without needing the macro table (i.e. everything except `Macro`, which
    /// `HclConfig::layer_slot_maps` resolves separately via `encode_macro_reference`
    /// since it needs the macro's table index, not just its own fields).
    pub fn to_slot_value(&self) -> Option<u32> {
        match self {
            LayerAction::Keyboard { key_code, modifiers } => {
                Some(KeyMappingCodec::encode_keyboard(*key_code, *modifiers))
            }
            LayerAction::Label(raw) | LayerAction::Raw(raw) => Some(KeyMappingCodec::encode_raw(*raw)),
            LayerAction::Macro(_) => None,
        }
    }
}

/// One entry of the per-key lighting map: which physical key, its KeyMatrix slot, and
/// the resolved RGB triple.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyColor {
    pub key: String,
    pub slot: u16,
    pub rgb: [u8; 3],
}

/// The validated `lighting` block. `brightness`/`speed` are range-checked to 0..=100 and
/// 0..=255 respectively; every color is resolved to an `[R, G, B]` triple.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LightingConfig {
    pub effect: Option<String>,
    pub brightness: Option<u8>,
    pub speed: Option<u8>,
    pub colors: Vec<KeyColor>,
}

/// A fully parsed and validated HCL configuration.
#[derive(Debug)]
pub struct HclConfig {
    theme: IndexMap<String, [u8; 3]>,
    macros: Vec<MacroDef>,
    /// layer number → (slot → action)
    layers: HashMap<u8, IndexMap<u16, LayerAction>>,
    lighting: Option<LightingConfig>,
}

/// Two sibling `mapping "X" { ... }` blocks sharing the same label don't error during
/// typed deserialization — hcl-rs resolves same-labeled sibling blocks by collapsing them
/// into a sequence under that label, which then fails against `RawAction` (a single
/// object) with an opaque "invalid type: sequence, expected a map"-shaped error instead of
/// naming the actual problem. Walking the raw parse tree first catches it with a message
/// that names the layer and the repeated key.
fn check_duplicate_mapping_labels(body: &hcl::Body) -> Result<(), KeymapError> {
    for structure in &body.0 {
        let hcl::Structure::Block(layer_block) = structure else {
            continue;
        };
        if layer_block.identifier.as_str() != "layer" {
            continue;
        }
        let layer_name = layer_block.labels.first().map(hcl::BlockLabel::as_str).unwrap_or("");
        let mut seen = std::collections::HashSet::new();
        for inner in &layer_block.body.0 {
            let hcl::Structure::Block(mapping_block) = inner else {
                continue;
            };
            if mapping_block.identifier.as_str() != "mapping" {
                continue;
            }
            let key_name = mapping_block.labels.first().map(hcl::BlockLabel::as_str).unwrap_or("");
            if !seen.insert(key_name) {
                return Err(KeymapError::HclValidation {
                    context: format!("layer \"{layer_name}\""),
                    detail: format!("duplicate mapping \"{key_name}\""),
                });
            }
        }
    }
    Ok(())
}

fn parse_css_color(context: &str, s: &str) -> Result<[u8; 3], KeymapError> {
    let c = s
        .parse::<csscolorparser::Color>()
        .map_err(|e| KeymapError::HclValidation {
            context: context.to_string(),
            detail: format!("\"{s}\" is not a valid CSS color ({e})"),
        })?;
    let [r, g, b, _a] = c.to_rgba8();
    Ok([r, g, b])
}

impl HclConfig {
    /// Parse and fully validate an HCL document. Every physical key name, key symbol,
    /// modifier, label, color and macro step is resolved here — a returned `HclConfig`
    /// is guaranteed internally consistent. Nothing touches hardware.
    pub fn parse(text: &str) -> Result<Self, KeymapError> {
        let codec = KeyMappingCodec::new();
        let layout = PhysicalKeyboardLayout::new();
        Self::parse_with(text, &codec, &layout)
    }

    /// Same as [`HclConfig::parse`] but reuses caller-owned codec/layout tables instead
    /// of building fresh ones (they load JSON on construction).
    pub fn parse_with(
        text: &str,
        codec: &KeyMappingCodec,
        layout: &PhysicalKeyboardLayout,
    ) -> Result<Self, KeymapError> {
        // Parse to the raw AST first (rather than `hcl::from_str` straight to `RawConfig`)
        // so duplicate `mapping "X"` labels can be caught with a clear error before
        // deserialization — see `check_duplicate_mapping_labels`.
        let body = hcl::parse(text).map_err(|e| KeymapError::Hcl(e.to_string()))?;
        check_duplicate_mapping_labels(&body)?;

        // `${env("VAR")}` interpolation: the whole document is HCL-template-evaluated
        // against a context that declares a single `env` function (rather than a flat
        // `${VAR}`, so a secret's origin is always explicit in the file, and there's no
        // risk of colliding with some other variable this module declares in the
        // future). No secret ever touches disk — only the *name* of the env var lives
        // in the HCL file; the value is read from the process environment at parse
        // time. `env(...)` reads `std::env::var` directly rather than a value captured
        // in the `Context` — `hcl::eval::Func` is a plain `fn` pointer, not a closure,
        // so it cannot capture anything — and returns "" for an unset variable instead
        // of erroring, so `env("X") != "" ? env("X") : "default"` is expressible in HCL.
        let mut ctx = hcl::eval::Context::new();
        ctx.declare_func(
            "env",
            hcl::eval::FuncDef::new(env_func, [hcl::eval::ParamType::String]),
        );
        let evaluated = body.evaluate(&ctx).map_err(|e| KeymapError::Hcl(e.to_string()))?;

        let raw: RawConfig = hcl::from_body(evaluated).map_err(|e| KeymapError::Hcl(e.to_string()))?;

        // theme: every value is a CSS color resolved up front, so lighting aliases and
        // any future consumer see [R, G, B], never a string to re-parse.
        let mut theme = IndexMap::new();
        for (name, value) in &raw.theme {
            let rgb = parse_css_color(&format!("theme \"{name}\""), value)?;
            theme.insert(name.clone(), rgb);
        }

        let macros = validate_macros(&raw.macros, codec)?;
        let encoded_len = crate::macros::encode_macro_table(
            &macros.iter().map(compile_one_macro).collect::<Vec<_>>(),
        )
        .len();
        if encoded_len > crate::macros::MACRO_BUFFER_LEN {
            return Err(KeymapError::HclValidation {
                context: "macro table".to_string(),
                detail: format!(
                    "all `macro` blocks together encode to {encoded_len} bytes, which exceeds the \
                     device's {} byte macro table capacity — remove or shorten some macros",
                    crate::macros::MACRO_BUFFER_LEN
                ),
            });
        }
        let layers = validate_layers(&raw.layers, &raw.macros, codec, layout)?;
        let lighting = raw
            .lighting
            .as_ref()
            .map(|l| validate_lighting(l, &theme, layout))
            .transpose()?;

        Ok(Self {
            theme,
            macros,
            layers,
            lighting,
        })
    }

    /// Resolved theme aliases (name → RGB).
    pub fn theme(&self) -> &IndexMap<String, [u8; 3]> {
        &self.theme
    }

    /// Validated macro definitions, in source order.
    pub fn macros(&self) -> &[MacroDef] {
        &self.macros
    }

    /// The validated lighting block, if the document had one.
    pub fn lighting(&self) -> Option<&LightingConfig> {
        self.lighting.as_ref()
    }

    /// Validated per-layer actions (layer number → slot → action), macro references
    /// included. Use [`HclConfig::layer_slot_maps`] for the flashable subset.
    pub fn layers(&self) -> &HashMap<u8, IndexMap<u16, LayerAction>> {
        &self.layers
    }

    /// Compile the `layer` section to the per-layer `{slot -> value}` maps the
    /// KeyMatrix write path consumes — the `{layer -> {slot -> value}}` shape the CLI
    /// import loop applies to the device. A `macro = "name"` layer action
    /// resolves via [`Self::encode_macro_reference`] instead of
    /// [`LayerAction::to_slot_value`], since it needs the macro's table index rather
    /// than just its own fields.
    pub fn layer_slot_maps(&self) -> Result<HashMap<u8, HashMap<u16, u32>>, KeymapError> {
        let mut out: HashMap<u8, HashMap<u16, u32>> = HashMap::new();
        out.insert(LAYER_NORMAL, HashMap::new());
        out.insert(LAYER_FN, HashMap::new());
        out.insert(LAYER_FN2, HashMap::new());

        for (&layer, slots) in &self.layers {
            let target = out.get_mut(&layer).expect("layer preseeded above");
            for (&slot, action) in slots {
                let value = match action {
                    LayerAction::Macro(name) => self.encode_macro_reference(name)?,
                    _ => action
                        .to_slot_value()
                        .expect("non-Macro LayerAction always has a slot value"),
                };
                target.insert(slot, value);
            }
        }
        Ok(out)
    }

    /// Resolves a `macro = "name"` layer reference to its `KeyMappingType::Macro` slot
    /// value: `byte3 = keyMappingType (Macro)`, `byte2 = keyMappingPara` (the
    /// configurator's "cycle type", always `1` — "repeat N times" — when assigning a
    /// macro via its UI; there's no other cycle type to select), `byte1 = repeat`
    /// count, `byte0` = the macro's index in `self.macros` (source order — the same
    /// order `compile_macros` encodes the table in). Confirmed against the real
    /// configurator's behavior: omitting `keyMappingPara = 1` (as an earlier version of
    /// this method did, packing `repeat` into that byte instead of its own) silently
    /// produces a slot the firmware never plays back.
    fn encode_macro_reference(&self, name: &str) -> Result<u32, KeymapError> {
        let (index, def) = self
            .macros
            .iter()
            .enumerate()
            .find(|(_, m)| m.name == name)
            .expect("validate_macros already rejected undefined macro references");
        let index = u8::try_from(index).map_err(|_| KeymapError::HclValidation {
            context: format!("macro \"{name}\""),
            detail: "more than 255 macros defined — the device's macro index is a single byte".to_string(),
        })?;
        const CYCLE_TYPE_REPEAT_N_TIMES: u32 = 1;
        let key_code = ((def.repeat as u32) << 8) | (index as u32);
        Ok(((crate::mapping_type::KeyMappingType::Macro.to_byte() as u32) << 24)
            | (CYCLE_TYPE_REPEAT_N_TIMES << 16)
            | key_code)
    }

    /// Same macro definitions `compile_macros` encodes, but as the `macros::Macro`
    /// structs `MacroRepository::write_macros` takes directly — avoids an
    /// encode-then-immediately-decode round trip for callers that want structs, not
    /// bytes. `compile_macros`'s `Vec<u8>` return stays for symmetry with
    /// `compile_lighting` and any caller that wants raw bytes to inspect/test.
    pub fn compiled_macros(&self) -> Vec<crate::macros::Macro> {
        self.macros.iter().map(compile_one_macro).collect()
    }

    /// Encodes the validated `macro` definitions to the 4096-byte-format `SetMacros`
    /// buffer (`macros.rs`'s `encode_macro_table`), in source order — that order is
    /// exactly the macro table index `LayerAction::Macro` resolution below uses, and
    /// what the CLI's import ordering (macros written before KeyMatrix) depends on.
    pub fn compile_macros(&self) -> Result<Vec<u8>, KeymapError> {
        Ok(crate::macros::encode_macro_table(&self.compiled_macros()))
    }

    /// Encodes the `lighting.colors` map to the 378-byte planar `R[126] G[126] B[126]`
    /// buffer `SetLedColors` expects, keyed by the same 0..125 slot as the KeyMatrix.
    /// Keys the document doesn't mention are left black (`0,0,0`) — there is no
    /// factory-default LED dump to merge against, and an all-zero base matches the
    /// device's own factory state (confirmed via USB capture).
    ///
    /// Returns an all-zero buffer if the document had no `lighting` block, or the
    /// block had no colors — callers should treat that as "nothing to flash" rather
    /// than skip calling this.
    pub fn compile_lighting(&self) -> Result<Vec<u8>, KeymapError> {
        let mut buffer = vec![0u8; LED_COLORS_BUFFER_LEN];
        let Some(lighting) = &self.lighting else {
            return Ok(buffer);
        };
        for color in &lighting.colors {
            let slot = color.slot as usize;
            buffer[slot] = color.rgb[0];
            buffer[slot + LED_COLORS_SLOT_COUNT] = color.rgb[1];
            buffer[slot + LED_COLORS_SLOT_COUNT * 2] = color.rgb[2];
        }
        Ok(buffer)
    }
}

/// Escapes a string for use inside an HCL double-quoted string literal. Physical key
/// names, KeyBoard symbols and labels in this codebase never contain control
/// characters, so only the two characters that would otherwise terminate or corrupt
/// the literal need handling.
fn hcl_escape(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

/// Wraps a comma-joined list at roughly `width` columns, indenting continuation lines
/// with `#   ` so the whole block stays a valid HCL comment. Keeps every reference list
/// in the exported header to a handful of lines instead of one name per line, since
/// there can be 100+ entries per category.
fn wrap_comment_list(items: &[String], width: usize) -> String {
    let mut lines = Vec::new();
    let mut line = String::new();
    for (i, item) in items.iter().enumerate() {
        let piece = if i + 1 == items.len() {
            item.clone()
        } else {
            format!("{item}, ")
        };
        if !line.is_empty() && line.len() + piece.len() > width {
            lines.push(std::mem::take(&mut line));
        }
        line.push_str(&piece);
    }
    if !line.is_empty() {
        lines.push(line);
    }
    lines
        .iter()
        .map(|l| format!("#   {}", l.trim_end()))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Builds the self-documenting header every `HclExporter` dump is prefixed with: the
/// `layer` block syntax, a few canonical examples, and the full reference lists (every
/// physical key name, KeyBoard symbol, modifier name, and label) needed to hand-edit
/// the file or extend it without cross-referencing `list-keys` or the README. Written
/// as `#`-comments so it's inert HCL and safe to leave in place on re-import.
fn hcl_doc_header(codec: &KeyMappingCodec, layout: &PhysicalKeyboardLayout) -> String {
    let physical_keys: Vec<String> = layout.list_named().into_iter().map(|(name, ..)| name).collect();
    let key_symbols: Vec<String> = codec
        .list_keycode_symbols()
        .into_iter()
        .map(|(_, sym, _)| sym)
        .collect();
    let modifiers: Vec<String> = codec
        .list_modifier_names()
        .into_iter()
        .map(|(_, name)| name.to_string())
        .collect();
    let labels: Vec<String> = codec.list_labels().into_iter().map(|(label, ..)| label).collect();

    format!(
        r#"# rk-a72 HCL keymap (issue #2 schema) — generated by `rk-a72 export-hcl`.
# Re-import with `rk-a72 import-hcl <file>`. This header is a comment; edit freely,
# delete it, or leave it in place — it round-trips either way.
#
# SYNTAX
#   layer "normal" {{
#     mapping "<PhysicalKey>" {{ <attrs> }}
#     ...
#   }}
#   Only "normal", "fn" and "fn2" layers exist; each key is its own repeatable `mapping`
#   block (a duplicate label within one layer is a hard error, not a silent overwrite).
#   On import, mappings merge onto the factory default: slots this file doesn't mention
#   reset to factory, not to whatever the device currently holds. "fn2" has no factory
#   mappings at all — every physical key is unbound on it out of the box.
#
#   IMPORTANT: a nested block's header (`mapping "X" {{`) must start on its own line —
#   HCL rejects `layer "x" {{ mapping "y" {{ ... }} }}` crammed onto one line. Attributes
#   inside a `mapping` block are newline-separated, never comma-separated.
#
#   A `mapping` block's attrs are exactly one of:
#     key = "<KeySymbol>"                          plain KeyBoard key
#     mods = ["<Mod>", ...]  /  key = "<KeySymbol>" KeyBoard key + modifier(s), one per line
#     label = "<Label>"                            non-KeyBoard action (media, special-fn, ...)
#     raw = "0xHHHHHHHH"                            raw 4-byte KeyMatrix slot value
#
# EXAMPLES
#   mapping "Esc" {{ key = "Esc" }}
#   mapping "A"   {{ label = "Mute" }}
#   mapping "W"   {{ raw = "0x02000192" }}
#   mapping "M1" {{
#     mods = ["LCtrl"]
#     key  = "C"
#   }}
#
# MACRO (a recorded key sequence, bindable to a mapping)
#   macro "<Name>" {{
#     repeat = <N>            optional, default 1 — how many times the device replays
#                              the sequence per key press
#     events = [
#       {{ press = "<KeySymbol-or-Mod>" }},
#       {{ release = "<KeySymbol-or-Mod>" }},
#       {{ delay = <ms> }},
#       ...
#     ]
#   }}
#   `press`/`release` take a `KeySymbol` (e.g. "C") or a modifier name or names joined
#   with `+` (e.g. "LCtrl", or "LCtrl+LShift" to press/release several modifiers at
#   once — the device has no single-action "press N modifiers together", so this
#   compiles to one action per modifier, back-to-back with no gap). `type` takes a
#   literal string to type out — see TYPE below. Each event is exactly one of `press`,
#   `release`, `delay`, or `type` — never combined on one line. `delay` accumulates onto
#   the *next* press/release action's timing; a trailing `delay` with nothing after it
#   is dropped. Importing writes the ENTIRE macro table found in the file — every
#   `macro` block, in the order written, becomes the device's full macro list (any
#   macro not redefined in the file is gone after import). A file with no `macro`
#   blocks at all leaves the device's macros untouched, matching `layer`'s "merge onto
#   what's there" behavior — to explicitly wipe every macro, import a file whose only
#   mention of macros is not writing any (there's no separate "clear" syntax; simply
#   omit `macro` blocks, or track a file with none for that purpose). Bind a macro to a
#   key with `mapping "<PhysicalKey>" {{ macro = "<Name>" }}` in a `layer` block, same as
#   any other mapping attribute.
#
# EXAMPLE
#   macro "SelectAll" {{
#     events = [
#       {{ press = "LCtrl" }},
#       {{ press = "A" }},
#       {{ delay = 20 }},
#       {{ release = "A" }},
#       {{ release = "LCtrl" }},
#     ]
#   }}
#   layer "fn" {{
#     mapping "M1" {{ macro = "SelectAll" }}
#   }}
#
# TYPE (a `{{ type = "..." }}` event — types out a literal string)
#   {{ type = "<Text>" }}
#   Shorthand for a whole string of press/release events — no need to spell out every
#   character by hand. Expands (at compile time, before anything is sent to the
#   device) to one press+release pair per character, always US QWERTY regardless of
#   the device's actual layout, with a 1ms delay before every press and release (close
#   enough to a real keystroke's timing that the firmware doesn't drop characters).
#   Uppercase letters and shifted symbols (e.g. "!", "@", "?") are automatically
#   wrapped in a Shift press/release. Supported characters: printable US-ASCII (letters,
#   digits, standard US-layout punctuation) plus `\n` (Enter) and `\t` (Tab) — anything
#   else (accented letters, non-Latin scripts, emoji, other control characters) is a
#   compile-time error naming the character and its position in the string, not a
#   silent skip. A `type` event can be freely mixed with `press`/`release`/`delay`
#   events in the same macro's `events` list.
#
# EXAMPLE
#   macro "Greeting" {{
#     events = [
#       {{ type = "Hello, World!\n" }},
#     ]
#   }}
#
# ENV VAR INTERPOLATION — `${{env("VAR")}}` (keep secrets out of the HCL file)
#   Any string attribute value in this file — not just `type` — can reference the
#   process environment with `${{env("VAR_NAME")}}`, using HCL's own template
#   interpolation syntax (`${{...}}`) and function-call syntax (this is the real HCL
#   language feature, not something specific to this tool). Resolved once, at parse
#   time, against the environment of whatever process ran `rk-a72
#   import-hcl`/`export-hcl` — the value is never written to this file or to the
#   device's macro table in any recoverable form beyond the plain keystrokes a `type`
#   event compiles it to. This is the intended way to put a secret into a macro
#   without ever writing the secret itself into this file: keep the actual secret in
#   a password manager or secrets store, and only the environment-variable NAME in
#   the HCL file.
#     MY_SECRET="$(some-secret-tool show my-entry)" rk-a72 import-hcl file.hcl
#   `env("VAR")` returns "" (never an error) if `VAR` isn't set in the environment —
#   this makes a default value expressible directly in HCL with a ternary:
#     type = "${{env("MY_VAR") != "" ? env("MY_VAR") : "default"}}"
#   To type a literal `${{...}}` without interpolating it, escape the leading `$` by
#   doubling it: `$${{...}}`.
#
#   Reminder: the device's macro table is plain HID-readable memory with no access
#   control — `get-macros`/`export-hcl` can read any macro back out, from this tool or
#   any other program that talks to the keyboard. Only put a secret into a macro if
#   you trust everyone with physical/software access to the keyboard itself.
#
# EXAMPLE
#   macro "Greeting" {{
#     events = [
#       {{ type = "${{env("MY_VAR")}}\n" }},
#     ]
#   }}
#
# THEME (named color aliases, optional)
#   theme {{
#     <name> = "<CSS color>"
#     ...
#   }}
#   Declares reusable color names for `lighting.colors` to reference instead of a
#   literal CSS color — e.g. `alert = "red"` then `"Esc" = "alert"`. Any CSS color
#   syntax is accepted (hex, hsl(), named colors); a `theme` block by itself writes
#   nothing to the device and is never emitted on export — the device has no concept of
#   named colors, only the resolved RGB triple.
#
# LIGHTING (per-key custom colours)
#   lighting {{
#     colors = {{
#       "<PhysicalKey>" = "<CSS color>"
#       ...
#     }}
#   }}
#   Any CSS color syntax works: hex ("ff0000" with a leading hash), "hsl(h, s%, l%)", or
#   a named color like "magenta" — all resolve to the same RGB triple written to the
#   device. A `colors` entry may also name a `theme {{ ... }}` alias instead of a literal
#   color. Physical keys this block doesn't mention are left black (off), not left as
#   whatever the device currently shows — there is no factory-default lighting to merge
#   onto, unlike `layer`. Not every physical key has an individually addressable LED (a
#   few ISO-only and media keys don't); writing a color for one of those is accepted but
#   has no visible effect.
#
# EXAMPLE
#   lighting {{
#     colors = {{
#       "A"   = "hsl(0, 100%, 50%)"
#       "W"   = "hsl(132, 100%, 50%)"
#       "Esc" = "alert"   # theme alias
#     }}
#   }}
#
# PHYSICAL KEYS ({phys_count}) — mapping label / lighting.colors key, e.g. mapping "Esc" {{ ... }}
{phys_list}
#
# KEY SYMBOLS ({sym_count}) — key = "..."
{sym_list}
#
# MODIFIERS ({mod_count}) — mods = [...]
{mod_list}
#
# LABELS ({label_count}) — label = "..."
{label_list}

"#,
        phys_count = physical_keys.len(),
        phys_list = wrap_comment_list(&physical_keys, 90),
        sym_count = key_symbols.len(),
        sym_list = wrap_comment_list(&key_symbols, 90),
        mod_count = modifiers.len(),
        mod_list = wrap_comment_list(&modifiers, 90),
        label_count = labels.len(),
        label_list = wrap_comment_list(&labels, 90),
    )
}

/// Renders one KeyMatrix slot value as the ordered attribute lines for an HCL `mapping`
/// block body — e.g. `["key = \"Esc\""]` or `["mods = [\"LCtrl\"]", "key = \"C\""]` —
/// following key/mod-vs-label-vs-raw precedence: a resolvable macro/label is preferred
/// over a bare `raw` hex value whenever there's enough information to name it.
fn slot_to_hcl_attrs(codec: &KeyMappingCodec, raw: u32, decoded: &crate::codec::DecodedMapping) -> Vec<String> {
    if let crate::codec::DecodedMapping::KeyBoard {
        key_code,
        symbol,
        modifiers,
    } = decoded
    {
        if *key_code != 0 && symbol.is_none() {
            return vec![format!("raw = \"0x{raw:08x}\"")];
        }
        let mut attrs = Vec::new();
        let names = modifiers.active_names();
        if !names.is_empty() {
            let quoted: Vec<String> = names.iter().map(|m| format!("\"{}\"", hcl_escape(m))).collect();
            attrs.push(format!("mods = [{}]", quoted.join(", ")));
        }
        if let Some(s) = symbol {
            attrs.push(format!("key = \"{}\"", hcl_escape(s)));
        }
        return attrs;
    }

    // A resolved macro reference (a name was supplied and the index existed) renders
    // as `macro = "name"`, not a raw hex value — mirrors how KeyBoard prefers `key =`
    // over `raw =` whenever it has enough information to. An unresolved macro index
    // (no name for it, e.g. it's beyond what GetMacros returned) falls through to the
    // label/raw branch below, same as today.
    if let crate::codec::DecodedMapping::Macro { name: Some(name), .. } = decoded {
        return vec![format!("macro = \"{}\"", hcl_escape(name))];
    }

    let label = decoded.label();
    if codec.has_label(&label) {
        return vec![format!("label = \"{}\"", hcl_escape(&label))];
    }

    vec![format!("raw = \"0x{raw:08x}\"")]
}

/// Renders the `layer` KeyMatrix state, via [`Self::dump`]/[`Self::dump_diff`], and the
/// `lighting.colors` map, via [`Self::dump_lighting`], as HCL — the export-side
/// counterpart to [`HclConfig::layer_slot_maps`] and [`HclConfig::compile_lighting`].
/// Never emits `theme`/`macro`, since the device holds nothing for those sections to
/// round-trip from.
pub struct HclExporter {
    codec: KeyMappingCodec,
    layout: PhysicalKeyboardLayout,
}

impl HclExporter {
    pub fn new(codec: KeyMappingCodec, layout: PhysicalKeyboardLayout) -> Self {
        Self { codec, layout }
    }

    /// Dumps every populated slot (`raw != 0`) in both layers as HCL, regardless of
    /// whether it matches the factory default. Use [`Self::dump_diff`] for the
    /// compact, default-relative form.
    pub fn dump(&self, buffers_by_layer: &HashMap<u8, Vec<u8>>, macro_names: &[String]) -> String {
        self.dump_impl(buffers_by_layer, None, macro_names)
    }

    /// Dumps only slots whose raw value differs from `baseline` — the compact export
    /// form. Compares raw bytes, not decoded labels, so a difference invisible in the
    /// label (e.g. a Macro's repeat count) is still surfaced.
    pub fn dump_diff(
        &self,
        buffers_by_layer: &HashMap<u8, Vec<u8>>,
        baseline: &HashMap<u8, Vec<u8>>,
        macro_names: &[String],
    ) -> String {
        self.dump_impl(buffers_by_layer, Some(baseline), macro_names)
    }

    /// Renders a single slot's current mapping as one `mapping "<Name>" { <attrs> }`
    /// block — the shape `import-hcl` round-trips — for callers (e.g. `get-keymap`) that
    /// want to show one key without producing a whole document. `macro_names` resolves a
    /// macro-index slot to `macro = "<name>"` when the name is known.
    pub fn describe_slot(&self, name: &str, raw: u32, macro_names: &[String]) -> String {
        let decoded = self.codec.decode(raw, Some(macro_names));
        let attrs = slot_to_hcl_attrs(&self.codec, raw, &decoded);
        let mut block = format!("mapping \"{}\" {{\n", hcl_escape(name));
        for attr in &attrs {
            block.push_str(&format!("  {attr}\n"));
        }
        block.push('}');
        block
    }

    /// Renders a `lighting { colors = {...} } ` block from a raw 378-byte planar
    /// `SetLedColors` buffer (as returned by `LedColorRepository::read_colors`). Every
    /// slot whose RGB is non-zero is emitted as `"KeyName" = "#rrggbb"`; all-zero slots
    /// (unlit or no physical LED) are omitted — there's no separate baseline to diff
    /// against, since black is both "off" and the implicit default for slots the config
    /// doesn't mention (see `HclConfig::compile_lighting`). Returns an empty string if
    /// every slot is black — the block's syntax is documented once in
    /// [`hcl_doc_header`] regardless, so that documentation survives even when there's
    /// nothing to export.
    pub fn dump_lighting(&self, colors: &[u8]) -> String {
        let mut mappings = Vec::new();
        for slot in 0..crate::protocol::LED_COLORS_SLOT_COUNT as u16 {
            let i = slot as usize;
            let (r, g, b) = (
                colors[i],
                colors[i + crate::protocol::LED_COLORS_SLOT_COUNT],
                colors[i + crate::protocol::LED_COLORS_SLOT_COUNT * 2],
            );
            if r == 0 && g == 0 && b == 0 {
                continue;
            }
            let name = self.layout.name_for_slot(slot);
            mappings.push(format!(
                "    \"{}\" = \"#{r:02x}{g:02x}{b:02x}\"",
                hcl_escape(&name)
            ));
        }
        if mappings.is_empty() {
            return String::new();
        }
        format!("lighting {{\n  colors = {{\n{}\n  }}\n}}\n\n", mappings.join("\n"))
    }

    /// Renders `macro "name" { repeat = N events = [...] }` blocks from a decoded
    /// macro table (as returned by `MacroRepository::read_macros`) — the export-side
    /// counterpart to `HclConfig::compile_macros`. Every wire `NormalKey`/`ModifyKey`
    /// action becomes a press/release event by its HID keycode; delay is folded back
    /// into a standalone `{ delay = N }` event immediately before the action it was
    /// attached to on the wire (the inverse of `compile_one_macro`'s folding). `repeat`
    /// is not carried here — the wire macro table has no repeat field of its own; it
    /// only exists on the layer-slot side (`KeyMappingType::Macro`'s keyCode high
    /// byte), so it's rendered as `repeat = 1` and left for the user to adjust per
    /// `mapping` reference if a real device round-trip ever needs it distinguished.
    pub fn dump_macros(&self, macros: &[crate::macros::Macro]) -> String {
        if macros.is_empty() {
            return String::new();
        }
        let mut out = String::new();
        for m in macros {
            out.push_str(&format!("macro \"{}\" {{\n  events = [\n", hcl_escape(&m.name)));
            for action in &m.actions {
                if action.delay > 0 {
                    out.push_str(&format!("    {{ delay = {} }},\n", action.delay));
                }
                let symbol = match action.kind {
                    crate::macros::MacroActionKind::NormalKey => self
                        .codec
                        .keycode_symbol(action.key as u16)
                        .unwrap_or_else(|| format!("key({})", action.key)),
                    crate::macros::MacroActionKind::ModifyKey => ModifierSet::from_hid_usage_code(action.key)
                        .and_then(|m| m.to_label())
                        .unwrap_or_else(|| format!("raw({})", action.key)),
                    _ => format!("raw({})", action.key),
                };
                let verb = match action.edge {
                    crate::macros::MacroEdge::Down => "press",
                    crate::macros::MacroEdge::Up => "release",
                };
                out.push_str(&format!("    {{ {verb} = \"{}\" }},\n", hcl_escape(&symbol)));
            }
            out.push_str("  ]\n}\n\n");
        }
        out
    }

    fn dump_impl(
        &self,
        buffers_by_layer: &HashMap<u8, Vec<u8>>,
        baseline: Option<&HashMap<u8, Vec<u8>>>,
        macro_names: &[String],
    ) -> String {
        let mut out = hcl_doc_header(&self.codec, &self.layout);
        for &layer in &[LAYER_NORMAL, LAYER_FN, LAYER_FN2] {
            let Some(buf) = buffers_by_layer.get(&layer) else {
                continue;
            };
            let mut mappings = Vec::new();
            for slot in 0..crate::protocol::KEYMATRIX_SLOT_COUNT as u16 {
                let offset = slot as usize * 4;
                let value = u32::from_be_bytes(buf[offset..offset + 4].try_into().unwrap());
                let skip_value = baseline
                    .and_then(|b| b.get(&layer))
                    .map(|b| u32::from_be_bytes(b[offset..offset + 4].try_into().unwrap()))
                    .unwrap_or(0);
                if value == skip_value {
                    continue;
                }
                let decoded = self.codec.decode(value, Some(macro_names));
                let name = self.layout.name_for_slot(slot);
                let attrs = slot_to_hcl_attrs(&self.codec, value, &decoded);
                let mut block = format!("  mapping \"{}\" {{\n", hcl_escape(&name));
                for attr in &attrs {
                    block.push_str(&format!("    {attr}\n"));
                }
                block.push_str("  }");
                mappings.push(block);
            }
            if mappings.is_empty() {
                continue;
            }
            let layer_name = match layer {
                LAYER_NORMAL => "normal",
                LAYER_FN => "fn",
                LAYER_FN2 => "fn2",
                _ => unreachable!("only layers 0, 1 and 2 exist on the A72"),
            };
            out.push_str(&format!("layer \"{layer_name}\" {{\n"));
            out.push_str(&mappings.join("\n"));
            out.push_str("\n}\n\n");
        }
        out
    }
}

fn resolve_macro_key(context: &str, symbol: &str, codec: &KeyMappingCodec) -> Result<MacroKey, KeymapError> {
    if let Some(code) = codec.symbol_to_keycode(symbol) {
        return Ok(MacroKey::Key(code));
    }
    // A modifier pressed on its own (LCtrl/LShift/…) is a valid macro step but is not in
    // the KeyBoard keycode table, so fall back to the modifier names.
    if let Ok(m) = ModifierSet::from_label(symbol) {
        return Ok(MacroKey::Modifier(m));
    }
    Err(KeymapError::HclValidation {
        context: context.to_string(),
        detail: format!(
            "\"{symbol}\" is neither a KeyBoard key symbol nor a modifier name (see \"list-keys\")"
        ),
    })
}

/// Expands a `{ type = "..." }` macro event into a `Press`/`Release` (and, for
/// shifted characters, wrapping `Press`/`Release(LShift)`) sequence, one `Delay(1)`
/// before every press and release — matching a real keystroke's timing closely enough
/// that the firmware doesn't drop characters (confirmed on real hardware). US QWERTY
/// only; see `ascii_char_to_key` for the supported character set.
/// The `env("VAR")` HCL function: the named environment variable's value, or `""` if
/// it's unset — deliberately never an error, so `env("X") != "" ? env("X") : "default"`
/// is expressible directly in HCL without a separate "is this var set" primitive.
fn env_func(args: hcl::eval::FuncArgs) -> Result<hcl::Value, String> {
    let name = args[0].as_str().expect("ParamType::String guarantees a string argument");
    Ok(hcl::Value::from(std::env::var(name).unwrap_or_default()))
}

fn ascii_text_to_events(context: &str, text: &str) -> Result<Vec<MacroEvent>, KeymapError> {
    let mut events = Vec::new();
    for (i, ch) in text.chars().enumerate() {
        let (symbol, needs_shift) = ascii_char_to_key(ch).ok_or_else(|| KeymapError::HclValidation {
            context: context.to_string(),
            detail: format!(
                "type: character {:?} at position {i} is not in the supported set (printable \
                 US-ASCII, plus \\n and \\t)",
                ch
            ),
        })?;
        let key = MacroKey::Key(
            KeyMappingCodec::new()
                .symbol_to_keycode(symbol)
                .unwrap_or_else(|| panic!("ascii_char_to_key returned unknown KeySymbol {symbol:?}")),
        );
        if needs_shift {
            events.push(MacroEvent::Delay(1));
            events.push(MacroEvent::Press(MacroKey::Modifier(ModifierSet::L_SHIFT)));
        }
        events.push(MacroEvent::Delay(1));
        events.push(MacroEvent::Press(key.clone()));
        events.push(MacroEvent::Delay(1));
        events.push(MacroEvent::Release(key));
        if needs_shift {
            events.push(MacroEvent::Delay(1));
            events.push(MacroEvent::Release(MacroKey::Modifier(ModifierSet::L_SHIFT)));
        }
    }
    Ok(events)
}

/// US QWERTY only: the `KeySymbol` (as recognized by `KeyMappingCodec::symbol_to_keycode`)
/// and whether Shift is needed to produce `ch`. `None` for anything outside printable
/// US-ASCII plus `\n`/`\t`.
fn ascii_char_to_key(ch: char) -> Option<(&'static str, bool)> {
    Some(match ch {
        'a'..='z' => (ascii_letter_symbol(ch.to_ascii_uppercase()), false),
        'A'..='Z' => (ascii_letter_symbol(ch), true),
        '0' => ("0", false),
        '1'..='9' => (ascii_digit_symbol(ch), false),
        ' ' => ("Space", false),
        '\n' => ("Enter", false),
        '\t' => ("Tab", false),
        '`' => ("Backtick", false),
        '~' => ("Backtick", true),
        '-' => ("Minus", false),
        '_' => ("Minus", true),
        '=' => ("=", false),
        '+' => ("=", true),
        '[' => ("[", false),
        '{' => ("[", true),
        ']' => ("]", false),
        '}' => ("]", true),
        '\\' => ("Backslash", false),
        '|' => ("Backslash", true),
        ';' => ("Semicolon", false),
        ':' => ("Semicolon", true),
        '\'' => ("Quote", false),
        '"' => ("Quote", true),
        ',' => (",", false),
        '<' => (",", true),
        '.' => (".", false),
        '>' => (".", true),
        '/' => ("/", false),
        '?' => ("/", true),
        '!' => ("1", true),
        '@' => ("2", true),
        '#' => ("3", true),
        '$' => ("4", true),
        '%' => ("5", true),
        '^' => ("6", true),
        '&' => ("7", true),
        '*' => ("8", true),
        '(' => ("9", true),
        ')' => ("0", true),
        _ => return None,
    })
}

fn ascii_letter_symbol(upper: char) -> &'static str {
    match upper {
        'A' => "A", 'B' => "B", 'C' => "C", 'D' => "D", 'E' => "E", 'F' => "F", 'G' => "G",
        'H' => "H", 'I' => "I", 'J' => "J", 'K' => "K", 'L' => "L", 'M' => "M", 'N' => "N",
        'O' => "O", 'P' => "P", 'Q' => "Q", 'R' => "R", 'S' => "S", 'T' => "T", 'U' => "U",
        'V' => "V", 'W' => "W", 'X' => "X", 'Y' => "Y", 'Z' => "Z",
        _ => unreachable!("ascii_char_to_key only calls this with an uppercase ASCII letter"),
    }
}

fn ascii_digit_symbol(digit: char) -> &'static str {
    match digit {
        '1' => "1", '2' => "2", '3' => "3", '4' => "4", '5' => "5",
        '6' => "6", '7' => "7", '8' => "8", '9' => "9",
        _ => unreachable!("ascii_char_to_key only calls this with an ASCII digit 1-9"),
    }
}

fn validate_macros(
    raw: &IndexMap<String, RawMacro>,
    codec: &KeyMappingCodec,
) -> Result<Vec<MacroDef>, KeymapError> {
    let mut out = Vec::with_capacity(raw.len());
    for (name, def) in raw {
        // The wire format's macro-name-length header is a single byte holding the
        // UTF-16LE byte count (not the character count) — max 255, i.e. at most 127
        // UTF-16 code units. `Macro::serialize` doesn't check this (it just truncates
        // silently via `as u8`), so it must be rejected here, before anything is ever
        // sent to the device.
        let name_utf16_bytes = name.encode_utf16().count() * 2;
        if name_utf16_bytes > 255 {
            return Err(KeymapError::HclValidation {
                context: format!("macro \"{name}\""),
                detail: format!(
                    "name is {name_utf16_bytes} bytes encoded as UTF-16LE, but the device's macro \
                     name-length header is a single byte (max 255 bytes / 127 UTF-16 code units)"
                ),
            });
        }
        let mut events = Vec::with_capacity(def.events.len());
        for (i, ev) in def.events.iter().enumerate() {
            let context = format!("macro \"{name}\" event {i}");
            let set = [
                ev.press.is_some(),
                ev.release.is_some(),
                ev.delay.is_some(),
                ev.type_text.is_some(),
            ];
            match set.iter().filter(|&&x| x).count() {
                1 => {}
                0 => {
                    return Err(KeymapError::HclValidation {
                        context,
                        detail: "empty event — expected exactly one of press/release/delay/type".into(),
                    })
                }
                _ => {
                    return Err(KeymapError::HclValidation {
                        context,
                        detail: "event has more than one of press/release/delay/type".into(),
                    })
                }
            }
            if let Some(text) = &ev.type_text {
                events.extend(ascii_text_to_events(&context, text)?);
                continue;
            }
            let event = if let Some(sym) = &ev.press {
                MacroEvent::Press(resolve_macro_key(&context, sym, codec)?)
            } else if let Some(sym) = &ev.release {
                MacroEvent::Release(resolve_macro_key(&context, sym, codec)?)
            } else {
                MacroEvent::Delay(ev.delay.expect("exactly-one check guarantees delay"))
            };
            events.push(event);
        }
        out.push(MacroDef {
            name: name.clone(),
            repeat: def.repeat,
            events,
        });
    }
    Ok(out)
}

/// Turns one validated `MacroDef` (press/release/delay events, expressed in terms of
/// key/modifier symbols) into a `macros::Macro` (down/up actions with numeric
/// type/key/delay), ready for `encode_macro_table`. A `Press`/`Release` of a
/// `MacroKey::Key(code)` becomes one `NormalKey` action. A `Press`/`Release` of a
/// `MacroKey::Modifier(set)` becomes one `ModifyKey` action *per bit* in `set` (in
/// canonical `ModifierSet` order) — the wire format has no "press N modifiers at once"
/// action, so `{ press = "LCtrl+LShift" }` compiles to two consecutive Down actions
/// (LCtrl then LShift) with no gap between them, and the matching release compiles to
/// two consecutive Up actions the same way; a single-modifier press/release is just the
/// N=1 case of this. The wire format also has no standalone delay action, so a
/// `Delay(ms)` event attaches as the `delay` field of the *next* action instead —
/// consecutive `Delay` events accumulate onto that same next action (only the first
/// action of a multi-bit modifier step gets the accumulated delay; the rest of that
/// step's actions get 0, since they represent one instantaneous key combination, not N
/// separately-timed presses), and a trailing `Delay` with no following key/modifier
/// event has nothing to attach to and is silently dropped (HCL validation doesn't
/// currently reject a macro ending in a bare delay; revisit if that turns out to
/// surprise users in practice).
fn compile_one_macro(def: &MacroDef) -> crate::macros::Macro {
    use crate::macros::{MacroAction, MacroActionKind, MacroEdge};

    let mut actions = Vec::new();
    let mut pending_delay: u32 = 0;
    for event in &def.events {
        match event {
            MacroEvent::Delay(ms) => {
                pending_delay += ms;
            }
            MacroEvent::Press(key) | MacroEvent::Release(key) => {
                let edge = if matches!(event, MacroEvent::Press(_)) {
                    MacroEdge::Down
                } else {
                    MacroEdge::Up
                };
                match key {
                    MacroKey::Key(code) => {
                        actions.push(MacroAction {
                            edge,
                            kind: MacroActionKind::NormalKey,
                            delay: pending_delay,
                            key: (*code & 0xff) as u8,
                        });
                        pending_delay = 0;
                    }
                    MacroKey::Modifier(set) => {
                        for bit in set.active_bits() {
                            let usage_code = bit
                                .to_hid_usage_code()
                                .expect("active_bits() always yields single-bit sets");
                            actions.push(MacroAction {
                                edge,
                                kind: MacroActionKind::ModifyKey,
                                delay: pending_delay,
                                key: usage_code,
                            });
                            pending_delay = 0;
                        }
                    }
                }
            }
        }
    }
    crate::macros::Macro {
        name: def.name.clone(),
        actions,
    }
}

fn validate_layers(
    raw: &IndexMap<String, RawLayer>,
    macros: &IndexMap<String, RawMacro>,
    codec: &KeyMappingCodec,
    layout: &PhysicalKeyboardLayout,
) -> Result<HashMap<u8, IndexMap<u16, LayerAction>>, KeymapError> {
    let mut out: HashMap<u8, IndexMap<u16, LayerAction>> = HashMap::new();
    for (layer_name, layer) in raw {
        let layer_num = layer_number(layer_name)
            .ok_or_else(|| KeymapError::HclUnknownLayer(layer_name.clone()))?;
        let slots = out.entry(layer_num).or_default();
        for (key_name, action) in &layer.mappings {
            let slot = layout.slot_for_name(key_name).ok_or_else(|| KeymapError::HclValidation {
                context: format!("layer \"{layer_name}\""),
                detail: format!(
                    "unknown physical key \"{key_name}\" — run \"list-keys\" to see valid names"
                ),
            })?;
            let context = format!("layer \"{layer_name}\" key \"{key_name}\"");
            let resolved = resolve_action(&context, action, macros, codec)?;
            slots.insert(slot, resolved);
        }
    }
    Ok(out)
}

fn resolve_action(
    context: &str,
    action: &RawAction,
    macros: &IndexMap<String, RawMacro>,
    codec: &KeyMappingCodec,
) -> Result<LayerAction, KeymapError> {
    // Precedence: raw > label > macro > key. Exactly one form
    // must be present.
    let forms = [
        action.raw.is_some(),
        action.label.is_some(),
        action.macro_ref.is_some(),
        action.key.is_some(),
    ];
    match forms.iter().filter(|&&x| x).count() {
        1 => {}
        0 => {
            return Err(KeymapError::HclValidation {
                context: context.to_string(),
                detail: "empty action — expected one of key/label/macro/raw".into(),
            })
        }
        _ => {
            return Err(KeymapError::HclValidation {
                context: context.to_string(),
                detail: "action has more than one of key/label/macro/raw".into(),
            })
        }
    }

    if let Some(raw) = &action.raw {
        let value = u32::from_str_radix(raw.trim_start_matches("0x"), 16).map_err(|_| {
            KeymapError::HclValidation {
                context: context.to_string(),
                detail: format!("invalid raw value \"{raw}\""),
            }
        })?;
        return Ok(LayerAction::Raw(value));
    }

    if let Some(label) = &action.label {
        // A layer action never carries `mods` alongside a non-KeyBoard label.
        if action.mods.is_some() {
            return Err(KeymapError::HclValidation {
                context: context.to_string(),
                detail: "mods are only valid with key, not label".into(),
            });
        }
        let raw = codec.label_to_raw(label).ok_or_else(|| KeymapError::HclValidation {
            context: context.to_string(),
            detail: format!("unknown label \"{label}\" — run \"list-keys\" to see valid labels"),
        })?;
        return Ok(LayerAction::Label(raw));
    }

    if let Some(name) = &action.macro_ref {
        if !macros.contains_key(name) {
            return Err(KeymapError::HclValidation {
                context: context.to_string(),
                detail: format!("macro \"{name}\" is not defined in this file"),
            });
        }
        return Ok(LayerAction::Macro(name.clone()));
    }

    // key (+ optional mods)
    let key = action.key.as_ref().expect("exactly-one check guarantees key");
    let key_code = codec.symbol_to_keycode(key).ok_or_else(|| KeymapError::HclValidation {
        context: context.to_string(),
        detail: format!("unknown KeyBoard key \"{key}\" — run \"list-keys\" to see valid symbols"),
    })?;
    let mut modifiers = ModifierSet::empty();
    if let Some(mods) = &action.mods {
        for m in mods {
            let bit = ModifierSet::from_label(m).map_err(|_| KeymapError::HclValidation {
                context: context.to_string(),
                detail: format!("unknown modifier \"{m}\" — run \"list-keys\" to see valid names"),
            })?;
            modifiers |= bit;
        }
    }
    Ok(LayerAction::Keyboard { key_code, modifiers })
}

fn validate_lighting(
    raw: &RawLighting,
    theme: &IndexMap<String, [u8; 3]>,
    layout: &PhysicalKeyboardLayout,
) -> Result<LightingConfig, KeymapError> {
    let brightness = match raw.brightness {
        Some(b) if b <= 100 => Some(b as u8),
        Some(b) => {
            return Err(KeymapError::HclValidation {
                context: "lighting".into(),
                detail: format!("brightness {b} out of range (expected 0..=100)"),
            })
        }
        None => None,
    };
    let speed = match raw.speed {
        Some(s) if s <= 255 => Some(s as u8),
        Some(s) => {
            return Err(KeymapError::HclValidation {
                context: "lighting".into(),
                detail: format!("speed {s} out of range (expected 0..=255)"),
            })
        }
        None => None,
    };

    let mut colors = Vec::with_capacity(raw.colors.len());
    for (key_name, value) in &raw.colors {
        let slot = layout.slot_for_name(key_name).ok_or_else(|| KeymapError::HclValidation {
            context: "lighting.colors".into(),
            detail: format!(
                "unknown physical key \"{key_name}\" — run \"list-keys\" to see valid names"
            ),
        })?;
        // A color is either a theme alias or a direct CSS color; alias wins.
        let rgb = match theme.get(value) {
            Some(&rgb) => rgb,
            None => parse_css_color(&format!("lighting.colors \"{key_name}\""), value)?,
        };
        colors.push(KeyColor {
            key: key_name.clone(),
            slot,
            rgb,
        });
    }

    Ok(LightingConfig {
        effect: raw.effect.clone(),
        brightness,
        speed,
        colors,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---- layer compilation (the flashable path) ----
    //
    // v2 schema (issue #2 comment): each key mapping is its own repeatable
    // `mapping "<PhysicalKey>" { ... }` block — NOT a v1 `mappings = { "<Key>" = {...} }`
    // object attribute. No back-compat: v1-style files are rejected (see
    // `v1_mappings_attribute_syntax_is_no_longer_accepted` below). Note HCL's own
    // grammar constraint this exercises throughout: a nested block's header must start
    // on its own line (`layer "x" { mapping "y" { ... } }` all on one line is a parse
    // error), and attributes inside one block are newline- not comma-separated.

    #[test]
    fn layer_plain_key_compiles_to_a_keyboard_slot_value() {
        let cfg = HclConfig::parse(
            r#"
layer "normal" {
  mapping "Esc" {
    key = "Esc"
  }
}
"#,
        )
        .unwrap();
        let maps = cfg.layer_slot_maps().unwrap();
        let esc_slot = PhysicalKeyboardLayout::new().slot_for_name("Esc").unwrap();
        let esc_code = KeyMappingCodec::new().symbol_to_keycode("Esc").unwrap();
        assert_eq!(
            maps[&LAYER_NORMAL][&esc_slot],
            KeyMappingCodec::encode_keyboard(esc_code, ModifierSet::empty())
        );
        assert!(maps[&LAYER_FN].is_empty());
    }

    #[test]
    fn layer_key_with_mods_array_compiles_with_all_modifier_bits() {
        let cfg = HclConfig::parse(
            r#"
layer "fn" {
  mapping "M1" {
    mods = ["LCtrl", "LShift"]
    key  = "C"
  }
}
"#,
        )
        .unwrap();
        let maps = cfg.layer_slot_maps().unwrap();
        let slot = PhysicalKeyboardLayout::new().slot_for_name("M1").unwrap();
        let c = KeyMappingCodec::new().symbol_to_keycode("C").unwrap();
        assert_eq!(
            maps[&LAYER_FN][&slot],
            KeyMappingCodec::encode_keyboard(c, ModifierSet::L_CTRL | ModifierSet::L_SHIFT)
        );
    }

    #[test]
    fn layer_fn2_compiles_to_a_keyboard_slot_value() {
        let cfg = HclConfig::parse(
            r#"
layer "fn2" {
  mapping "S" {
    key = "Backspace"
  }
}
"#,
        )
        .unwrap();
        let maps = cfg.layer_slot_maps().unwrap();
        let s_slot = PhysicalKeyboardLayout::new().slot_for_name("S").unwrap();
        let backspace_code = KeyMappingCodec::new().symbol_to_keycode("Backspace").unwrap();
        assert_eq!(
            maps[&LAYER_FN2][&s_slot],
            KeyMappingCodec::encode_keyboard(backspace_code, ModifierSet::empty())
        );
        assert!(maps[&LAYER_NORMAL].is_empty());
        assert!(maps[&LAYER_FN].is_empty());
    }

    #[test]
    fn layer_label_and_raw_actions_compile() {
        let cfg = HclConfig::parse(
            r#"
layer "normal" {
  mapping "Esc"   { label = "Mute" }
  mapping "Enter" { raw = "0x02000192" }
}
"#,
        )
        .unwrap();
        let maps = cfg.layer_slot_maps().unwrap();
        let layout = PhysicalKeyboardLayout::new();
        assert_eq!(
            maps[&LAYER_NORMAL][&layout.slot_for_name("Esc").unwrap()],
            0x020000e2
        );
        assert_eq!(
            maps[&LAYER_NORMAL][&layout.slot_for_name("Enter").unwrap()],
            0x02000192
        );
    }

    #[test]
    fn unknown_layer_name_is_rejected() {
        let err = HclConfig::parse(
            r#"
layer "gaming" {
  mapping "Esc" { key = "Esc" }
}
"#,
        )
        .unwrap_err();
        assert!(matches!(err, KeymapError::HclUnknownLayer(n) if n == "gaming"));
    }

    #[test]
    fn unknown_physical_key_is_rejected() {
        let err = HclConfig::parse(
            r#"
layer "normal" {
  mapping "NotAKey" { key = "Esc" }
}
"#,
        )
        .unwrap_err();
        assert!(matches!(err, KeymapError::HclValidation { detail, .. } if detail.contains("NotAKey")));
    }

    #[test]
    fn unknown_keyboard_symbol_is_rejected() {
        let err = HclConfig::parse(
            r#"
layer "normal" {
  mapping "Esc" { key = "NotAKey" }
}
"#,
        )
        .unwrap_err();
        assert!(matches!(err, KeymapError::HclValidation { detail, .. } if detail.contains("NotAKey")));
    }

    #[test]
    fn action_with_more_than_one_form_is_rejected() {
        let err = HclConfig::parse(
            r#"
layer "normal" {
  mapping "Esc" {
    key   = "A"
    label = "Mute"
  }
}
"#,
        )
        .unwrap_err();
        assert!(matches!(err, KeymapError::HclValidation { detail, .. } if detail.contains("more than one")));
    }

    #[test]
    fn empty_action_is_rejected() {
        let err = HclConfig::parse(
            r#"
layer "normal" {
  mapping "Esc" {}
}
"#,
        )
        .unwrap_err();
        assert!(matches!(err, KeymapError::HclValidation { detail, .. } if detail.contains("empty action")));
    }

    #[test]
    fn unknown_action_key_is_rejected_by_deny_unknown_fields() {
        let err = HclConfig::parse(
            r#"
layer "normal" {
  mapping "Esc" {
    kye = "A"
  }
}
"#,
        )
        .unwrap_err();
        // deny_unknown_fields surfaces through hcl's serde error path.
        assert!(matches!(err, KeymapError::Hcl(_)));
    }

    #[test]
    fn duplicate_mapping_label_in_the_same_layer_is_rejected_with_a_clear_error() {
        let err = HclConfig::parse(
            r#"
layer "fn" {
  mapping "Esc" { key = "A" }
  mapping "Esc" { key = "B" }
}
"#,
        )
        .unwrap_err();
        assert!(
            matches!(err, KeymapError::HclValidation { context, detail } if context == "layer \"fn\"" && detail.contains("duplicate mapping \"Esc\""))
        );
    }

    #[test]
    fn duplicate_mapping_label_check_does_not_false_positive_across_different_layers() {
        // Same key name in "normal" and "fn" is not a duplicate — only same-layer,
        // same-label mapping blocks are.
        let cfg = HclConfig::parse(
            r#"
layer "normal" {
  mapping "Esc" { key = "A" }
}
layer "fn" {
  mapping "Esc" { key = "B" }
}
"#,
        )
        .unwrap();
        let maps = cfg.layer_slot_maps().unwrap();
        let esc_slot = PhysicalKeyboardLayout::new().slot_for_name("Esc").unwrap();
        assert_ne!(maps[&LAYER_NORMAL][&esc_slot], maps[&LAYER_FN][&esc_slot]);
    }

    #[test]
    fn v1_mappings_attribute_syntax_is_no_longer_accepted() {
        // No backward compatibility with the v1 `mappings = { "Esc" = {...} }` object
        // attribute — v2 requires per-key `mapping "Esc" { ... }` blocks. RawLayer's
        // deny_unknown_fields turns the old attribute into a clear rejection rather than
        // silently parsing to an empty layer.
        let err = HclConfig::parse(
            r#"
layer "normal" {
  mappings = { "Esc" = { key = "A" } }
}
"#,
        )
        .unwrap_err();
        assert!(matches!(err, KeymapError::Hcl(_)));
    }

    // ---- macro sequencing (parsed + validated, not flashable) ----

    #[test]
    fn macro_events_preserve_order_and_resolve_keys_and_modifiers() {
        let cfg = HclConfig::parse(
            r#"
macro "copy" {
  events = [
    { press = "LCtrl" },
    { press = "C" },
    { delay = 50 },
    { release = "C" },
    { release = "LCtrl" },
  ]
}
"#,
        )
        .unwrap();
        let m = &cfg.macros()[0];
        assert_eq!(m.name, "copy");
        let c = KeyMappingCodec::new().symbol_to_keycode("C").unwrap();
        assert_eq!(
            m.events,
            vec![
                MacroEvent::Press(MacroKey::Modifier(ModifierSet::L_CTRL)),
                MacroEvent::Press(MacroKey::Key(c)),
                MacroEvent::Delay(50),
                MacroEvent::Release(MacroKey::Key(c)),
                MacroEvent::Release(MacroKey::Modifier(ModifierSet::L_CTRL)),
            ]
        );
    }

    #[test]
    fn macro_event_with_two_fields_is_rejected() {
        let err = HclConfig::parse(
            r#"
macro "x" {
  events = [ { press = "C", delay = 10 } ]
}
"#,
        )
        .unwrap_err();
        assert!(matches!(err, KeymapError::HclValidation { detail, .. } if detail.contains("more than one")));
    }

    #[test]
    fn macro_event_with_unknown_symbol_is_rejected() {
        let err = HclConfig::parse(
            r#"
macro "x" {
  events = [ { press = "NopeKey" } ]
}
"#,
        )
        .unwrap_err();
        assert!(matches!(err, KeymapError::HclValidation { detail, .. } if detail.contains("NopeKey")));
    }

    #[test]
    fn compile_macros_encodes_a_real_macro_table() {
        let cfg = HclConfig::parse(
            r#"
macro "copy" {
  events = [
    { press = "LCtrl" },
    { press = "C" },
    { delay = 50 },
    { release = "C" },
    { release = "LCtrl" },
  ]
}
"#,
        )
        .unwrap();
        let encoded = cfg.compile_macros().unwrap();

        // Round-trips through the macros.rs decoder: one macro, named "copy". 5 HCL
        // events compile to 4 wire actions — the { delay = 50 } event has no action of
        // its own; it folds onto the next action (release "C") as its delay field.
        let decoded = crate::macros::decode_macro_table(&encoded);
        assert_eq!(decoded.len(), 1);
        assert_eq!(decoded[0].name, "copy");
        assert_eq!(decoded[0].actions.len(), 4);

        // The LCtrl press/release actions must carry the USB HID modifier usage code
        // (224 = LeftControl), NOT the ModifierSet KeyMatrix bitmask (LCtrl = 1) — the
        // real captured device data (macros-capture.md) confirms 224 is what the
        // firmware expects in a ModifyKey action's key byte. A prior version of this
        // encoder used `.bits()` (1) here, which is silently ignored by the firmware.
        use crate::macros::MacroActionKind;
        assert_eq!(decoded[0].actions[0].kind, MacroActionKind::ModifyKey);
        assert_eq!(decoded[0].actions[0].key, 224, "LCtrl press must encode as HID usage code 224, not bitmask 1");
        assert_eq!(decoded[0].actions[3].kind, MacroActionKind::ModifyKey);
        assert_eq!(decoded[0].actions[3].key, 224, "LCtrl release must encode as HID usage code 224, not bitmask 1");
    }

    #[test]
    fn a_multi_modifier_macro_press_compiles_to_one_modifykey_action_per_bit() {
        // `press = "LCtrl+LShift"` presses both modifiers at once — the wire format has
        // no single action for that, so it must expand to two consecutive ModifyKey Down
        // actions (one per bit, in canonical order), and the release the mirror pair of
        // Up actions. This used to panic (`.expect("...exactly one bit")`) instead.
        let cfg = HclConfig::parse(
            r#"
macro "combo" {
  events = [
    { press = "LCtrl+LShift" },
    { press = "C" },
    { release = "C" },
    { release = "LCtrl+LShift" },
  ]
}
"#,
        )
        .unwrap();
        let encoded = cfg.compile_macros().unwrap();
        let decoded = crate::macros::decode_macro_table(&encoded);
        assert_eq!(decoded.len(), 1);

        use crate::macros::{MacroActionKind, MacroEdge};
        let actions = &decoded[0].actions;
        assert_eq!(actions.len(), 6, "2 (press LCtrl,LShift) + 1 (press C) + 1 (release C) + 2 (release LCtrl,LShift)");
        assert_eq!(actions[0], crate::macros::MacroAction { edge: MacroEdge::Down, kind: MacroActionKind::ModifyKey, delay: 0, key: 224 }, "LCtrl press (usage code 224)");
        assert_eq!(actions[1], crate::macros::MacroAction { edge: MacroEdge::Down, kind: MacroActionKind::ModifyKey, delay: 0, key: 225 }, "LShift press (usage code 225)");
        assert_eq!(actions[4], crate::macros::MacroAction { edge: MacroEdge::Up, kind: MacroActionKind::ModifyKey, delay: 0, key: 224 }, "LCtrl release (usage code 224)");
        assert_eq!(actions[5], crate::macros::MacroAction { edge: MacroEdge::Up, kind: MacroActionKind::ModifyKey, delay: 0, key: 225 }, "LShift release (usage code 225)");
    }

    // ---- `type = "..."` pseudo-command: expands to press/release events ----

    #[test]
    fn type_event_with_a_lowercase_letter_compiles_to_a_press_release_pair_with_1ms_delays() {
        let cfg = HclConfig::parse(r#"macro "x" { events = [ { type = "a" } ] }"#).unwrap();
        let a_code = KeyMappingCodec::new().symbol_to_keycode("A").unwrap();
        assert_eq!(
            cfg.macros()[0].events,
            vec![
                MacroEvent::Delay(1),
                MacroEvent::Press(MacroKey::Key(a_code)),
                MacroEvent::Delay(1),
                MacroEvent::Release(MacroKey::Key(a_code)),
            ]
        );
    }

    #[test]
    fn type_event_with_an_uppercase_letter_wraps_it_in_shift_press_release() {
        let cfg = HclConfig::parse(r#"macro "x" { events = [ { type = "A" } ] }"#).unwrap();
        let a_code = KeyMappingCodec::new().symbol_to_keycode("A").unwrap();
        assert_eq!(
            cfg.macros()[0].events,
            vec![
                MacroEvent::Delay(1),
                MacroEvent::Press(MacroKey::Modifier(ModifierSet::L_SHIFT)),
                MacroEvent::Delay(1),
                MacroEvent::Press(MacroKey::Key(a_code)),
                MacroEvent::Delay(1),
                MacroEvent::Release(MacroKey::Key(a_code)),
                MacroEvent::Delay(1),
                MacroEvent::Release(MacroKey::Modifier(ModifierSet::L_SHIFT)),
            ]
        );
    }

    #[test]
    fn type_event_with_multiple_characters_concatenates_their_expansions() {
        let cfg = HclConfig::parse(r#"macro "x" { events = [ { type = "ab" } ] }"#).unwrap();
        let a_code = KeyMappingCodec::new().symbol_to_keycode("A").unwrap();
        let b_code = KeyMappingCodec::new().symbol_to_keycode("B").unwrap();
        assert_eq!(
            cfg.macros()[0].events,
            vec![
                MacroEvent::Delay(1),
                MacroEvent::Press(MacroKey::Key(a_code)),
                MacroEvent::Delay(1),
                MacroEvent::Release(MacroKey::Key(a_code)),
                MacroEvent::Delay(1),
                MacroEvent::Press(MacroKey::Key(b_code)),
                MacroEvent::Delay(1),
                MacroEvent::Release(MacroKey::Key(b_code)),
            ]
        );
    }

    #[test]
    fn type_event_supports_digits_punctuation_and_shifted_symbols() {
        // "1!" -> digit 1 (no shift) then shifted 1 (the "!" symbol)
        let cfg = HclConfig::parse(r#"macro "x" { events = [ { type = "1!" } ] }"#).unwrap();
        let one_code = KeyMappingCodec::new().symbol_to_keycode("1").unwrap();
        assert_eq!(
            cfg.macros()[0].events,
            vec![
                MacroEvent::Delay(1),
                MacroEvent::Press(MacroKey::Key(one_code)),
                MacroEvent::Delay(1),
                MacroEvent::Release(MacroKey::Key(one_code)),
                MacroEvent::Delay(1),
                MacroEvent::Press(MacroKey::Modifier(ModifierSet::L_SHIFT)),
                MacroEvent::Delay(1),
                MacroEvent::Press(MacroKey::Key(one_code)),
                MacroEvent::Delay(1),
                MacroEvent::Release(MacroKey::Key(one_code)),
                MacroEvent::Delay(1),
                MacroEvent::Release(MacroKey::Modifier(ModifierSet::L_SHIFT)),
            ]
        );
    }

    #[test]
    fn type_event_supports_newline_and_tab() {
        let cfg = HclConfig::parse(r#"macro "x" { events = [ { type = "\n\t" } ] }"#).unwrap();
        let enter_code = KeyMappingCodec::new().symbol_to_keycode("Enter").unwrap();
        let tab_code = KeyMappingCodec::new().symbol_to_keycode("Tab").unwrap();
        assert_eq!(
            cfg.macros()[0].events,
            vec![
                MacroEvent::Delay(1),
                MacroEvent::Press(MacroKey::Key(enter_code)),
                MacroEvent::Delay(1),
                MacroEvent::Release(MacroKey::Key(enter_code)),
                MacroEvent::Delay(1),
                MacroEvent::Press(MacroKey::Key(tab_code)),
                MacroEvent::Delay(1),
                MacroEvent::Release(MacroKey::Key(tab_code)),
            ]
        );
    }

    #[test]
    fn type_event_with_an_unsupported_character_is_rejected_with_position() {
        // é is HCL's 4-digit unicode escape for 'é' — outside printable US-ASCII.
        let err = HclConfig::parse(r#"macro "x" { events = [ { type = "abéc" } ] }"#).unwrap_err();
        assert!(
            matches!(&err, KeymapError::HclValidation { detail, .. } if detail.contains("position 2")),
            "got: {err:?}"
        );
    }

    #[test]
    fn type_event_can_be_mixed_with_press_release_delay_in_one_macro() {
        let cfg = HclConfig::parse(
            r#"
macro "x" {
  events = [
    { press = "LCtrl" },
    { type = "a" },
    { release = "LCtrl" },
  ]
}
"#,
        )
        .unwrap();
        let a_code = KeyMappingCodec::new().symbol_to_keycode("A").unwrap();
        assert_eq!(
            cfg.macros()[0].events,
            vec![
                MacroEvent::Press(MacroKey::Modifier(ModifierSet::L_CTRL)),
                MacroEvent::Delay(1),
                MacroEvent::Press(MacroKey::Key(a_code)),
                MacroEvent::Delay(1),
                MacroEvent::Release(MacroKey::Key(a_code)),
                MacroEvent::Release(MacroKey::Modifier(ModifierSet::L_CTRL)),
            ]
        );
    }

    // ---- `${env("VAR")}` interpolation: env vars in, never secrets on disk ----

    /// Sets a real process env var for the duration of the test and removes it on
    /// drop (including on panic/early return), so tests never leak env state into
    /// each other. Each test must use its own unique var name — `env_func` reads the
    /// real `std::env`, which is process-global, so two tests sharing a name would
    /// race under Rust's default parallel test execution.
    struct EnvVarGuard {
        name: &'static str,
    }
    impl EnvVarGuard {
        fn set(name: &'static str, value: &str) -> Self {
            std::env::set_var(name, value);
            EnvVarGuard { name }
        }
    }
    impl Drop for EnvVarGuard {
        fn drop(&mut self) {
            std::env::remove_var(self.name);
        }
    }

    #[test]
    fn env_func_substitutes_into_a_type_event() {
        let _guard = EnvVarGuard::set("RK_A72_TEST_GREETING", "Hi");
        let cfg = HclConfig::parse(r#"macro "x" { events = [ { type = "${env("RK_A72_TEST_GREETING")}" } ] }"#).unwrap();
        let h_code = KeyMappingCodec::new().symbol_to_keycode("H").unwrap();
        let i_code = KeyMappingCodec::new().symbol_to_keycode("I").unwrap();
        assert_eq!(
            cfg.macros()[0].events,
            vec![
                MacroEvent::Delay(1),
                MacroEvent::Press(MacroKey::Modifier(ModifierSet::L_SHIFT)),
                MacroEvent::Delay(1),
                MacroEvent::Press(MacroKey::Key(h_code)),
                MacroEvent::Delay(1),
                MacroEvent::Release(MacroKey::Key(h_code)),
                MacroEvent::Delay(1),
                MacroEvent::Release(MacroKey::Modifier(ModifierSet::L_SHIFT)),
                MacroEvent::Delay(1),
                MacroEvent::Press(MacroKey::Key(i_code)),
                MacroEvent::Delay(1),
                MacroEvent::Release(MacroKey::Key(i_code)),
            ]
        );
    }

    #[test]
    fn env_func_returns_empty_string_for_an_unset_var_not_an_error() {
        std::env::remove_var("RK_A72_TEST_DEFINITELY_UNSET"); // in case a previous run left it
        let cfg = HclConfig::parse(
            r#"macro "x" { events = [ { type = "${env("RK_A72_TEST_DEFINITELY_UNSET")}a" } ] }"#,
        )
        .unwrap();
        // "" + "a" -> just one press/release pair for 'a', proving env(...) resolved to
        // "" rather than erroring the whole parse.
        let a_code = KeyMappingCodec::new().symbol_to_keycode("A").unwrap();
        assert_eq!(
            cfg.macros()[0].events,
            vec![
                MacroEvent::Delay(1),
                MacroEvent::Press(MacroKey::Key(a_code)),
                MacroEvent::Delay(1),
                MacroEvent::Release(MacroKey::Key(a_code)),
            ]
        );
    }

    #[test]
    fn env_func_enables_a_default_value_ternary_in_hcl() {
        std::env::remove_var("RK_A72_TEST_UNSET_FOR_TERNARY");
        let cfg = HclConfig::parse(
            r#"macro "x" { events = [ { type = "${env("RK_A72_TEST_UNSET_FOR_TERNARY") != "" ? env("RK_A72_TEST_UNSET_FOR_TERNARY") : "empty"}" } ] }"#,
        )
        .unwrap();
        // Should type "empty" (5 press/release pairs), proving the ternary's false
        // branch fired rather than the whole expression erroring.
        assert_eq!(cfg.macros()[0].events.len(), 5 * 4);
    }

    #[test]
    fn dollar_dollar_brace_escapes_interpolation_to_a_literal_dollar_brace() {
        // "$${x}" must NOT try to call env(...) — it types the literal three
        // characters '$', '{', '}' instead (HCL's own escape for a literal "${" —
        // see the hcl-rs eval module docs).
        let cfg = HclConfig::parse(r#"macro "x" { events = [ { type = "$${}" } ] }"#).unwrap();
        let dollar_code = KeyMappingCodec::new().symbol_to_keycode("4").unwrap(); // Shift+4 = '$'
        let brace_code = KeyMappingCodec::new().symbol_to_keycode("[").unwrap(); // Shift+[ = '{'
        let close_brace_code = KeyMappingCodec::new().symbol_to_keycode("]").unwrap(); // Shift+] = '}'
        assert_eq!(
            cfg.macros()[0].events,
            vec![
                MacroEvent::Delay(1),
                MacroEvent::Press(MacroKey::Modifier(ModifierSet::L_SHIFT)),
                MacroEvent::Delay(1),
                MacroEvent::Press(MacroKey::Key(dollar_code)),
                MacroEvent::Delay(1),
                MacroEvent::Release(MacroKey::Key(dollar_code)),
                MacroEvent::Delay(1),
                MacroEvent::Release(MacroKey::Modifier(ModifierSet::L_SHIFT)),
                MacroEvent::Delay(1),
                MacroEvent::Press(MacroKey::Modifier(ModifierSet::L_SHIFT)),
                MacroEvent::Delay(1),
                MacroEvent::Press(MacroKey::Key(brace_code)),
                MacroEvent::Delay(1),
                MacroEvent::Release(MacroKey::Key(brace_code)),
                MacroEvent::Delay(1),
                MacroEvent::Release(MacroKey::Modifier(ModifierSet::L_SHIFT)),
                MacroEvent::Delay(1),
                MacroEvent::Press(MacroKey::Modifier(ModifierSet::L_SHIFT)),
                MacroEvent::Delay(1),
                MacroEvent::Press(MacroKey::Key(close_brace_code)),
                MacroEvent::Delay(1),
                MacroEvent::Release(MacroKey::Key(close_brace_code)),
                MacroEvent::Delay(1),
                MacroEvent::Release(MacroKey::Modifier(ModifierSet::L_SHIFT)),
            ]
        );
    }

    #[test]
    fn env_func_also_works_outside_type_events() {
        // The whole document is HCL-template-evaluated, not just `type` strings — so
        // env(...) works anywhere an attribute value is a string, e.g. `press = "..."`.
        let _guard = EnvVarGuard::set("RK_A72_TEST_KEYNAME", "C");
        let cfg = HclConfig::parse(r#"macro "x" { events = [ { press = "${env("RK_A72_TEST_KEYNAME")}" } ] }"#).unwrap();
        let c_code = KeyMappingCodec::new().symbol_to_keycode("C").unwrap();
        assert_eq!(cfg.macros()[0].events, vec![MacroEvent::Press(MacroKey::Key(c_code))]);
    }

    #[test]
    fn macro_repeat_defaults_to_one() {
        let cfg = HclConfig::parse(r#"macro "x" { events = [ { delay = 5 } ] }"#).unwrap();
        assert_eq!(cfg.macros()[0].repeat, 1);
    }

    #[test]
    fn macro_repeat_can_be_set_explicitly() {
        // HCL attributes are newline-separated, not comma-separated (see the `mapping`
        // block grammar note in `hcl_doc_header`) — same constraint applies here.
        let cfg = HclConfig::parse(
            r#"
macro "x" {
  repeat = 3
  events = [ { delay = 5 } ]
}
"#,
        )
        .unwrap();
        assert_eq!(cfg.macros()[0].repeat, 3);
    }

    #[test]
    fn a_macro_action_on_a_layer_compiles_to_a_macro_keymappingtype_slot_value() {
        let cfg = HclConfig::parse(
            r#"
macro "first" { events = [ { delay = 1 } ] }
macro "copy" {
  repeat = 2
  events = [ { delay = 1 } ]
}
layer "fn" {
  mapping "M1" { macro = "copy" }
}
"#,
        )
        .unwrap();
        assert_eq!(
            cfg.layers()[&LAYER_FN].values().next().unwrap(),
            &LayerAction::Macro("copy".to_string())
        );

        let maps = cfg.layer_slot_maps().unwrap();
        let slot = PhysicalKeyboardLayout::new().slot_for_name("M1").unwrap();
        let value = maps[&LAYER_FN][&slot];

        // KeyMappingType::Macro (3) in the top byte, keyMappingPara = 1 ("repeat N
        // times" cycle type — confirmed against the real configurator's
        // confirmSetMacro(), required for the firmware to actually play the macro
        // back) in the next byte, repeat count in keyCode's high byte, macro index (1,
        // since "copy" is the second macro block, source order 0-based) in keyCode's
        // low byte.
        assert_eq!((value >> 24) as u8, crate::mapping_type::KeyMappingType::Macro.to_byte());
        assert_eq!(((value >> 16) & 0xff) as u8, 1, "keyMappingPara must be 1 (cycle type: repeat N times)");
        let key_code = (value & 0xffff) as u16;
        assert_eq!((key_code & 0xff) as u8, 1); // macro index of "copy"
        assert_eq!((key_code >> 8) as u8, 2); // repeat count
    }

    #[test]
    fn a_layer_macro_reference_to_an_undefined_macro_is_rejected() {
        let err = HclConfig::parse(
            r#"
layer "fn" {
  mapping "M1" { macro = "ghost" }
}
"#,
        )
        .unwrap_err();
        assert!(matches!(err, KeymapError::HclValidation { detail, .. } if detail.contains("ghost")));
    }

    #[test]
    fn a_macro_name_that_does_not_fit_the_devices_one_byte_length_header_is_rejected() {
        // The wire format's macro-name-length header is one byte holding the UTF-16LE
        // byte count — max 255 bytes / 127 UTF-16 code units. `Macro::serialize` doesn't
        // check this (silently truncates via `as u8`), so a too-long name must be
        // rejected here, before compiling anything.
        let too_long = "x".repeat(128); // 128 code units = 256 UTF-16LE bytes
        let hcl = format!(r#"macro "{too_long}" {{ events = [ {{ delay = 1 }} ] }}"#);
        let err = HclConfig::parse(&hcl).unwrap_err();
        assert!(
            matches!(&err, KeymapError::HclValidation { detail, .. } if detail.contains("255 bytes")),
            "got: {err:?}"
        );

        // 127 code units (254 bytes) is exactly at the limit and must be accepted.
        let at_limit = "x".repeat(127);
        let hcl = format!(r#"macro "{at_limit}" {{ events = [ {{ delay = 1 }} ] }}"#);
        HclConfig::parse(&hcl).unwrap();
    }

    #[test]
    fn a_macro_table_that_would_exceed_the_devices_buffer_capacity_is_rejected() {
        // Build enough NormalKey press/release actions across a few macros that the
        // encoded table exceeds MACRO_BUFFER_LEN (4096 bytes) — the device's real macro
        // table capacity (8 pages x 512 bytes, confirmed by GetMacros/read_macros).
        // Neither compile_macros nor write_macros validated this; an oversized table
        // used to silently write past the device's real macro region with no error.
        let events: String = (0..1100)
            .map(|_| r#"{ press = "C" },"#)
            .collect::<Vec<_>>()
            .join("\n");
        let hcl = format!(r#"macro "big" {{ events = [{events}] }}"#);
        let err = HclConfig::parse(&hcl).unwrap_err();
        assert!(
            matches!(&err, KeymapError::HclValidation { detail, .. } if detail.contains("exceeds")),
            "got: {err:?}"
        );
    }

    // ---- theme + color parsing (parsed + validated, not flashable) ----

    #[test]
    fn theme_resolves_hex_rgb_and_named_colors() {
        let cfg = HclConfig::parse(
            r##"
theme {
  main   = "#00FF00"
  accent = "rgb(0, 128, 255)"
  alert  = "red"
}
"##,
        )
        .unwrap();
        assert_eq!(cfg.theme()["main"], [0, 255, 0]);
        assert_eq!(cfg.theme()["accent"], [0, 128, 255]);
        assert_eq!(cfg.theme()["alert"], [255, 0, 0]);
    }

    #[test]
    fn invalid_theme_color_is_rejected() {
        let err = HclConfig::parse(r#"theme { bad = "notacolor" }"#).unwrap_err();
        assert!(matches!(err, KeymapError::HclValidation { detail, .. } if detail.contains("not a valid CSS color")));
    }

    #[test]
    fn lighting_colors_resolve_via_theme_alias_and_direct_css() {
        let cfg = HclConfig::parse(
            r#"
theme {
  alert = "red"
}
lighting {
  effect     = "custom"
  brightness = 100
  speed      = 5
  colors = {
    "Esc"   = "alert"
    "Enter" = "gold"
  }
}
"#,
        )
        .unwrap();
        let light = cfg.lighting().unwrap();
        assert_eq!(light.effect.as_deref(), Some("custom"));
        assert_eq!(light.brightness, Some(100));
        assert_eq!(light.speed, Some(5));
        let esc = light.colors.iter().find(|c| c.key == "Esc").unwrap();
        assert_eq!(esc.rgb, [255, 0, 0]); // resolved via theme alias "alert"
        let enter = light.colors.iter().find(|c| c.key == "Enter").unwrap();
        assert_eq!(enter.rgb, [255, 215, 0]); // CSS "gold"
    }

    #[test]
    fn lighting_brightness_out_of_range_is_rejected() {
        let err = HclConfig::parse(r#"lighting { brightness = 150 }"#).unwrap_err();
        assert!(matches!(err, KeymapError::HclValidation { detail, .. } if detail.contains("brightness")));
    }

    #[test]
    fn lighting_unknown_physical_key_is_rejected() {
        let err = HclConfig::parse(r#"lighting { colors = { "NotAKey" = "red" } }"#).unwrap_err();
        assert!(matches!(err, KeymapError::HclValidation { detail, .. } if detail.contains("NotAKey")));
    }

    #[test]
    fn compile_lighting_encodes_the_planar_378_byte_buffer() {
        let cfg = HclConfig::parse(r#"lighting { colors = { "Esc" = "red" } }"#).unwrap();
        let esc_slot = cfg.lighting().unwrap().colors[0].slot as usize;

        let buf = cfg.compile_lighting().unwrap();
        assert_eq!(buf.len(), LED_COLORS_BUFFER_LEN);
        assert_eq!(buf[esc_slot], 255); // R plane
        assert_eq!(buf[esc_slot + LED_COLORS_SLOT_COUNT], 0); // G plane
        assert_eq!(buf[esc_slot + LED_COLORS_SLOT_COUNT * 2], 0); // B plane
        // every other slot stays black
        assert_eq!(buf.iter().filter(|&&b| b != 0).count(), 1);
    }

    #[test]
    fn compile_lighting_is_all_zero_without_a_lighting_block() {
        let cfg = HclConfig::parse("").unwrap();
        let buf = cfg.compile_lighting().unwrap();
        assert_eq!(buf, vec![0u8; LED_COLORS_BUFFER_LEN]);
    }

    #[test]
    fn dump_lighting_emits_only_non_zero_slots_as_raw_hex() {
        let layout = PhysicalKeyboardLayout::new();
        let esc_slot = layout.slot_for_name("Esc").unwrap() as usize;
        let mut buf = vec![0u8; LED_COLORS_BUFFER_LEN];
        buf[esc_slot] = 0xff; // R
        buf[esc_slot + LED_COLORS_SLOT_COUNT] = 0x80; // G
        buf[esc_slot + LED_COLORS_SLOT_COUNT * 2] = 0x00; // B

        let exporter = HclExporter::new(KeyMappingCodec::new(), layout);
        let text = exporter.dump_lighting(&buf);
        assert!(text.contains("lighting {"));
        assert!(text.contains("\"Esc\" = \"#ff8000\""));

        // round-trips: re-parsing the emitted block and re-compiling reproduces the buffer.
        let cfg = HclConfig::parse(&text).unwrap();
        assert_eq!(cfg.compile_lighting().unwrap(), buf);
    }

    #[test]
    fn dump_lighting_is_empty_when_every_slot_is_black() {
        let exporter = HclExporter::new(KeyMappingCodec::new(), PhysicalKeyboardLayout::new());
        let text = exporter.dump_lighting(&vec![0u8; LED_COLORS_BUFFER_LEN]);
        assert_eq!(text, "");
    }

    // ---- whole-document round trip ----

    #[test]
    fn a_full_document_parses_and_only_layers_compile() {
        let cfg = HclConfig::parse(
            r##"
theme {
  main  = "#00FF00"
  alert = "red"
}

macro "copy" {
  events = [
    { press = "LCtrl" },
    { press = "C" },
    { release = "C" },
    { release = "LCtrl" },
  ]
}

layer "fn" {
  mapping "Enter" { key = "Insert" }
  mapping "Esc"   { label = "Mute" }
}

lighting {
  brightness = 80
  colors = {
    "Esc" = "alert"
    "W"   = "main"
  }
}
"##,
        )
        .unwrap();

        // Layers compile.
        let maps = cfg.layer_slot_maps().unwrap();
        assert_eq!(maps[&LAYER_FN].len(), 2);

        // Macros, layers, and lighting all compile.
        assert_eq!(cfg.macros().len(), 1);
        assert_eq!(cfg.lighting().unwrap().colors.len(), 2);
        assert!(cfg.compile_macros().is_ok());
        assert!(cfg.compile_lighting().is_ok());
    }

    #[test]
    fn empty_document_parses_to_nothing() {
        let cfg = HclConfig::parse("").unwrap();
        let maps = cfg.layer_slot_maps().unwrap();
        assert!(maps[&LAYER_NORMAL].is_empty());
        assert!(maps[&LAYER_FN].is_empty());
        assert!(cfg.macros().is_empty());
        assert!(cfg.lighting().is_none());
    }

    // ---- HCL export ----

    fn new_exporter() -> HclExporter {
        HclExporter::new(KeyMappingCodec::new(), PhysicalKeyboardLayout::new())
    }

    /// True if `text` contains a real, non-commented `layer "..." {` block start (as
    /// opposed to the doc header's `#   layer "normal" { ... }` syntax example, which
    /// contains the same substring but prefixed with `#`).
    fn has_layer_block(text: &str) -> bool {
        text.lines().any(|l| l.starts_with("layer \""))
    }

    fn empty_buffers() -> HashMap<u8, Vec<u8>> {
        let mut m = HashMap::new();
        m.insert(LAYER_NORMAL, vec![0u8; crate::protocol::KEYMATRIX_BUFFER_LEN]);
        m.insert(LAYER_FN, vec![0u8; crate::protocol::KEYMATRIX_BUFFER_LEN]);
        m.insert(LAYER_FN2, vec![0u8; crate::protocol::KEYMATRIX_BUFFER_LEN]);
        m
    }

    #[test]
    fn dump_diff_omits_slots_matching_the_baseline() {
        let exporter = new_exporter();
        let baseline = empty_buffers();
        let buffers = empty_buffers(); // identical to baseline
        let text = exporter.dump_diff(&buffers, &baseline, &[]);
        assert!(
            !has_layer_block(&text),
            "expected no layer blocks when nothing differs, got: {text}"
        );
        // Still parses cleanly to no slots — the doc header is inert comment text.
        let cfg = HclConfig::parse(&text).unwrap();
        let maps = cfg.layer_slot_maps().unwrap();
        assert!(maps[&LAYER_NORMAL].is_empty());
        assert!(maps[&LAYER_FN].is_empty());
    }

    #[test]
    fn dump_includes_a_self_documenting_header_with_every_reference_category() {
        let exporter = new_exporter();
        let text = exporter.dump(&empty_buffers(), &[]);
        for needle in [
            "# SYNTAX",
            "# EXAMPLES",
            "# PHYSICAL KEYS",
            "# KEY SYMBOLS",
            "# MODIFIERS",
            "# LABELS",
            "Esc,",  // a physical key name shows up in the reference list
            "LCtrl", // a modifier name shows up in the reference list
        ] {
            assert!(text.contains(needle), "expected header to contain {needle:?}, got: {text}");
        }
        // The header is comment-only — parses to no layers even with nothing exported.
        let cfg = HclConfig::parse(&text).unwrap();
        assert!(cfg.layer_slot_maps().unwrap()[&LAYER_NORMAL].is_empty());
    }

    #[test]
    fn dump_diff_emits_only_the_slot_that_differs_from_baseline() {
        let exporter = new_exporter();
        let baseline = empty_buffers();
        let mut buffers = empty_buffers();

        let esc_slot = PhysicalKeyboardLayout::new().slot_for_name("Esc").unwrap();
        let b = KeyMappingCodec::encode_keyboard(5, ModifierSet::empty()); // "B"
        let offset = esc_slot as usize * 4;
        buffers.get_mut(&LAYER_NORMAL).unwrap()[offset..offset + 4]
            .copy_from_slice(&b.to_be_bytes());

        let text = exporter.dump_diff(&buffers, &baseline, &[]);
        assert!(has_layer_block(&text), "expected a real layer block, got: {text}");
        assert!(!text.contains("\nlayer \"fn\" {"), "got: {text}");
        assert!(
            text.contains("mapping \"Esc\" {\n    key = \"B\"\n  }"),
            "got: {text}"
        );

        // And it parses back through the real HCL front-end to the same slot value.
        let cfg = HclConfig::parse(&text).unwrap();
        let maps = cfg.layer_slot_maps().unwrap();
        assert_eq!(maps[&LAYER_NORMAL][&esc_slot], b);
    }

    #[test]
    fn dump_diff_emits_a_fn2_layer_block() {
        let exporter = new_exporter();
        let baseline = empty_buffers();
        let mut buffers = empty_buffers();

        let s_slot = PhysicalKeyboardLayout::new().slot_for_name("S").unwrap();
        let backspace_code = KeyMappingCodec::new().symbol_to_keycode("Backspace").unwrap();
        let backspace = KeyMappingCodec::encode_keyboard(backspace_code, ModifierSet::empty());
        let offset = s_slot as usize * 4;
        buffers.get_mut(&LAYER_FN2).unwrap()[offset..offset + 4]
            .copy_from_slice(&backspace.to_be_bytes());

        let text = exporter.dump_diff(&buffers, &baseline, &[]);
        assert!(text.contains("layer \"fn2\" {"), "got: {text}");
        assert!(!text.contains("\nlayer \"normal\" {"), "got: {text}");
        assert!(!text.contains("\nlayer \"fn\" {"), "got: {text}");

        let cfg = HclConfig::parse(&text).unwrap();
        let maps = cfg.layer_slot_maps().unwrap();
        assert_eq!(maps[&LAYER_FN2][&s_slot], backspace);
    }

    #[test]
    fn dump_with_modifiers_round_trips_through_parse() {
        let exporter = new_exporter();
        let mut buffers = empty_buffers();

        let m1_slot = PhysicalKeyboardLayout::new().slot_for_name("M1").unwrap();
        let c = KeyMappingCodec::new().symbol_to_keycode("C").unwrap();
        let ctrl_shift_c =
            KeyMappingCodec::encode_keyboard(c, ModifierSet::L_CTRL | ModifierSet::L_SHIFT);
        let offset = m1_slot as usize * 4;
        buffers.get_mut(&LAYER_NORMAL).unwrap()[offset..offset + 4]
            .copy_from_slice(&ctrl_shift_c.to_be_bytes());

        let text = exporter.dump(&buffers, &[]);
        let cfg = HclConfig::parse(&text).unwrap();
        let maps = cfg.layer_slot_maps().unwrap();
        assert_eq!(maps[&LAYER_NORMAL][&m1_slot], ctrl_shift_c);
    }

    #[test]
    fn dump_with_a_non_keyboard_label_round_trips_through_parse() {
        let exporter = new_exporter();
        let codec = KeyMappingCodec::new();
        let mut buffers = empty_buffers();

        let mute_raw = codec.label_to_raw("Mute").unwrap();
        let mute_slot = PhysicalKeyboardLayout::new().slot_for_name("Mute").unwrap();
        let offset = mute_slot as usize * 4;
        buffers.get_mut(&LAYER_NORMAL).unwrap()[offset..offset + 4]
            .copy_from_slice(&mute_raw.to_be_bytes());

        let text = exporter.dump(&buffers, &[]);
        assert!(
            text.contains("mapping \"Mute\" {\n    label = \"Mute\"\n  }"),
            "got: {text}"
        );

        let cfg = HclConfig::parse(&text).unwrap();
        let maps = cfg.layer_slot_maps().unwrap();
        assert_eq!(maps[&LAYER_NORMAL][&mute_slot], mute_raw);
    }

    #[test]
    fn dump_with_macro_names_resolves_the_macro_label_instead_of_a_bare_index() {
        let exporter = new_exporter();
        let mut buffers = empty_buffers();

        let m1_slot = PhysicalKeyboardLayout::new().slot_for_name("M1").unwrap();
        // KeyMappingType::Macro, index 0, repeat 1.
        let macro_value: u32 = (3u32 << 24) | (1u32 << 8) | 0u32;
        let offset = m1_slot as usize * 4;
        buffers.get_mut(&LAYER_NORMAL).unwrap()[offset..offset + 4]
            .copy_from_slice(&macro_value.to_be_bytes());

        let macro_names = vec!["MyMacro".to_string()];
        let text = exporter.dump(&buffers, &macro_names);
        assert!(
            text.contains("mapping \"M1\" {\n    macro = \"MyMacro\"\n  }"),
            "got: {text}"
        );
    }

    #[test]
    fn dump_macros_emits_macro_blocks_with_events_and_repeat() {
        use crate::macros::{Macro as WireMacro, MacroAction, MacroActionKind, MacroEdge};
        let exporter = new_exporter();
        let macros = vec![WireMacro {
            name: "copy".to_string(),
            actions: vec![
                MacroAction { edge: MacroEdge::Down, kind: MacroActionKind::NormalKey, delay: 0, key: KeyMappingCodec::new().symbol_to_keycode("C").unwrap() as u8 },
                MacroAction { edge: MacroEdge::Up, kind: MacroActionKind::NormalKey, delay: 50, key: KeyMappingCodec::new().symbol_to_keycode("C").unwrap() as u8 },
            ],
        }];
        let text = exporter.dump_macros(&macros);
        assert!(text.contains("macro \"copy\" {"), "got: {text}");
        assert!(text.contains("press = \"C\""), "got: {text}");
        assert!(text.contains("delay = 50"), "got: {text}");
        assert!(text.contains("release = \"C\""), "got: {text}");

        // Round-trips: re-parsing reproduces an equivalent macro definition.
        let cfg = HclConfig::parse(&text).unwrap();
        assert_eq!(cfg.macros()[0].name, "copy");
    }
}
