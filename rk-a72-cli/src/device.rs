use anyhow::{Context, Result};
use hidapi::HidApi;
use rk_a72_keymap::{find_wired_device, DeviceMatch};

pub fn select_wired_device(vid: u16, pid: u16) -> Result<(HidApi, DeviceMatch)> {
    let api = HidApi::new().context("failed to initialize HID API")?;
    let device = find_wired_device(&api, vid, pid).with_context(|| {
        format!(
            "No wired command interface found for vid={vid:04x} pid={pid:04x}. \
             Is the keyboard connected via USB cable?"
        )
    })?;
    Ok((api, device))
}
