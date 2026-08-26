pub const MACRO_ACTION_LEN: usize = 4;
pub const MACRO_BUFFER_LEN: usize = 4096;
pub const MACRO_PAGE_LEN: usize = 512;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MacroEdge {
    Down,
    Up,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MacroActionKind {
    NormalKey,
    ModifyKey,
    MouseKey,
    MouseCursorX,
    MouseCursorY,
    MouseWheel,
}

impl MacroActionKind {
    /// `None` for any bit pattern outside the 6 known kinds (the 3-bit field can
    /// represent 0-7, but only 0-5 are assigned) — reachable from a real device's
    /// `GetMacros` response, not just malformed input, so this must not panic.
    fn from_bits(bits: u8) -> Option<Self> {
        match bits {
            0 => Some(Self::NormalKey),
            1 => Some(Self::ModifyKey),
            2 => Some(Self::MouseKey),
            3 => Some(Self::MouseCursorX),
            4 => Some(Self::MouseCursorY),
            5 => Some(Self::MouseWheel),
            _ => None,
        }
    }

    fn to_bits(self) -> u8 {
        match self {
            Self::NormalKey => 0,
            Self::ModifyKey => 1,
            Self::MouseKey => 2,
            Self::MouseCursorX => 3,
            Self::MouseCursorY => 4,
            Self::MouseWheel => 5,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MacroAction {
    pub edge: MacroEdge,
    pub kind: MacroActionKind,
    pub delay: u32,
    pub key: u8,
}

impl MacroAction {
    /// `None` if the action's type nibble doesn't match any known `MacroActionKind` —
    /// propagated by the caller (see `Macro::deserialize`) as "this macro didn't decode
    /// cleanly", the same handling already used for undecodable names.
    fn decode(bytes: [u8; MACRO_ACTION_LEN]) -> Option<Self> {
        let edge = if bytes[0] >> 7 == 0 { MacroEdge::Down } else { MacroEdge::Up };
        let kind = MacroActionKind::from_bits((bytes[0] >> 4) & 7)?;
        let delay = (((bytes[0] as u32) << 16) & 0x0f_0000) | ((bytes[1] as u32) << 8) | (bytes[2] as u32);
        Some(MacroAction { edge, kind, delay, key: bytes[3] })
    }

    fn encode(self) -> [u8; MACRO_ACTION_LEN] {
        let edge_bit = match self.edge { MacroEdge::Down => 0u8, MacroEdge::Up => 1u8 };
        let byte0 = (edge_bit << 7) | (self.kind.to_bits() << 4) | (((self.delay >> 16) & 0x0f) as u8);
        let byte1 = ((self.delay >> 8) & 0xff) as u8;
        let byte2 = (self.delay & 0xff) as u8;
        [byte0, byte1, byte2, self.key]
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Macro {
    pub name: String,
    pub actions: Vec<MacroAction>,
}

impl Macro {
    fn serialize(&self) -> Vec<u8> {
        let name_utf16: Vec<u16> = self.name.encode_utf16().collect();
        let name_bytes_len = name_utf16.len() * 2;
        let mut out = Vec::with_capacity(1 + name_bytes_len + self.actions.len() * MACRO_ACTION_LEN);
        out.push(name_bytes_len as u8);
        for unit in &name_utf16 {
            out.extend_from_slice(&unit.to_le_bytes());
        }
        for action in &self.actions {
            out.extend_from_slice(&action.encode());
        }
        out
    }

    fn deserialize(bytes: &[u8]) -> Option<Self> {
        if bytes.is_empty() {
            return None;
        }
        let name_len = bytes[0] as usize;
        let name_bytes = bytes.get(1..1 + name_len)?;
        // Decode as a whole UTF-16LE sequence, not code-unit-by-code-unit — a
        // non-BMP character (e.g. an emoji) is a surrogate PAIR, and neither half is a
        // valid standalone `char` on its own (an earlier version of this loop called
        // `char::from_u32` per code unit, which returned `None` on the first surrogate
        // half and silently dropped the entire macro from decode_macro_table's output).
        let name_units: Vec<u16> = name_bytes
            .chunks_exact(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .collect();
        let name = String::from_utf16(&name_units).ok()?;
        let actions_start = 1 + name_len;
        let mut actions = Vec::new();
        let mut offset = actions_start;
        while offset + MACRO_ACTION_LEN <= bytes.len() {
            let chunk: [u8; MACRO_ACTION_LEN] = bytes[offset..offset + MACRO_ACTION_LEN].try_into().unwrap();
            actions.push(MacroAction::decode(chunk)?);
            offset += MACRO_ACTION_LEN;
        }
        Some(Macro { name, actions })
    }
}

pub fn decode_macro_table(buffer: &[u8]) -> Vec<Macro> {
    if buffer.len() < 2 {
        return Vec::new();
    }
    let header_table_length = (buffer[0] as usize) | ((buffer[1] as usize) << 8);
    if header_table_length == 0 {
        return Vec::new();
    }
    let macro_count = header_table_length / MACRO_ACTION_LEN;
    let mut macros = Vec::with_capacity(macro_count);
    for i in 0..macro_count {
        let h = i * MACRO_ACTION_LEN;
        if h + MACRO_ACTION_LEN > buffer.len() {
            break;
        }
        let offset = (buffer[h] as usize) | ((buffer[h + 1] as usize) << 8);
        let length = (buffer[h + 2] as usize) | ((buffer[h + 3] as usize) << 8);
        let Some(macro_bytes) = buffer.get(offset..offset + length) else {
            continue;
        };
        if let Some(m) = Macro::deserialize(macro_bytes) {
            macros.push(m);
        }
    }
    macros
}

pub fn encode_macro_table(macros: &[Macro]) -> Vec<u8> {
    let header_table_length = macros.len() * MACRO_ACTION_LEN;
    let mut headers = Vec::with_capacity(macros.len());
    let mut data = Vec::new();
    let mut offset = header_table_length;
    for m in macros {
        let bytes = m.serialize();
        headers.push((offset as u16, bytes.len() as u16));
        offset += bytes.len();
        data.push(bytes);
    }

    let mut out = vec![0u8; header_table_length];
    for (i, (off, len)) in headers.iter().enumerate() {
        let h = i * MACRO_ACTION_LEN;
        out[h] = (*off & 0xff) as u8;
        out[h + 1] = (*off >> 8) as u8;
        out[h + 2] = (*len & 0xff) as u8;
        out[h + 3] = (*len >> 8) as u8;
    }
    for bytes in data {
        out.extend_from_slice(&bytes);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The exact 71-byte SetMacros payload captured from the official RK
    /// configurator writing a single macro (10 actions: 2 mouse-button
    /// press/release pairs, 1 modifier press/release pair, 2 normal-key
    /// press/release pairs) — see docs/superpowers/specs/2026-08-25-macros-capture.md.
    const CAPTURED_SHORT: &[u8] = &[
        0x04, 0x00, 0x43, 0x00, 0x1a, 0x44, 0x00, 0x65, 0x00, 0x66, 0x00, 0x61, 0x00,
        0x75, 0x00, 0x6c, 0x00, 0x74, 0x00, 0x20, 0x00, 0x4d, 0x00, 0x61, 0x00, 0x63,
        0x00, 0x72, 0x00, 0x6f, 0x00, 0x20, 0x00, 0xc2, 0x01, 0xa0, 0x03, 0x9e, 0x01,
        0x20, 0x00, 0x33, 0x02, 0xa0, 0x04, 0x26, 0x02, 0x10, 0x01, 0x28, 0xe0, 0x00,
        0x00, 0x7d, 0x06, 0x80, 0x00, 0x0a, 0x06, 0x90, 0x04, 0x29, 0xe0, 0x00, 0x00,
        0x4a, 0x1b, 0x80, 0x00, 0x00, 0x1b,
    ];

    #[test]
    fn decode_matches_the_real_captured_short_setmacros_buffer() {
        let macros = decode_macro_table(CAPTURED_SHORT);
        assert_eq!(macros.len(), 1);
        assert_eq!(macros[0].name, "Default Macro");
        assert_eq!(macros[0].actions.len(), 10);

        // Every action, verified against the configurator's own recorder JSON
        // (action=edge, type, delay, key) — see the capture notes for the source.
        let expected = [
            (MacroEdge::Down, MacroActionKind::MouseKey, 194, 1),
            (MacroEdge::Up, MacroActionKind::MouseKey, 926, 1),
            (MacroEdge::Down, MacroActionKind::MouseKey, 51, 2),
            (MacroEdge::Up, MacroActionKind::MouseKey, 1062, 2),
            (MacroEdge::Down, MacroActionKind::ModifyKey, 296, 224),
            (MacroEdge::Down, MacroActionKind::NormalKey, 125, 6),
            (MacroEdge::Up, MacroActionKind::NormalKey, 10, 6),
            (MacroEdge::Up, MacroActionKind::ModifyKey, 1065, 224),
            (MacroEdge::Down, MacroActionKind::NormalKey, 74, 27),
            (MacroEdge::Up, MacroActionKind::NormalKey, 0, 27),
        ];
        for (i, &(edge, kind, delay, key)) in expected.iter().enumerate() {
            assert_eq!(
                macros[0].actions[i],
                MacroAction { edge, kind, delay, key },
                "action[{i}] mismatch"
            );
        }
    }

    #[test]
    fn encode_reproduces_the_real_captured_short_setmacros_buffer_byte_for_byte() {
        let macros = decode_macro_table(CAPTURED_SHORT);
        let re_encoded = encode_macro_table(&macros);
        assert_eq!(re_encoded, CAPTURED_SHORT);
    }

    /// The exact 1183-byte SetMacros payload (reassembled from 3 pages) captured
    /// from the configurator writing a single macro with 288 actions (144
    /// press+release pairs of Backspace, key code 42, random delays) — large
    /// enough to have required 3 real SetMacros pages on the wire, confirming the
    /// paging protocol in Task 3 separately from this buffer-format test. See
    /// docs/superpowers/specs/2026-08-25-macros-capture.md for the full derivation.
    const CAPTURED_LONG: &str = "04009b041a440065006600610075006c00740020004d006100630072006f000000562a80005f2a0000502a80005a2a00004c2a8000542a00004f2a8000832a0000422a8000502a00004b2a8000552a0000522a8000502a0000542a8000832a0000462a8000462a00004b2a80005b2a0000462a80005b2a00004f2a8000702a0000462a80005a2a0000462a80005e2a0000412a80004d2a0000592a8002dd2a00003a2a8000562a0000462a8000612a0000452a8000642a00003c2a8000782a0000412a8000602a0000412a8000512a00004a2a80005f2a0000502a8000882a00004b2a80005a2a0000462a80005b2a0000522a80005f2a00004f2a8000782a00004b2a8000642a0000472a8000622a0000422a8000652a00004b2a80034f2a0000482a8000682a0000412a8000602a00004a2a8000642a0000502a80008c2a0000512a80005f2a00004f2a8000522a0000552a8000692a00005b2a80008c2a0000552a8000642a00004c2a80005f2a0000562a80005f2a0000652a80007d2a0000652a8000592a0000542a8000552a0000562a80005a2a0000732a80035f2a0000422a8000632a00004c2a80004f2a0000512a80005f2a0000572a80008b2a00004a2a8000502a0000562a8000552a0000512a80006d2a0000502a80009c2a00005b2a8000722a0000552a8000562a0000542a80005f2a00005b2a80008d2a0000542a80005f2a0000582a8000662a00004d2a80005f2a00005a2a80090a2a0000402a8000552a0000512a8000662a00004f2a8000632a0000512a8000902a0000502a8000472a0000562a8000632a0000562a8000682a0000562a8000962a00005a2a80005f2a00005b2a8000682a0000502a80004b2a00005a2a80009b2a0000552a80005f2a00005a2a8000642a0000472a8000692a0000502a8007d22a00004b2a8000722a0000382a8000562a0000502a8000652a00004e2a8000832a0000552a8000692a00004a2a8000562a0000502a8000742a0000552a8000932a00005e2a8000692a0000542a8000552a00005a2a8000692a0000642a80007e2a00008d2a80005b2a0000592a8000552a0000602a8000622a00005b2a8003502a0000462a8000612a0000542a8000502a0000552a80006a2a0000642a8000732a00006e2a8000592a0000602a80004a2a0000602a8000692a0000562a8000782a00006c2a8000532a0000542a8000562a00005a2a80005b2a00005a2a8000692a00005f2a80005f2a0000602a80005f2a00005d2a8000582a0000592a8003282a0000452a8000652a0000512a8000552a00005a2a8000682a0000562a8000872a00004b2a80005f2a0000512a8000552a00005a2a8000692a00005a2a80007e2a00005b2a80006e2a0000502a80005b2a00005f2a8000542a0000632a8000792a00005a2a80006d2a0000562a8000732a0000642a8000822a0000832a800ae62a0000552a80006f2a00005a2a8000782a0000552a80009b2a0000552a8000dc2a0000a72a8000872a0000642a8000a52a00005f2a8000bf2a00005a2a8000fb2a0000cd2a8000782a0000742a8000b02a0000632a8000c32a0000702a8000da2a0000c52a8000772a00005a2a80009b2a0000562a8000be2a00005a2a8000002a";

    fn hex_decode(s: &str) -> Vec<u8> {
        (0..s.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
            .collect()
    }

    #[test]
    fn decode_matches_the_real_captured_long_multi_page_setmacros_buffer() {
        let captured = hex_decode(CAPTURED_LONG);
        assert_eq!(captured.len(), 1183);
        let macros = decode_macro_table(&captured);

        assert_eq!(macros.len(), 1);
        assert_eq!(macros[0].name, "Default Macro");
        assert_eq!(macros[0].actions.len(), 288);

        // Spot-check the first and last two actions (all 288 alternate
        // press/release of key 42 = Backspace with varying delay).
        assert_eq!(
            macros[0].actions[0],
            MacroAction { edge: MacroEdge::Down, kind: MacroActionKind::NormalKey, delay: 86, key: 42 }
        );
        assert_eq!(
            macros[0].actions[1],
            MacroAction { edge: MacroEdge::Up, kind: MacroActionKind::NormalKey, delay: 95, key: 42 }
        );
        assert_eq!(
            macros[0].actions[286],
            MacroAction { edge: MacroEdge::Down, kind: MacroActionKind::NormalKey, delay: 90, key: 42 }
        );
        assert_eq!(
            macros[0].actions[287],
            MacroAction { edge: MacroEdge::Up, kind: MacroActionKind::NormalKey, delay: 0, key: 42 }
        );
        for action in &macros[0].actions {
            assert_eq!(action.kind, MacroActionKind::NormalKey);
            assert_eq!(action.key, 42);
        }
    }

    #[test]
    fn encode_reproduces_the_real_captured_long_setmacros_buffer_byte_for_byte() {
        let captured = hex_decode(CAPTURED_LONG);
        let macros = decode_macro_table(&captured);
        let re_encoded = encode_macro_table(&macros);
        assert_eq!(re_encoded, captured);
    }

    #[test]
    fn round_trip_empty_table() {
        let buf = encode_macro_table(&[]);
        assert_eq!(decode_macro_table(&buf), Vec::new());
    }

    #[test]
    fn round_trip_a_macro_with_zero_actions() {
        let macros = vec![Macro { name: "Empty".to_string(), actions: vec![] }];
        let buf = encode_macro_table(&macros);
        assert_eq!(decode_macro_table(&buf), macros);
    }

    #[test]
    fn round_trip_a_macro_with_a_non_ascii_name() {
        let macros = vec![Macro {
            name: "日本語".to_string(),
            actions: vec![MacroAction { edge: MacroEdge::Down, kind: MacroActionKind::NormalKey, delay: 0, key: 4 }],
        }];
        let buf = encode_macro_table(&macros);
        assert_eq!(decode_macro_table(&buf), macros);
    }

    #[test]
    fn round_trip_multiple_macros_with_varying_action_counts() {
        let macros = vec![
            Macro {
                name: "One".to_string(),
                actions: vec![MacroAction { edge: MacroEdge::Down, kind: MacroActionKind::NormalKey, delay: 0, key: 4 }],
            },
            Macro {
                name: "Three".to_string(),
                actions: vec![
                    MacroAction { edge: MacroEdge::Down, kind: MacroActionKind::ModifyKey, delay: 0, key: 1 },
                    MacroAction { edge: MacroEdge::Down, kind: MacroActionKind::NormalKey, delay: 4095, key: 6 },
                    MacroAction { edge: MacroEdge::Up, kind: MacroActionKind::NormalKey, delay: 0, key: 6 },
                ],
            },
        ];
        let buf = encode_macro_table(&macros);
        assert_eq!(decode_macro_table(&buf), macros);
    }

    #[test]
    fn round_trip_a_macro_with_a_non_bmp_name() {
        // A name containing a character outside the Basic Multilingual Plane (here, an
        // emoji) UTF-16LE-encodes as a surrogate pair. A prior version of `deserialize`
        // decoded UTF-16 code units one at a time via `char::from_u32`, which fails on
        // either half of a surrogate pair alone — silently dropping the whole macro from
        // decode_macro_table's output, with no error.
        let macros = vec![Macro {
            name: "Copy \u{1F4CB}".to_string(),
            actions: vec![MacroAction { edge: MacroEdge::Down, kind: MacroActionKind::NormalKey, delay: 0, key: 4 }],
        }];
        let buf = encode_macro_table(&macros);
        let decoded = decode_macro_table(&buf);
        assert_eq!(decoded, macros, "macro with a non-BMP name must not be dropped");
    }

    #[test]
    fn an_action_with_an_unknown_type_nibble_does_not_panic_and_drops_that_macro() {
        // Type nibble 6 is unused (only 0-5 are assigned kinds) but representable in the
        // 3-bit field — reachable from a real device response, not just malformed input.
        // decode_macro_table must report "this macro didn't decode", not panic and take
        // down the whole `get-macros`/`export-hcl` command.
        let mut buf = encode_macro_table(&[Macro {
            name: "x".to_string(),
            actions: vec![MacroAction { edge: MacroEdge::Down, kind: MacroActionKind::NormalKey, delay: 0, key: 4 }],
        }]);
        let name_len = buf[4] as usize; // header[0].offset == 4 for a single macro
        let action_byte0_offset = 4 + 1 + name_len;
        buf[action_byte0_offset] = (buf[action_byte0_offset] & 0b1000_1111) | (6 << 4); // set type nibble to 6
        let decoded = decode_macro_table(&buf); // must not panic
        assert!(decoded.is_empty(), "a macro with an undecodable action must be dropped, not panic");
    }
}
