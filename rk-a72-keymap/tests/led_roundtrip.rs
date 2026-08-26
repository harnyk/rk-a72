//! Real-hardware round-trip test for `SetLedColors`/`GetLedColors`, mirroring
//! `hardware_roundtrip.rs`'s KeyMatrix test. Requires a real A72 connected via USB.
//! `#[ignore]` by default; run with `cargo test -p rk-a72-keymap -- --ignored led`.

use hidapi::HidApi;
use rk_a72_keymap::{find_wired_device, LedColorRepository, WiredSession};

const VID: u16 = 0x258a;
const PID: u16 = 0x0216;
const LED_COLORS_SLOT_COUNT: usize = 126;

#[test]
#[ignore]
fn led_colors_round_trip_on_real_a72_hardware() {
    // Slots 15 (A) and 16 (IntlBackslash, no physical LED on this ANSI board) — one
    // slot we expect to actually light, one we expect the write to accept but the
    // read-back to still show whatever the no-LED slot always reports.
    let a_slot = 15usize;

    let api = HidApi::new().expect("failed to initialize HID API");
    let device = find_wired_device(&api, VID, PID)
        .expect("no wired A72 device found — connect via USB to run this test");
    println!("[e2e] connected: {:?}", device.product);

    let session = WiredSession::open(&api, &device.path).expect("failed to open device");
    let repo = LedColorRepository::new(session);

    println!("[e2e] reading current LED colors (snapshot to restore afterward)...");
    let original = repo.read_colors().expect("failed to read LED colors");

    let mut working = vec![0u8; original.len()];
    working[a_slot] = 255; // R plane: slot A -> pure red

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        println!("[e2e] entering SelfDefine mode (SetProfile mode-select)...");
        repo.enter_self_define()
            .expect("failed to enter SelfDefine mode");

        println!("[e2e] writing test colors (slot {a_slot} = red, rest = black)...");
        repo.write_colors(&working)
            .expect("failed to write LED colors");

        println!("[e2e] reading back...");
        let readback = repo.read_colors().expect("failed to read LED colors back");
        assert_eq!(
            readback.len(),
            working.len(),
            "GetLedColors returned {} bytes, expected {}",
            readback.len(),
            working.len()
        );
        assert_eq!(
            readback[a_slot], 255,
            "slot {a_slot} (A) R-plane round-tripped wrong: got {}, want 255",
            readback[a_slot]
        );
        assert_eq!(
            readback[a_slot + LED_COLORS_SLOT_COUNT],
            0,
            "slot {a_slot} (A) G-plane should be 0"
        );
        assert_eq!(
            readback[a_slot + LED_COLORS_SLOT_COUNT * 2],
            0,
            "slot {a_slot} (A) B-plane should be 0"
        );
        println!("[e2e] slot {a_slot} (A) round-tripped correctly as pure red.");
    }));

    println!("[e2e] restoring original LED colors...");
    repo.write_colors(&original)
        .expect("failed to restore original LED colors");
    let restored = repo
        .read_colors()
        .expect("failed to read back restored LED colors");
    assert_eq!(
        restored, original,
        "FAILED TO RESTORE the original LED colors after the test — the keyboard may be \
         left in the test's temporary state. Re-open the official configurator to check, \
         and re-apply your real lighting settings if so."
    );
    println!("[e2e] restored and verified — LED colors match the pre-test snapshot.");

    result.expect("test assertions failed (see panic above) — restore already completed");
}
