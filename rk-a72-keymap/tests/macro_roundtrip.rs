//! Real-hardware round-trip test for `SetMacros`/`GetMacros`, mirroring
//! `led_roundtrip.rs`'s KeyMatrix/LED tests. Requires a real A72 connected via USB.
//! `#[ignore]` by default; run with `cargo test -p rk-a72-keymap -- --ignored macro`.

use hidapi::HidApi;
use rk_a72_keymap::macros::{Macro, MacroAction, MacroActionKind, MacroEdge};
use rk_a72_keymap::{find_wired_device, MacroRepository, WiredSession};

const VID: u16 = 0x258a;
const PID: u16 = 0x0216;

#[test]
#[ignore]
fn macro_table_round_trips_on_real_a72_hardware() {
    let api = HidApi::new().expect("failed to initialize HID API");
    let device = find_wired_device(&api, VID, PID)
        .expect("no wired A72 device found — connect via USB to run this test");
    println!("[e2e] connected: {:?}", device.product);

    let session = WiredSession::open(&api, &device.path).expect("failed to open device");
    let repo = MacroRepository::new(session);

    println!("[e2e] reading current macro table (snapshot to restore afterward)...");
    let original = repo.read_macros().expect("failed to read macro table");

    let test_macros = vec![Macro {
        name: "RoundTripTest".to_string(),
        actions: vec![
            MacroAction { edge: MacroEdge::Down, kind: MacroActionKind::NormalKey, delay: 0, key: 4 }, // 'A'
            MacroAction { edge: MacroEdge::Up, kind: MacroActionKind::NormalKey, delay: 50, key: 4 },
        ],
    }];

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        println!("[e2e] writing test macro table...");
        repo.write_macros(&test_macros)
            .expect("failed to write macro table");

        println!("[e2e] reading back...");
        let readback = repo.read_macros().expect("failed to read macro table back");
        assert_eq!(
            readback, test_macros,
            "macro table round-tripped wrong: got {readback:?}, want {test_macros:?}"
        );
        println!("[e2e] macro table round-tripped correctly.");
    }));

    println!("[e2e] restoring original macro table...");
    repo.write_macros(&original)
        .expect("failed to restore original macro table");
    let restored = repo.read_macros().expect("failed to read back restored macro table");
    assert_eq!(
        restored, original,
        "FAILED TO RESTORE the original macro table after the test — the keyboard may be \
         left in the test's temporary state. Re-open the official configurator to check, \
         and re-apply your real macros if so."
    );
    println!("[e2e] restored and verified — macro table matches the pre-test snapshot.");

    result.expect("test assertions failed (see panic above) — restore already completed");
}
