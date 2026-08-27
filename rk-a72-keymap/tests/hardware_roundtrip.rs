//! Real-hardware round-trip test. Requires a real A72 connected via USB.
//! `#[ignore]` by default; run with `cargo test -- --ignored`.

use hidapi::HidApi;
use rk_a72_keymap::{find_wired_device, KeyMappingCodec, KeyMatrixRepository, WiredSession};

const VID: u16 = 0x258a;
const PID: u16 = 0x0216;

struct Case {
    slot: u16,
    name: &'static str,
    raw: u32,
}

#[test]
#[ignore]
fn keymap_round_trip_on_real_a72_hardware() {
    let codec = KeyMappingCodec::new();

    // One slot per representable KeyMappingType, all in the media-key cluster (or the
    // ISO key) — chosen so a failed restore leaves the least disruptive keyboard in
    // the worst case.
    let cases = vec![
        Case {
            slot: 16,
            name: "IntlBackslash",
            raw: KeyMappingCodec::encode_keyboard(104, rk_a72_keymap::ModifierSet::empty()), // F13
        },
        Case {
            slot: 105,
            name: "PrevTr",
            raw: codec
                .label_to_raw("NextTr")
                .expect("NextTr label must exist"),
        },
        Case {
            slot: 106,
            name: "PlayPause",
            raw: codec.label_to_raw("BT0").expect("BT0 label must exist"),
        },
        Case {
            slot: 107,
            name: "NextTr",
            raw: codec
                .label_to_raw("mouseKey.leftKey")
                .expect("mouseKey.leftKey label must exist"),
        },
    ];

    let api = HidApi::new().expect("failed to initialize HID API");
    let device = find_wired_device(&api, VID, PID)
        .expect("no wired A72 device found — connect via USB to run this test");
    println!("[e2e] connected: {:?}", device.product);

    let session = WiredSession::open(&api, &device.path).expect("failed to open device");
    let repo = KeyMatrixRepository::new(session);
    let layer = 0u8;

    println!("[e2e] reading layer {layer} (snapshot to restore afterward)...");
    let original = repo.read_layer(layer).expect("failed to read layer");
    let mut working = original.clone();
    for case in &cases {
        let offset = case.slot as usize * 4;
        let before = codec
            .decode(
                u32::from_be_bytes(original[offset..offset + 4].try_into().unwrap()),
                None,
            )
            .label();
        let after = codec.decode(case.raw, None).label();
        println!(
            "[e2e]   slot {} ({}): {before} -> {after} (test value)",
            case.slot, case.name
        );
        working[offset..offset + 4].copy_from_slice(&case.raw.to_be_bytes());
    }

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        println!("[e2e] writing test values...");
        repo.write_layer(layer, &working)
            .expect("failed to write layer");
        println!("[e2e] reading back...");
        let readback = repo.read_layer(layer).expect("failed to read layer back");
        for case in &cases {
            let offset = case.slot as usize * 4;
            let got = u32::from_be_bytes(readback[offset..offset + 4].try_into().unwrap());
            let decoded = codec.decode(got, None);
            println!(
                "[e2e]   slot {} ({}): read back 0x{got:08x} ({})",
                case.slot,
                case.name,
                decoded.label()
            );
            assert_eq!(
                got,
                case.raw,
                "slot {} ({}) round-tripped wrong: got 0x{got:08x} ({}), want 0x{:08x}",
                case.slot,
                case.name,
                decoded.label(),
                case.raw
            );
        }
        println!("[e2e] all {} slots round-tripped correctly.", cases.len());
    }));

    println!("[e2e] restoring original layer {layer}...");
    repo.write_layer(layer, &original)
        .expect("failed to restore original layer");
    let restored = repo
        .read_layer(layer)
        .expect("failed to read back restored layer");
    assert_eq!(
        restored, original,
        "FAILED TO RESTORE the original layer-{layer} keymap after the test — the keyboard \
         may be left in the test's temporary state. Re-run export-hcl to check, and \
         re-apply your real keymap.hcl with import-hcl if so."
    );
    println!("[e2e] restored and verified — layer {layer} matches the pre-test snapshot.");

    result.expect("test assertions failed (see panic above) — restore already completed");
}
