use eframe::egui;

use crate::app::GuiApp;
use crate::layout_grid::grid_position;

const CELL_SIZE: f32 = 56.0;

/// Renders the physical keyboard grid in the central panel. Each key is a button
/// showing its active-layer decoded label; clicking selects it for editing.
pub fn show(ctx: &egui::Context, app: &mut GuiApp) {
    egui::CentralPanel::default().show(ctx, |ui| {
        ui.horizontal(|ui| {
            ui.label("Layer:");
            ui.selectable_value(&mut app.active_layer_tab, 0, "normal");
            ui.selectable_value(&mut app.active_layer_tab, 1, "fn");
            ui.selectable_value(&mut app.active_layer_tab, 2, "fn2");
        });
        ui.separator();

        let active_layer = app.active_layer_tab;
        let Some(buffer) = app.layer_buffers.get(&active_layer).cloned() else {
            ui.label("Loading…");
            return;
        };

        // `list_named()` is ordered by KeyMatrix slot number, which is independent
        // of (and not monotonic with) the visual grid's (row, col) order — sort by
        // (row, col) first so `end_row()` below only ever advances forward.
        let mut keys: Vec<(String, u16, u8, u8)> = app
            .layout
            .list_named()
            .into_iter()
            .filter_map(|(name, slot, _visual)| {
                grid_position(&name).map(|(row, col)| (name, slot, row, col))
            })
            .collect();
        keys.sort_by_key(|(_, _, row, col)| (*row, *col));

        egui::Grid::new("keyboard_grid")
            .spacing([4.0, 4.0])
            .show(ui, |ui| {
                let mut current_row = 0u8;
                for (name, slot, row, _col) in keys {
                    while current_row < row {
                        ui.end_row();
                        current_row += 1;
                    }
                    let offset = slot as usize * 4;
                    let value = u32::from_be_bytes(buffer[offset..offset + 4].try_into().unwrap());
                    let label = app.codec.decode(value, None).label();
                    let selected = app.selected_slot == Some(slot);
                    let button = egui::Button::new(format!("{name}\n{label}"))
                        .min_size(egui::vec2(CELL_SIZE, CELL_SIZE))
                        .selected(selected);
                    if ui.add(button).clicked() {
                        app.selected_slot = Some(slot);
                        app.edit_query.clear();
                    }
                }
                ui.end_row();
            });
    });
}
