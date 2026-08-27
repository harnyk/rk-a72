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
