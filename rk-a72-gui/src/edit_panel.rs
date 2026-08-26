use eframe::egui;
use rk_a72_keymap::{KeyMappingCodec, KeyMappingType};

use crate::app::GuiApp;

#[derive(Debug, Clone, PartialEq)]
pub enum CandidateKind {
    KeyBoardSymbol,
    Label { type_name: String },
}

#[derive(Debug, Clone, PartialEq)]
pub struct Candidate {
    /// The canonical name — what gets typed and what gets sent to `encode_*`.
    pub display: String,
    /// The old glyph, only `Some` when it differs from `display` (mirrors the
    /// CLI's `visual_suffix` — visual names are never accepted as input).
    pub visual: Option<String>,
    pub kind: CandidateKind,
}

pub fn build_candidates(codec: &KeyMappingCodec) -> Vec<Candidate> {
    let mut candidates: Vec<Candidate> = codec
        .list_labels()
        .into_iter()
        .map(|(canonical, raw, visual)| {
            let type_name = KeyMappingType::from_byte((raw >> 24) as u8)
                .type_name()
                .to_string();
            Candidate {
                visual: (visual != canonical).then_some(visual),
                display: canonical,
                kind: CandidateKind::Label { type_name },
            }
        })
        .collect();

    candidates.extend(codec.list_keycode_symbols().into_iter().map(
        |(_code, canonical, visual)| Candidate {
            visual: (visual != canonical).then_some(visual),
            display: canonical,
            kind: CandidateKind::KeyBoardSymbol,
        },
    ));

    candidates
}

pub fn filter_candidates<'a>(candidates: &'a [Candidate], query: &str) -> Vec<&'a Candidate> {
    let query = query.to_lowercase();
    candidates
        .iter()
        .filter(|c| c.display.to_lowercase().starts_with(&query))
        .collect()
}

/// Renders the right-hand side panel for the currently selected key. No-op if no
/// key is selected.
pub fn show(ctx: &egui::Context, app: &mut GuiApp) {
    let Some(slot) = app.selected_slot else {
        return;
    };
    let name = app.layout.name_for_slot(slot);
    let layer = app.active_layer_tab;

    // A write (single-slot edit or an in-progress import) already owns
    // `pending_write`/`import_queue`/`import_progress` — letting the user queue a
    // second, concurrent edit here would race with it (see write_in_flight's doc
    // comment on `GuiApp`), so the panel goes read-only until it resolves.
    let write_in_flight = app.write_in_flight();

    egui::SidePanel::right("edit_panel").min_width(260.0).show(ctx, |ui| {
        ui.heading(&name);
        let layer_name = match layer {
            0 => "normal",
            1 => "fn",
            2 => "fn2",
            _ => unreachable!("only layers 0, 1 and 2 exist on the A72"),
        };
        ui.label(format!("Editing layer: {layer_name}"));
        ui.separator();

        ui.add_enabled_ui(!write_in_flight, |ui| {
            if write_in_flight {
                ui.label("Write in progress — waiting before allowing another edit…");
            }

            ui.label("Type to search KeyBoard symbols and labels:");
            ui.text_edit_singleline(&mut app.edit_query);

            let candidates = build_candidates(&app.codec);
            let matches = filter_candidates(&candidates, &app.edit_query);

            egui::ScrollArea::vertical().max_height(300.0).show(ui, |ui| {
                for candidate in matches {
                    let hint = candidate
                        .visual
                        .as_deref()
                        .map(|v| format!("  (was: {v})"))
                        .unwrap_or_default();
                    let type_hint = match &candidate.kind {
                        CandidateKind::KeyBoardSymbol => "KeyBoard".to_string(),
                        CandidateKind::Label { type_name } => type_name.clone(),
                    };
                    let text = format!("{}{hint}  [{type_hint}]", candidate.display);
                    if ui.selectable_label(false, text).clicked() && !write_in_flight {
                        let value = match &candidate.kind {
                            CandidateKind::KeyBoardSymbol => {
                                let key_code = app
                                    .codec
                                    .symbol_to_keycode(&candidate.display)
                                    .expect("candidate symbol must resolve — it came from list_keycode_symbols()");
                                rk_a72_keymap::KeyMappingCodec::encode_keyboard(
                                    key_code,
                                    rk_a72_keymap::ModifierSet::empty(),
                                )
                            }
                            CandidateKind::Label { .. } => {
                                let raw = app
                                    .codec
                                    .label_to_raw(&candidate.display)
                                    .expect("candidate label must resolve — it came from list_labels()");
                                rk_a72_keymap::KeyMappingCodec::encode_raw(raw)
                            }
                        };
                        app.write_slot(slot, layer, value);
                    }
                }
            });
        });
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_candidates_includes_both_keyboard_symbols_and_labels() {
        let codec = KeyMappingCodec::new();
        let candidates = build_candidates(&codec);

        let a = candidates
            .iter()
            .find(|c| c.display == "A")
            .expect("KeyBoard symbol A");
        assert_eq!(a.kind, CandidateKind::KeyBoardSymbol);
        assert_eq!(a.visual, None);

        let mute = candidates
            .iter()
            .find(|c| c.display == "Mute")
            .expect("label Mute");
        assert_eq!(
            mute.kind,
            CandidateKind::Label {
                type_name: "Media".to_string()
            }
        );

        let backtick = candidates
            .iter()
            .find(|c| c.display == "Backtick")
            .expect("KeyBoard symbol Backtick");
        assert_eq!(backtick.visual.as_deref(), Some("`"));
    }

    #[test]
    fn build_candidates_does_not_include_physical_key_names() {
        let codec = KeyMappingCodec::new();
        let candidates = build_candidates(&codec);
        assert!(
            !candidates.iter().any(|c| c.display == "Digit1"),
            "physical key names are a separate namespace from --symbol/--label values"
        );
    }

    #[test]
    fn filter_candidates_matches_by_case_insensitive_prefix() {
        let codec = KeyMappingCodec::new();
        let candidates = build_candidates(&codec);

        let results = filter_candidates(&candidates, "mu");
        assert!(results.iter().any(|c| c.display == "Mute"));

        let results = filter_candidates(&candidates, "MU");
        assert!(results.iter().any(|c| c.display == "Mute"));

        let results = filter_candidates(&candidates, "NotAPrefixThatMatchesAnything");
        assert!(results.is_empty());
    }

    #[test]
    fn filter_candidates_with_empty_query_returns_everything() {
        let codec = KeyMappingCodec::new();
        let candidates = build_candidates(&codec);
        let results = filter_candidates(&candidates, "");
        assert_eq!(results.len(), candidates.len());
    }
}
