use std::collections::{HashMap, VecDeque};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread::JoinHandle;

use hidapi::HidApi;
use rk_a72_keymap::{
    KeyMappingCodec, KeyMatrixRepository, KeymapYamlSerializer, PhysicalKeyboardLayout,
    WiredSession, SUPPORTED_PRODUCT_ID as PID, SUPPORTED_VENDOR_ID as VID,
};

use crate::worker::{spawn_worker, GuiCommand, GuiEvent};

/// Slot maps parsed from an imported YAML file: layer -> (slot -> raw value).
type SlotMaps = HashMap<u8, HashMap<u16, u32>>;

pub enum ConnectionState {
    Disconnected,
    Connecting,
    Ready,
}

/// Tracks one in-flight single-slot edit so a failed write can be reverted without
/// re-reading the device — `previous_value` is what was at `slot`/`layer` in the
/// local buffer immediately before this edit was applied.
pub struct PendingWrite {
    pub slot: u16,
    pub layer: u8,
    pub previous_value: u32,
}

pub struct GuiApp {
    pub state: ConnectionState,
    pub codec: KeyMappingCodec,
    pub layout: PhysicalKeyboardLayout,
    pub layer_buffers: HashMap<u8, Vec<u8>>,
    pub selected_slot: Option<u16>,
    pub active_layer_tab: u8,
    pub edit_query: String,
    pub pending_write: Option<PendingWrite>,
    pub status_message: Option<String>,
    pub import_preview: Option<(crate::file_ops::ImportSummary, SlotMaps)>,
    /// Layers still waiting to be written as part of an in-progress Import — every
    /// changed slot for a layer is patched into that layer's buffer up front (see
    /// `start_import`), so only one whole-layer write per changed layer is queued
    /// here. Writes are sent one at a time (see `advance_import`), never all at
    /// once, so a failure partway through stops the rest instead of racing
    /// multiple writes.
    import_queue: VecDeque<u8>,
    /// (layers written, layers queued) for the current import, shown in status
    /// messages. `None` when no import is in progress.
    import_progress: Option<(u32, u32)>,
    cmd_tx: Option<Sender<GuiCommand>>,
    event_rx: Option<Receiver<GuiEvent>>,
    worker_handle: Option<JoinHandle<()>>,
}

impl Default for GuiApp {
    fn default() -> Self {
        Self {
            state: ConnectionState::Disconnected,
            codec: KeyMappingCodec::new(),
            layout: PhysicalKeyboardLayout::new(),
            layer_buffers: HashMap::new(),
            selected_slot: None,
            active_layer_tab: 0,
            edit_query: String::new(),
            pending_write: None,
            status_message: None,
            import_preview: None,
            import_queue: VecDeque::new(),
            import_progress: None,
            cmd_tx: None,
            event_rx: None,
            worker_handle: None,
        }
    }
}

impl GuiApp {
    /// Enumerates for the device, opens a session, and spawns the worker thread.
    /// Cheap enough (HID enumeration only, no per-request delay) to call directly
    /// on the UI thread from the "Retry" button — the 150ms-per-request cost only
    /// applies to the ReadLayer/WriteLayer commands the worker executes.
    pub fn try_connect(&mut self) {
        self.status_message = None;
        let api = match HidApi::new() {
            Ok(api) => api,
            Err(e) => {
                self.status_message = Some(format!("HID init failed: {e}"));
                return;
            }
        };
        let Some(device) = rk_a72_keymap::find_wired_device(&api, VID, PID) else {
            self.status_message = Some("No A72 found — is it connected via USB?".to_string());
            return;
        };
        let session = match WiredSession::open(&api, &device.path) {
            Ok(s) => s,
            Err(e) => {
                self.status_message = Some(format!("Failed to open device: {e}"));
                return;
            }
        };
        let repo = KeyMatrixRepository::new(session);
        let (cmd_tx, cmd_rx) = mpsc::channel();
        let (event_tx, event_rx) = mpsc::channel();
        self.worker_handle = Some(spawn_worker(repo, cmd_rx, event_tx));
        self.layer_buffers.clear();
        let _ = cmd_tx.send(GuiCommand::ReadLayer(0));
        let _ = cmd_tx.send(GuiCommand::ReadLayer(1));
        let _ = cmd_tx.send(GuiCommand::ReadLayer(2));
        self.cmd_tx = Some(cmd_tx);
        self.event_rx = Some(event_rx);
        self.state = ConnectionState::Connecting;
    }

    /// Drains every event the worker has sent since the last call. Called once per
    /// egui frame from `update()`.
    fn drain_events(&mut self) {
        let Some(event_rx) = &self.event_rx else {
            return;
        };
        let events: Vec<GuiEvent> = event_rx.try_iter().collect();
        let mut lost = false;
        for event in events {
            match event {
                GuiEvent::LayerLoaded { layer, buffer } => {
                    self.layer_buffers.insert(layer, buffer);
                    if self.layer_buffers.len() == 3 {
                        self.state = ConnectionState::Ready;
                    }
                }
                GuiEvent::LayerWritten { ok, .. } => {
                    if self.import_progress.is_some() {
                        if ok {
                            if let Some((applied, total)) = self.import_progress {
                                self.import_progress = Some((applied + 1, total));
                            }
                            self.advance_import();
                        } else if let Some((applied, total)) = self.import_progress.take() {
                            self.import_queue.clear();
                            self.status_message = Some(format!(
                                "Import stopped after a write failed — {applied}/{total} layers applied."
                            ));
                        }
                    } else if ok {
                        self.pending_write = None;
                    } else if let Some(pending) = self.pending_write.take() {
                        self.revert_pending(&pending);
                        self.status_message = Some("Write failed — reverted.".to_string());
                    }
                }
                GuiEvent::DeviceLost => {
                    lost = true;
                }
            }
        }
        if lost {
            self.disconnect("Device disconnected.".to_string());
        }
    }

    fn revert_pending(&mut self, pending: &PendingWrite) {
        if let Some(buffer) = self.layer_buffers.get_mut(&pending.layer) {
            let offset = pending.slot as usize * 4;
            buffer[offset..offset + 4].copy_from_slice(&pending.previous_value.to_be_bytes());
        }
    }

    /// Sends the next queued import write, if any. Called after every successful
    /// write while an import is in progress, and once to kick the first write off.
    /// Per-spec: import writes are sequential, never concurrent, so a failure
    /// partway through leaves the queue (and the rest of the device) untouched
    /// rather than racing further writes against an already-failed session.
    fn advance_import(&mut self) {
        if let Some(layer) = self.import_queue.pop_front() {
            let Some(buffer) = self.layer_buffers.get(&layer).cloned() else {
                return;
            };
            if let Some(cmd_tx) = &self.cmd_tx {
                let _ = cmd_tx.send(GuiCommand::WriteLayer { layer, buffer });
            }
        } else if let Some((_, total)) = self.import_progress.take() {
            self.status_message = Some(format!("Import complete — {total} layers applied."));
        }
    }

    /// Patches every changed slot from an accepted Import preview directly into
    /// the in-memory layer buffers, then queues exactly one whole-layer write per
    /// layer that had any changes (the device has no slot-level write opcode, so
    /// writing once per changed layer — instead of once per changed slot — avoids
    /// dozens of redundant full-layer HID writes). `slot_maps` is the full parsed
    /// YAML map (from `KeymapYamlSerializer::parse_yaml`); only the slots
    /// `summary.entries` marked as actually changed are patched in.
    fn start_import(&mut self, summary: &crate::file_ops::ImportSummary, slot_maps: &SlotMaps) {
        let mut layers_to_write: Vec<u8> = Vec::new();
        for entry in &summary.entries {
            let Some(slot) = self.layout.slot_for_name(&entry.name) else {
                continue;
            };
            let Some(&value) = slot_maps.get(&entry.layer).and_then(|m| m.get(&slot)) else {
                continue;
            };
            let Some(buffer) = self.layer_buffers.get_mut(&entry.layer) else {
                continue;
            };
            let offset = slot as usize * 4;
            buffer[offset..offset + 4].copy_from_slice(&value.to_be_bytes());
            if !layers_to_write.contains(&entry.layer) {
                layers_to_write.push(entry.layer);
            }
        }
        self.import_progress = Some((0, layers_to_write.len() as u32));
        self.import_queue = layers_to_write.into();
        self.advance_import();
    }

    fn disconnect(&mut self, message: String) {
        self.state = ConnectionState::Disconnected;
        self.cmd_tx = None;
        self.event_rx = None;
        self.worker_handle = None;
        self.layer_buffers.clear();
        self.pending_write = None;
        self.import_queue.clear();
        self.import_progress = None;
        self.status_message = Some(match self.status_message.take() {
            Some(existing) => format!("{existing} {message}"),
            None => message,
        });
    }

    /// Patches `value` into the local buffer for `slot`/`layer` and sends the
    /// resulting full buffer to the worker for writing. Remembers the previous
    /// value so a failed write can be reverted in `drain_events`.
    pub fn write_slot(&mut self, slot: u16, layer: u8, value: u32) {
        let Some(buffer) = self.layer_buffers.get_mut(&layer) else {
            return;
        };
        let offset = slot as usize * 4;
        let previous_value = u32::from_be_bytes(buffer[offset..offset + 4].try_into().unwrap());
        buffer[offset..offset + 4].copy_from_slice(&value.to_be_bytes());
        self.pending_write = Some(PendingWrite {
            slot,
            layer,
            previous_value,
        });

        if let Some(cmd_tx) = &self.cmd_tx {
            let _ = cmd_tx.send(GuiCommand::WriteLayer {
                layer,
                buffer: buffer.clone(),
            });
        }
    }

    pub fn yaml_serializer(&self) -> KeymapYamlSerializer {
        KeymapYamlSerializer::new(KeyMappingCodec::new(), PhysicalKeyboardLayout::new())
    }

    /// True while a single-slot edit or an import batch has an outstanding write
    /// the worker hasn't confirmed yet. `pending_write` and `import_queue`/
    /// `import_progress` are shared, unlocked state keyed on "the one write in
    /// flight" — starting a second write (a manual edit clicked mid-import, or a
    /// second import started before the first finishes) would silently clobber
    /// whichever one is tracked in these fields, so callers must check this before
    /// offering another write and gate the UI accordingly (see `edit_panel::show`
    /// and the "Import from file…" menu button in `update()`).
    pub fn write_in_flight(&self) -> bool {
        self.pending_write.is_some() || self.import_progress.is_some()
    }
}

impl eframe::App for GuiApp {
    fn update(&mut self, ctx: &eframe::egui::Context, _frame: &mut eframe::Frame) {
        self.drain_events();

        match self.state {
            ConnectionState::Disconnected => {
                eframe::egui::CentralPanel::default().show(ctx, |ui| {
                    ui.heading("RK A72 Keymap Editor");
                    if let Some(msg) = &self.status_message {
                        ui.label(msg);
                    } else {
                        ui.label("Keyboard not connected.");
                    }
                    if ui.button("Retry").clicked() {
                        self.try_connect();
                    }
                });
            }
            ConnectionState::Connecting => {
                eframe::egui::CentralPanel::default().show(ctx, |ui| {
                    ui.label("Connecting…");
                });
                ctx.request_repaint();
            }
            ConnectionState::Ready => {
                // A second import started while the first is still writing (or a
                // manual edit started while an import is writing) would clobber
                // `pending_write`/`import_queue`/`import_progress` out from under
                // whichever write is already in flight — see `write_in_flight`'s
                // doc comment. Starting a NEW import is the only thing gated here;
                // Export just reads the current buffers, so it's always safe.
                let write_in_flight = self.write_in_flight();
                eframe::egui::TopBottomPanel::top("menu_bar").show(ctx, |ui| {
                    eframe::egui::menu::bar(ui, |ui| {
                        if ui.button("Export to file…").clicked() {
                            crate::file_ops::export_to_file(self);
                        }
                        let import_button = eframe::egui::Button::new("Import from file…");
                        if ui.add_enabled(!write_in_flight, import_button).clicked() {
                            if let Some(path) = crate::file_ops::pick_import_file() {
                                match std::fs::read_to_string(&path) {
                                    Ok(text) => match self.yaml_serializer().parse_yaml(&text) {
                                        Ok(slot_maps) => {
                                            let summary = crate::file_ops::build_import_summary(
                                                &self.codec,
                                                &self.layout,
                                                &self.layer_buffers,
                                                &slot_maps,
                                            );
                                            self.import_preview = Some((summary, slot_maps));
                                        }
                                        Err(e) => {
                                            self.status_message = Some(format!(
                                                "Failed to parse {}: {e}",
                                                path.display()
                                            ));
                                        }
                                    },
                                    Err(e) => {
                                        self.status_message =
                                            Some(format!("Failed to read {}: {e}", path.display()));
                                    }
                                }
                            }
                        }
                    });
                });
                if let Some(msg) = &self.status_message {
                    eframe::egui::TopBottomPanel::top("status_bar").show(ctx, |ui| {
                        ui.label(msg);
                    });
                }
                crate::edit_panel::show(ctx, self);
                crate::keyboard_view::show(ctx, self);
                crate::app::show_import_preview(ctx, self);
                ctx.request_repaint();
            }
        }
    }
}

/// Shows the Import confirmation dialog when `app.import_preview` is `Some`. Apply
/// queues every changed slot and starts writing them one at a time via
/// `start_import` (never all at once — see its doc comment); Cancel discards the
/// pending import with no device writes.
pub fn show_import_preview(ctx: &eframe::egui::Context, app: &mut GuiApp) {
    let Some((summary, slot_maps)) = app.import_preview.clone() else {
        return;
    };
    // Guards against the same class of race as `write_in_flight` everywhere else:
    // a manual edit could still be in flight from before this window opened, and
    // applying now would clobber `pending_write` with the import's queue.
    let write_in_flight = app.write_in_flight();
    let mut close = false;
    let mut apply = false;
    eframe::egui::Window::new("Import preview").show(ctx, |ui| {
        ui.label(format!(
            "{} key{} will change, {} unchanged",
            summary.entries.len(),
            if summary.entries.len() == 1 { "" } else { "s" },
            summary.unchanged
        ));
        for entry in &summary.entries {
            let layer_name = match entry.layer {
                0 => "normal",
                1 => "fn",
                2 => "fn2",
                _ => unreachable!("only layers 0, 1 and 2 exist on the A72"),
            };
            ui.label(format!(
                "{}.{}: {} -> {}",
                entry.name, layer_name, entry.before, entry.after
            ));
        }
        if write_in_flight {
            ui.label("A write is still in progress — waiting before this import can apply…");
        }
        ui.horizontal(|ui| {
            if ui
                .add_enabled(!write_in_flight, eframe::egui::Button::new("Apply"))
                .clicked()
            {
                apply = true;
            }
            if ui.button("Cancel").clicked() {
                close = true;
            }
        });
    });
    if apply {
        app.start_import(&summary, &slot_maps);
        close = true;
    }
    if close {
        app.import_preview = None;
    }
}
