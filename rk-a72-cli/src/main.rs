mod device;

use std::collections::HashMap;
use std::ffi::OsStr;
use std::fs;

use anyhow::{bail, Result};
use clap::{CommandFactory, Parser, Subcommand};
use clap_complete::engine::{ArgValueCompleter, CompletionCandidate};
use clap_complete::CompleteEnv;
use rk_a72_keymap::{
    HclConfig, HclExporter, KeyMappingCodec, KeyMappingType, KeyMatrixRepository,
    KeymapYamlSerializer, LedColorRepository, MacroRepository, ModifierSet, PhysicalKeyboardLayout,
    WiredSession, SUPPORTED_PRODUCT_ID, SUPPORTED_VENDOR_ID,
};
use rk_a72_keymap::macros::{MacroActionKind, MacroEdge};

use device::select_wired_device;

#[derive(Parser)]
#[command(name = "rk-a72")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Read the KeyMatrix (layers 0/1/2 — Normal, Fn, Fn2) from a wired A72 and write it out
    /// as YAML. By default only prints slots that differ from the factory default (the compact
    /// form import-keymap round-trips onto); pass --full to dump every populated slot.
    ExportKeymap {
        #[arg(long, default_value = DEFAULT_VID, value_parser = parse_supported_vid)]
        vid: u16,
        #[arg(long, default_value = DEFAULT_PID, value_parser = parse_supported_pid)]
        pid: u16,
        /// Dump every populated slot, not just the ones that differ from factory default
        #[arg(long)]
        full: bool,
        /// Output YAML file (default: stdout)
        out: Option<String>,
    },
    /// Apply a YAML keymap file (from export-keymap) to a wired A72 — the
    /// file is merged onto the factory-default KeyMatrix, so slots it doesn't mention
    /// reset to factory rather than keeping whatever the device currently holds
    ImportKeymap {
        file: String,
        #[arg(long, default_value = DEFAULT_VID, value_parser = parse_supported_vid)]
        vid: u16,
        #[arg(long, default_value = DEFAULT_PID, value_parser = parse_supported_pid)]
        pid: u16,
        #[arg(long)]
        dry_run: bool,
    },
    /// Read the KeyMatrix, macro table, and LED colors from a wired A72 and write
    /// them out as HCL (issue #2 schema) — `layer` blocks, `macro` blocks, and a `lighting.colors`
    /// block. There's no `theme` to round-trip since the device holds only resolved RGB, not named
    /// aliases. By default only prints KeyMatrix slots that differ from the factory default; pass
    /// --full to dump every populated slot (macros and lighting are always dumped in full, since
    /// there's no factory baseline to diff them against).
    ExportHcl {
        #[arg(long, default_value = DEFAULT_VID, value_parser = parse_supported_vid)]
        vid: u16,
        #[arg(long, default_value = DEFAULT_PID, value_parser = parse_supported_pid)]
        pid: u16,
        /// Dump every populated slot, not just the ones that differ from factory default
        #[arg(long)]
        full: bool,
        /// Output HCL file (default: stdout)
        out: Option<String>,
    },
    /// Apply an HCL config file (issue #2 schema) to a wired A72. Writes
    /// the macro table (full replace when the file has at least one `macro` block; a
    /// file with NO `macro` blocks at all leaves the device's macros untouched), then
    /// the `layer` KeyMatrix (merged onto the factory-default, so slots the file
    /// doesn't mention reset to factory rather than keeping device state), then
    /// `lighting.colors` if present. `theme` blocks are parsed but never written — the
    /// device has no concept of named colors, only resolved RGB.
    ImportHcl {
        file: String,
        #[arg(long, default_value = DEFAULT_VID, value_parser = parse_supported_vid)]
        vid: u16,
        #[arg(long, default_value = DEFAULT_PID, value_parser = parse_supported_pid)]
        pid: u16,
        #[arg(long)]
        dry_run: bool,
    },
    /// Read the macro table from a wired A72 and print each macro's
    /// name and decoded action sequence. Read-only — does not write anything.
    GetMacros {
        #[arg(long, default_value = DEFAULT_VID, value_parser = parse_supported_vid)]
        vid: u16,
        #[arg(long, default_value = DEFAULT_PID, value_parser = parse_supported_pid)]
        pid: u16,
    },
    /// Print every physical key name, KeyBoard key symbol, modifier name, and label
    /// value usable in export-keymap/import-keymap YAML files
    ListKeys,
    /// Print the current mapping of one physical key, without going through a YAML file
    GetKeymap {
        key: String,
        /// "normal", "fn" or "fn2"
        #[arg(long, default_value = "normal", value_parser = ["normal", "fn", "fn2"])]
        layer: String,
        #[arg(long, default_value = DEFAULT_VID, value_parser = parse_supported_vid)]
        vid: u16,
        #[arg(long, default_value = DEFAULT_PID, value_parser = parse_supported_pid)]
        pid: u16,
    },
    /// Change the mapping of one physical key directly on the device, without going
    /// through a YAML file. Give exactly one of --raw, --label, --symbol
    SetKeymap {
        key: String,
        /// "normal", "fn" or "fn2"
        #[arg(long, default_value = "normal", value_parser = ["normal", "fn", "fn2"])]
        layer: String,
        /// Non-KeyBoard label value, e.g. "Mute" (see list-keys)
        #[arg(long)]
        label: Option<String>,
        /// KeyBoard key symbol, e.g. "A" (see list-keys)
        #[arg(long)]
        symbol: Option<String>,
        /// Modifier names to combine with --symbol, e.g. "LCtrl+LShift"
        #[arg(long = "mod")]
        modifier: Option<String>,
        /// Raw hex value, e.g. "0x020000e2"
        #[arg(long)]
        raw: Option<String>,
        #[arg(long)]
        dry_run: bool,
        #[arg(long, default_value = DEFAULT_VID, value_parser = parse_supported_vid)]
        vid: u16,
        #[arg(long, default_value = DEFAULT_PID, value_parser = parse_supported_pid)]
        pid: u16,
    },
}

/// Textual defaults for `--vid`/`--pid`. Kept in sync with the library constants by
/// `defaults_match_supported_ids` below.
const DEFAULT_VID: &str = "258a";
const DEFAULT_PID: &str = "0216";

fn parse_hex_u16(s: &str) -> Result<u16, String> {
    u16::from_str_radix(s, 16).map_err(|e| e.to_string())
}

/// `--vid`/`--pid` exist so the IDs are visible and scriptable, but only the wired A72's
/// own IDs are accepted: every byte layout this tool writes was verified against that one
/// device, so pointing it at another keyboard would flash guessed data. See
/// `SUPPORTED_VENDOR_ID` in `rk-a72-keymap` for the reasoning.
fn parse_supported_vid(s: &str) -> Result<u16, String> {
    let vid = parse_hex_u16(s)?;
    if vid == SUPPORTED_VENDOR_ID {
        Ok(vid)
    } else {
        Err(format!(
            "unsupported vendor id {vid:04x} — rk-a72 only supports the wired RK A72 \
             (vid={SUPPORTED_VENDOR_ID:04x} pid={SUPPORTED_PRODUCT_ID:04x})"
        ))
    }
}

/// See [`parse_supported_vid`].
fn parse_supported_pid(s: &str) -> Result<u16, String> {
    let pid = parse_hex_u16(s)?;
    if pid == SUPPORTED_PRODUCT_ID {
        Ok(pid)
    } else {
        Err(format!(
            "unsupported product id {pid:04x} — rk-a72 only supports the wired RK A72 \
             (vid={SUPPORTED_VENDOR_ID:04x} pid={SUPPORTED_PRODUCT_ID:04x})"
        ))
    }
}

/// Builds a completion candidate from a canonical name, attaching the old glyph as
/// a help string when it differs (zsh/fish show it next to the suggestion; bash
/// ignores `.help()` entirely, so this is a no-op improvement there).
fn candidate_with_visual(canonical: String, visual: String) -> CompletionCandidate {
    let candidate = CompletionCandidate::new(canonical.clone());
    if visual == canonical {
        candidate
    } else {
        candidate.help(Some(visual.into()))
    }
}

/// Candidates for the positional `key` argument of get-keymap/set-keymap: every
/// physical key name known to `PhysicalKeyboardLayout`, filtered by what's typed so
/// far. Built fresh per completion request — loading the JSON table is cheap and this
/// only runs when a shell asks for completions, never during normal execution.
fn key_name_completer(current: &OsStr) -> Vec<CompletionCandidate> {
    let Some(current) = current.to_str() else {
        return Vec::new();
    };
    PhysicalKeyboardLayout::new()
        .list_named()
        .into_iter()
        .filter(|(name, _, _)| name.starts_with(current))
        .map(|(name, _, visual)| candidate_with_visual(name, visual))
        .collect()
}

/// Candidates for --label: every non-KeyBoard label value known to `KeyMappingCodec`.
fn label_completer(current: &OsStr) -> Vec<CompletionCandidate> {
    let Some(current) = current.to_str() else {
        return Vec::new();
    };
    KeyMappingCodec::new()
        .list_labels()
        .into_iter()
        .filter(|(label, _, _)| label.starts_with(current))
        .map(|(label, _, visual)| candidate_with_visual(label, visual))
        .collect()
}

/// Candidates for --symbol: every KeyBoard key symbol known to `KeyMappingCodec`.
fn symbol_completer(current: &OsStr) -> Vec<CompletionCandidate> {
    let Some(current) = current.to_str() else {
        return Vec::new();
    };
    KeyMappingCodec::new()
        .list_keycode_symbols()
        .into_iter()
        .filter(|(_, symbol, _)| symbol.starts_with(current))
        .map(|(_, symbol, visual)| candidate_with_visual(symbol, visual))
        .collect()
}

/// Candidates for --mod: modifier names, completed one "+"-separated segment at a
/// time so "LCtrl+LSh<Tab>" completes to "LCtrl+LShift" rather than replacing the
/// whole value.
fn modifier_completer(current: &OsStr) -> Vec<CompletionCandidate> {
    let Some(current) = current.to_str() else {
        return Vec::new();
    };
    let (prefix, partial) = match current.rfind('+') {
        Some(i) => (&current[..=i], &current[i + 1..]),
        None => ("", current),
    };
    ModifierSet::list_named()
        .into_iter()
        .filter(|(_, name)| name.starts_with(partial))
        .map(|(_, name)| CompletionCandidate::new(format!("{prefix}{name}")))
        .collect()
}

fn build_cli() -> clap::Command {
    Cli::command()
        .mut_subcommand("get-keymap", |sub| {
            sub.mut_arg("key", |arg| {
                arg.add(ArgValueCompleter::new(key_name_completer))
            })
        })
        .mut_subcommand("set-keymap", |sub| {
            sub.mut_arg("key", |arg| {
                arg.add(ArgValueCompleter::new(key_name_completer))
            })
            .mut_arg("label", |arg| {
                arg.add(ArgValueCompleter::new(label_completer))
            })
            .mut_arg("symbol", |arg| {
                arg.add(ArgValueCompleter::new(symbol_completer))
            })
            .mut_arg("modifier", |arg| {
                arg.add(ArgValueCompleter::new(modifier_completer))
            })
        })
}

fn layer_from_arg(layer: &str) -> Result<u8> {
    match layer {
        "normal" => Ok(0),
        "fn" => Ok(1),
        "fn2" => Ok(2),
        other => bail!("unknown layer \"{other}\" (expected \"normal\", \"fn\" or \"fn2\")"),
    }
}

/// Encodes a set-keymap value from its CLI flags, in the same precedence order as the
/// YAML format's raw/label/key+mod fields: --raw wins if given, then --label, then
/// --symbol/--mod.
fn encode_from_flags(
    codec: &KeyMappingCodec,
    raw: Option<&str>,
    label: Option<&str>,
    symbol: Option<&str>,
    modifier: Option<&str>,
) -> Result<u32> {
    if let Some(raw) = raw {
        let raw = u32::from_str_radix(raw.trim_start_matches("0x"), 16)
            .map_err(|_| anyhow::anyhow!("invalid --raw value \"{raw}\""))?;
        return Ok(KeyMappingCodec::encode_raw(raw));
    }
    if let Some(label) = label {
        return codec
            .label_to_raw(label)
            .map(KeyMappingCodec::encode_raw)
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "unknown label \"{label}\" — run \"list-keys\" to see valid labels."
                )
            });
    }
    if let Some(symbol) = symbol {
        let key_code = codec.symbol_to_keycode(symbol).ok_or_else(|| {
            anyhow::anyhow!(
                "unknown KeyBoard key \"{symbol}\" — run \"list-keys\" to see valid key symbols."
            )
        })?;
        let modifiers = match modifier {
            Some(m) => ModifierSet::from_label(m)?,
            None => ModifierSet::empty(),
        };
        return Ok(KeyMappingCodec::encode_keyboard(key_code, modifiers));
    }
    bail!("give exactly one of --raw, --label, or --symbol")
}

fn main() -> Result<()> {
    CompleteEnv::with_factory(build_cli).complete();

    let cli = Cli::parse();
    match cli.command {
        Command::ExportKeymap {
            vid,
            pid,
            full,
            out,
        } => export_keymap(vid, pid, full, out),
        Command::ExportHcl {
            vid,
            pid,
            full,
            out,
        } => export_hcl(vid, pid, full, out),
        Command::ImportKeymap {
            file,
            vid,
            pid,
            dry_run,
        } => import_keymap(&file, vid, pid, dry_run),
        Command::ImportHcl {
            file,
            vid,
            pid,
            dry_run,
        } => import_hcl(&file, vid, pid, dry_run),
        Command::GetMacros { vid, pid } => get_macros(vid, pid),
        Command::ListKeys => list_keys(),
        Command::GetKeymap {
            key,
            layer,
            vid,
            pid,
        } => get_keymap(&key, &layer, vid, pid),
        Command::SetKeymap {
            key,
            layer,
            label,
            symbol,
            modifier,
            raw,
            dry_run,
            vid,
            pid,
        } => set_keymap(
            &key,
            &layer,
            label.as_deref(),
            symbol.as_deref(),
            modifier.as_deref(),
            raw.as_deref(),
            dry_run,
            vid,
            pid,
        ),
    }
}

fn get_keymap(key: &str, layer: &str, vid: u16, pid: u16) -> Result<()> {
    let layout = PhysicalKeyboardLayout::new();
    let slot = layout.slot_for_name(key).ok_or_else(|| {
        anyhow::anyhow!(
            "Unknown physical key name \"{key}\" — run \"list-keys\" to see valid names."
        )
    })?;
    let layer_num = layer_from_arg(layer)?;

    let (api, device) = select_wired_device(vid, pid)?;
    eprintln!(
        "Connecting to {} [vid={vid:04x} pid={pid:04x}]",
        device.product.as_deref().unwrap_or("(unknown product)")
    );
    let session = WiredSession::open(&api, &device.path)?;
    let repo = KeyMatrixRepository::new(session);
    let serializer =
        KeymapYamlSerializer::new(KeyMappingCodec::new(), PhysicalKeyboardLayout::new());

    let buffer = repo.read_layer(layer_num)?;
    let offset = slot as usize * 4;
    let value = u32::from_be_bytes(buffer[offset..offset + 4].try_into().unwrap());

    let mut doc = serde_yaml_ng::Mapping::new();
    doc.insert(key.into(), serializer.describe_slot(value));
    println!(
        "{}",
        serde_yaml_ng::to_string(&serde_yaml_ng::Value::Mapping(doc))
            .expect("serializing a Mapping never fails")
    );
    Ok(())
}

fn get_macros(vid: u16, pid: u16) -> Result<()> {
    let (api, device) = select_wired_device(vid, pid)?;
    eprintln!(
        "Connecting to {} [vid={vid:04x} pid={pid:04x}]",
        device.product.as_deref().unwrap_or("(unknown product)")
    );
    let session = WiredSession::open(&api, &device.path)?;
    let repo = MacroRepository::new(session);

    eprintln!("Reading macro table…");
    let macros = repo.read_macros()?;

    if macros.is_empty() {
        println!("No macros defined.");
        return Ok(());
    }

    let codec = KeyMappingCodec::new();
    for (i, m) in macros.iter().enumerate() {
        println!("[{i}] \"{}\" ({} action(s)):", m.name, m.actions.len());
        for action in &m.actions {
            let verb = match action.edge {
                MacroEdge::Down => "press",
                MacroEdge::Up => "release",
            };
            let key_desc = match action.kind {
                MacroActionKind::NormalKey => codec
                    .keycode_symbol(action.key as u16)
                    .unwrap_or_else(|| format!("key({})", action.key)),
                other => format!("{other:?}({})", action.key),
            };
            if action.delay > 0 {
                println!("      delay {}ms", action.delay);
            }
            println!("      {verb} {key_desc}");
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn set_keymap(
    key: &str,
    layer: &str,
    label: Option<&str>,
    symbol: Option<&str>,
    modifier: Option<&str>,
    raw: Option<&str>,
    dry_run: bool,
    vid: u16,
    pid: u16,
) -> Result<()> {
    let layout = PhysicalKeyboardLayout::new();
    let codec = KeyMappingCodec::new();
    let slot = layout.slot_for_name(key).ok_or_else(|| {
        anyhow::anyhow!(
            "Unknown physical key name \"{key}\" — run \"list-keys\" to see valid names."
        )
    })?;
    let layer_num = layer_from_arg(layer)?;
    let new_value = encode_from_flags(&codec, raw, label, symbol, modifier)?;

    let (api, device) = select_wired_device(vid, pid)?;
    eprintln!(
        "Connecting to {} [vid={vid:04x} pid={pid:04x}]",
        device.product.as_deref().unwrap_or("(unknown product)")
    );
    let session = WiredSession::open(&api, &device.path)?;
    let repo = KeyMatrixRepository::new(session);

    let mut buffer = repo.read_layer(layer_num)?;
    let offset = slot as usize * 4;
    let old_value = u32::from_be_bytes(buffer[offset..offset + 4].try_into().unwrap());
    let before = codec.decode(old_value, None).label();
    let after = codec.decode(new_value, None).label();
    println!("{key}.{layer}: {before} -> {after}");

    if dry_run {
        eprintln!("Dry run — nothing was written.");
        return Ok(());
    }

    buffer[offset..offset + 4].copy_from_slice(&new_value.to_be_bytes());
    repo.write_layer(layer_num, &buffer)?;
    eprintln!("Written.");
    Ok(())
}

fn export_keymap(vid: u16, pid: u16, full: bool, out: Option<String>) -> Result<()> {
    let (api, device) = select_wired_device(vid, pid)?;
    eprintln!(
        "Connecting to {} [vid={vid:04x} pid={pid:04x}]",
        device.product.as_deref().unwrap_or("(unknown product)")
    );
    let session = WiredSession::open(&api, &device.path)?;
    let repo = KeyMatrixRepository::new(session);
    let serializer =
        KeymapYamlSerializer::new(KeyMappingCodec::new(), PhysicalKeyboardLayout::new());

    let mut buffers_by_layer = HashMap::new();
    for layer in [0u8, 1u8, 2u8] {
        eprintln!("Reading layer {layer}…");
        buffers_by_layer.insert(layer, repo.read_layer(layer)?);
    }
    let text = if full {
        serializer.dump_yaml(&buffers_by_layer)
    } else {
        serializer.dump_yaml_diff(&buffers_by_layer)
    };

    match out {
        Some(path) => {
            fs::write(&path, text)?;
            eprintln!("Wrote {path}");
        }
        None => println!("{text}"),
    }
    Ok(())
}

fn export_hcl(vid: u16, pid: u16, full: bool, out: Option<String>) -> Result<()> {
    let (api, device) = select_wired_device(vid, pid)?;
    eprintln!(
        "Connecting to {} [vid={vid:04x} pid={pid:04x}]",
        device.product.as_deref().unwrap_or("(unknown product)")
    );
    let yaml_serializer =
        KeymapYamlSerializer::new(KeyMappingCodec::new(), PhysicalKeyboardLayout::new());
    let hcl_exporter = HclExporter::new(KeyMappingCodec::new(), PhysicalKeyboardLayout::new());

    // Each session is scoped to its own block and dropped before the next one opens —
    // macOS's IOHIDDeviceOpen is exclusive by default and errors
    // ("exclusive access and device already open") if a second handle to the same
    // path is opened while the first is still alive. Windows/Linux tolerate multiple
    // concurrent opens, which is why this only ever surfaced on macOS.
    eprintln!("Reading macros…");
    let macros = {
        let macro_session = WiredSession::open(&api, &device.path)?;
        let macro_repo = MacroRepository::new(macro_session);
        macro_repo.read_macros()?
    };
    let macro_names: Vec<String> = macros.iter().map(|m| m.name.clone()).collect();

    let mut buffers_by_layer = HashMap::new();
    {
        let session = WiredSession::open(&api, &device.path)?;
        let repo = KeyMatrixRepository::new(session);
        for layer in [0u8, 1u8, 2u8] {
            eprintln!("Reading layer {layer}…");
            buffers_by_layer.insert(layer, repo.read_layer(layer)?);
        }
    }
    let mut text = if full {
        hcl_exporter.dump(&buffers_by_layer, &macro_names)
    } else {
        let baseline: HashMap<u8, Vec<u8>> = [0u8, 1u8, 2u8]
            .into_iter()
            .map(|layer| (layer, yaml_serializer.factory_default_buffer(layer)))
            .collect();
        hcl_exporter.dump_diff(&buffers_by_layer, &baseline, &macro_names)
    };

    text.push_str(&hcl_exporter.dump_macros(&macros));

    eprintln!("Reading LED colors…");
    let colors = {
        let led_session = WiredSession::open(&api, &device.path)?;
        let led_repo = LedColorRepository::new(led_session);
        led_repo.read_colors()?
    };
    text.push_str(&hcl_exporter.dump_lighting(&colors));

    match out {
        Some(path) => {
            fs::write(&path, text)?;
            eprintln!("Wrote {path}");
        }
        None => println!("{text}"),
    }
    Ok(())
}

fn import_keymap(file: &str, vid: u16, pid: u16, dry_run: bool) -> Result<()> {
    let text = fs::read_to_string(file)?;
    let serializer =
        KeymapYamlSerializer::new(KeyMappingCodec::new(), PhysicalKeyboardLayout::new());
    let slot_maps = serializer.parse_yaml(&text)?;

    let (api, device) = select_wired_device(vid, pid)?;
    eprintln!(
        "Connecting to {} [vid={vid:04x} pid={pid:04x}]",
        device.product.as_deref().unwrap_or("(unknown product)")
    );
    let session = WiredSession::open(&api, &device.path)?;
    let repo = KeyMatrixRepository::new(session);
    apply_slot_maps(&repo, &slot_maps, dry_run)
}

fn import_hcl(file: &str, vid: u16, pid: u16, dry_run: bool) -> Result<()> {
    let text = fs::read_to_string(file)?;
    // Parse and fully validate the whole document up front — bad colors, unknown keys or
    // malformed macros fail here, before touching any hardware.
    let config = HclConfig::parse(&text)?;

    let slot_maps = config.layer_slot_maps()?;
    let has_layers = !slot_maps.values().all(|m| m.is_empty());
    let has_lighting = config.lighting().is_some_and(|l| !l.colors.is_empty());

    let (api, device) = select_wired_device(vid, pid)?;
    eprintln!(
        "Connecting to {} [vid={vid:04x} pid={pid:04x}]",
        device.product.as_deref().unwrap_or("(unknown product)")
    );

    // Macros are written FIRST (before any macro-referencing KeyMatrix write) and, when
    // the file has at least one `macro` block, as a full-table replace — layer slots
    // that reference a macro by index assume the macro table on the device will match
    // this file's macro order once this step completes. A file with NO `macro` blocks
    // at all leaves the device's macro table untouched (mirrors `layer`'s "merge onto
    // what's there" behavior) — this is different from a file that defines macros: HCL
    // has no syntax for "here are zero macros, wipe the table", only "this file doesn't
    // mention macros at all".
    if config.macros().is_empty() {
        eprintln!("No macro blocks in file — leaving the device's macro table untouched.");
    } else {
        eprintln!("{} macro(s) to write.", config.macros().len());
        if !dry_run {
            let session = WiredSession::open(&api, &device.path)?;
            let repo = MacroRepository::new(session);
            eprintln!("Writing macro table…");
            repo.write_macros(&config.compiled_macros())?;
        }
    }

    if has_layers {
        let session = WiredSession::open(&api, &device.path)?;
        let repo = KeyMatrixRepository::new(session);
        apply_slot_maps(&repo, &slot_maps, dry_run)?;
    }

    if has_lighting {
        let session = WiredSession::open(&api, &device.path)?;
        let repo = LedColorRepository::new(session);
        apply_lighting(&repo, &config, dry_run)?;
    }

    Ok(())
}

/// Writes the `lighting.colors` map to the device, only if it actually differs from
/// what's currently there. Unlike KeyMatrix, there's no embedded factory-default to
/// merge against — untouched slots simply stay black (see `HclConfig::compile_lighting`
/// docs) — so the "current" read below is purely for the diff report and the
/// write-skip check, not a merge base.
fn apply_lighting(repo: &LedColorRepository, config: &HclConfig, dry_run: bool) -> Result<()> {
    let target = config.compile_lighting()?;

    eprintln!("Reading current LED colors…");
    let current = repo.read_colors()?;

    if target == current {
        eprintln!(
            "LED colors unchanged ({} color(s) already match).",
            config.lighting().map(|l| l.colors.len()).unwrap_or(0)
        );
        return Ok(());
    }

    let lighting = config.lighting().expect("has_lighting implies Some");
    eprintln!("{} LED color(s) to apply:", lighting.colors.len());
    for color in &lighting.colors {
        println!(
            "  {} -> rgb({}, {}, {})",
            color.key, color.rgb[0], color.rgb[1], color.rgb[2]
        );
    }

    if dry_run {
        return Ok(());
    }

    eprintln!("Entering SelfDefine mode…");
    repo.enter_self_define()?;
    // enter_self_define's last write is fire-and-forget (no response to wait on) —
    // give the device a moment to actually apply the mode change before the colour
    // write, or it was observed to accept the write but show no visible change.
    std::thread::sleep(std::time::Duration::from_millis(200));
    eprintln!("Writing LED colors…");
    repo.write_colors(&target)?;
    Ok(())
}

/// Merge the per-layer `{slot -> value}` maps produced by either the YAML or HCL
/// front-end onto the factory-default KeyMatrix and write the result to the device,
/// printing a per-key before/after diff (against what the device currently holds)
/// and a summary.
///
/// The merge base is the embedded factory-default dump, not the device's current
/// state: slots the config doesn't mention are reset to factory rather than left
/// however a previous import happened to leave them, so imports are reproducible
/// from the config alone instead of depending on prior device state.
fn apply_slot_maps(
    repo: &KeyMatrixRepository,
    slot_maps: &HashMap<u8, HashMap<u16, u32>>,
    dry_run: bool,
) -> Result<()> {
    let codec = KeyMappingCodec::new();
    let layout = PhysicalKeyboardLayout::new();
    let serializer =
        KeymapYamlSerializer::new(KeyMappingCodec::new(), PhysicalKeyboardLayout::new());

    let mut changed = 0u32;
    let mut unchanged = 0u32;
    for layer in [0u8, 1u8, 2u8] {
        let Some(slot_map) = slot_maps.get(&layer) else {
            continue;
        };

        eprintln!("Reading layer {layer}…");
        let current = repo.read_layer(layer)?;
        let mut target = serializer.factory_default_buffer(layer);
        serializer.patch_buffer(&mut target, slot_map);

        // Compares decoded labels (not raw values) to match the Node CLI exactly — a raw-only
        // change invisible in the label (e.g. a Macro's repeat count) is still written even
        // though it reports "unchanged". Diffed against the device's *current* buffer so the
        // report reflects what will actually change on the hardware, even though the merge
        // itself is computed against the factory default.
        for slot in 0..(target.len() / 4) as u16 {
            let offset = slot as usize * 4;
            let before = codec
                .decode(
                    u32::from_be_bytes(current[offset..offset + 4].try_into().unwrap()),
                    None,
                )
                .label();
            let after = codec
                .decode(
                    u32::from_be_bytes(target[offset..offset + 4].try_into().unwrap()),
                    None,
                )
                .label();
            if before == after {
                unchanged += 1;
                continue;
            }
            changed += 1;
            let layer_name = match layer {
                0 => "normal",
                1 => "fn",
                2 => "fn2",
                _ => unreachable!("only layers 0, 1 and 2 exist on the A72"),
            };
            println!(
                "  {}.{layer_name}: {before} -> {after}",
                layout.name_for_slot(slot)
            );
        }

        if dry_run || target == current {
            continue;
        }
        eprintln!("Writing layer {layer}…");
        repo.write_layer(layer, &target)?;
    }

    let summary = format!(
        "{changed} key{} changed{}",
        if changed == 1 { "" } else { "s" },
        if unchanged > 0 {
            format!(", {unchanged} unchanged")
        } else {
            String::new()
        }
    );
    if dry_run {
        eprintln!("Dry run — {summary}, nothing was written.");
    } else {
        eprintln!("Keymap applied — {summary}.");
    }
    Ok(())
}

fn visual_suffix(canonical: &str, visual: &str) -> String {
    if visual == canonical {
        String::new()
    } else {
        format!("  (was: {visual})")
    }
}

fn list_keys() -> Result<()> {
    let codec = KeyMappingCodec::new();
    let layout = PhysicalKeyboardLayout::new();

    println!("Physical keys (name -> KeyMatrix slot):");
    for (name, slot, visual) in layout.list_named() {
        println!("  {name:<14} slot {slot}{}", visual_suffix(&name, &visual));
    }

    println!();
    println!("KeyBoard \"key\" symbols (standard USB HID keyboard usage page):");
    for (code, symbol, visual) in codec.list_keycode_symbols() {
        println!(
            "  {symbol:<14} code {code}{}",
            visual_suffix(&symbol, &visual)
        );
    }

    println!();
    println!("KeyBoard \"mod\" names (combine multiple with \"+\", e.g. LCtrl+LShift):");
    for (bit, name) in codec.list_modifier_names() {
        println!("  {name:<14} bit {bit}");
    }

    println!();
    println!("Non-KeyBoard \"label\" values (for type/label entries, e.g. SpecialFun):");
    for (label, raw, visual) in codec.list_labels() {
        let type_name = KeyMappingType::from_byte((raw >> 24) as u8).type_name();
        println!(
            "  {:<20} {type_name}{}",
            label,
            visual_suffix(&label, &visual)
        );
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The clap defaults are string literals; this pins them to the library constants so
    /// changing one without the other can't ship a default the parser itself rejects.
    #[test]
    fn defaults_match_supported_ids() {
        assert_eq!(parse_supported_vid(DEFAULT_VID), Ok(SUPPORTED_VENDOR_ID));
        assert_eq!(parse_supported_pid(DEFAULT_PID), Ok(SUPPORTED_PRODUCT_ID));
    }

    #[test]
    fn rejects_other_devices() {
        assert!(parse_supported_vid("1234").is_err());
        assert!(parse_supported_pid("005e").is_err());
    }

    #[test]
    fn cli_parses_defaults_and_rejects_overrides() {
        use clap::Parser;

        assert!(Cli::try_parse_from(["rk-a72", "export-keymap"]).is_ok());
        assert!(Cli::try_parse_from(["rk-a72", "export-keymap", "--pid", "005e"]).is_err());
        assert!(Cli::try_parse_from(["rk-a72", "export-keymap", "--vid", "1234"]).is_err());
    }
}
