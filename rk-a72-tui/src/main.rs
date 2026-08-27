//! `rk-a72-tui`: an interactive terminal viewer for the RK A72's keymap and LED state.
//!
//! Current build is a **read-only viewer**. Users can browse the Keymap and LED tabs, move
//! the cursor across keys, cycle layers, and see current state — including the
//! dirty/customized colour-coding once an editing path exists to produce it. There is no
//! in-UI editing path yet: the action-edit dialog (`ui::keymap_tab::ActionDialogState`)
//! and the colour-edit widget (`color_input::ColorInput`) are built but not wired to any
//! key handler, so nothing in the running UI can mark a keymap or LED slot dirty. As a
//! result, `Ctrl+S`/Save always has an unmodified working state to write back. Wiring up
//! the interactive dialogs is future work.

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

    // Exactly one WiredSession::open call for this device path: WiredSession wraps its
    // HidDevice in an Rc internally, so cloning it (cheap — no second `open_path` call)
    // lets both repositories share the single open handle. Opening a second concurrent
    // handle to the same path fails on macOS (IOHIDDeviceOpen is exclusive there) — see
    // the same constraint documented in rk-a72-cli/src/main.rs.
    let session = WiredSession::open(&api, &device.path).context("failed to open device")?;
    let keymap_repo = KeyMatrixRepository::new(session.clone());
    let led_repo = LedColorRepository::new(session);

    let mut device_keymap = std::collections::HashMap::new();
    for layer in 0u8..3 {
        let buffer = keymap_repo.read_layer(layer).context("failed to read keymap layer")?;
        let mut slot_map = std::collections::HashMap::new();
        let (chunks, _remainder) = buffer.as_chunks::<4>();
        for (slot, chunk) in chunks.iter().enumerate() {
            let value = u32::from_be_bytes(*chunk);
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
