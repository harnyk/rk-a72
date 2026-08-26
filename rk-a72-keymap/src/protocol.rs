/// The only device this crate is known to speak to: the wired RK A72.
///
/// Every byte layout here — the KeyMatrix slot count, the LED plane sizes, the macro
/// table pages, the `.last()` collection pick in [`crate::session::find_wired_device`] —
/// was confirmed against this one device. Other RK models share the same "BeiYing"
/// protocol family and may well work, but none have been verified, so the tooling
/// refuses to talk to them rather than writing guessed byte layouts to a keyboard.
pub const SUPPORTED_VENDOR_ID: u16 = 0x258a;
/// See [`SUPPORTED_VENDOR_ID`].
pub const SUPPORTED_PRODUCT_ID: u16 = 0x0216;

pub const REPORT_ID: u8 = 9;
pub const FEATURE_USAGE_PAGE: u16 = 0xff02;
pub const FEATURE_USAGE: u16 = 0x01;
pub const REPORT_LEN: usize = 519;
pub const RESPONSE_HEADER_LEN: usize = 7;

pub const KEYMATRIX_SLOT_COUNT: usize = 126;
pub const KEYMATRIX_BUFFER_LEN: usize = KEYMATRIX_SLOT_COUNT * 4;

/// Per-key custom-colour buffer: planar `R[126] G[126] B[126]`, indexed by the same
/// 0..125 slot as the KeyMatrix. Confirmed byte-for-byte against real USB captures.
pub const LED_COLORS_SLOT_COUNT: usize = 126;
pub const LED_COLORS_BUFFER_LEN: usize = LED_COLORS_SLOT_COUNT * 3;

/// The 128-byte profile buffer read/written by `GetProfile`/`SetProfile`. LED mode
/// selection lives inside it (offset 9), among unrelated device settings.
pub const PROFILE_BUFFER_LEN: usize = 128;

/// Offset of `LedModeSelection` in the profile buffer: `0` = a built-in effect is
/// active, `1` = SelfDefine (custom per-key colours from `SetLedColors` are shown).
pub const PROFILE_LED_MODE_SELECTION_OFFSET: usize = 9;

/// Offset of an unnamed profile byte that the real device's own frontend flips
/// alongside `LedModeSelection` when entering SelfDefine. Confirmed via USB capture:
/// changes `0 -> 19` in lockstep with offset 9's `0 -> 1`. Meaning unknown; written
/// verbatim because the device does it, not because it's understood.
pub const PROFILE_SELF_DEFINE_MARKER_OFFSET: usize = 33;
pub const PROFILE_SELF_DEFINE_MARKER_VALUE: u8 = 19;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpCode {
    GetKeyMatrix,
    SetKeyMatrix,
    GetProfile,
    SetProfile,
    GetLedColors,
    SetLedColors,
    GetMacros,
    SetMacros,
}

impl OpCode {
    pub fn to_byte(self) -> u8 {
        match self {
            OpCode::GetKeyMatrix => 131,
            OpCode::SetKeyMatrix => 3,
            OpCode::GetProfile => 132,
            OpCode::SetProfile => 4,
            OpCode::GetLedColors => 134,
            OpCode::SetLedColors => 6,
            OpCode::GetMacros => 133,
            OpCode::SetMacros => 5,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct RequestOptions {
    pub byte1: u8,
    pub cmd_val: u8,
    pub data_length: u16,
    pub payload: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ParsedResponse {
    pub cmd_id: u8,
    pub byte1: u8,
    pub cmd_val: u8,
    pub data_length: u16,
    pub payload: Vec<u8>,
}

pub fn build_request(opcode: OpCode, opts: &RequestOptions) -> Vec<u8> {
    let mut report = vec![0u8; REPORT_LEN];
    report[0] = opcode.to_byte();
    report[1] = opts.byte1;
    report[2] = opts.cmd_val;
    report[3] = 1;
    report[5] = (opts.data_length & 0xff) as u8;
    report[6] = (opts.data_length >> 8) as u8;
    for (i, &byte) in opts.payload.iter().enumerate() {
        report[7 + i] = byte;
    }
    report
}

pub fn build_request_with_report_id(opcode: OpCode, opts: &RequestOptions) -> Vec<u8> {
    let mut out = Vec::with_capacity(REPORT_LEN + 1);
    out.push(REPORT_ID);
    out.extend(build_request(opcode, opts));
    out
}

/// `GetMacros` request for one page: report[4] = page index, dataLength = MACRO_PAGE_LEN.
/// Distinct from `build_request` only in which field carries the page index — GetMacros
/// puts it at byte 4, the same position `SetMacros` uses for packageIndex, but GetMacros
/// has no packageNum byte.
pub fn build_macro_get_page_request(page_index: u8) -> Vec<u8> {
    let mut report = vec![0u8; REPORT_LEN];
    report[0] = OpCode::GetMacros.to_byte();
    report[3] = 1;
    report[4] = page_index;
    report[5] = (crate::macros::MACRO_PAGE_LEN & 0xff) as u8;
    report[6] = (crate::macros::MACRO_PAGE_LEN >> 8) as u8;
    report
}

/// `SetMacros` request for one page. Field layout does NOT match `build_request`'s
/// fixed `report[3] = 1` marker — SetMacros instead uses report[3] for packageNum and
/// report[4] for packageIndex.
pub fn build_macro_set_page_request(package_num: u8, package_index: u8, payload: &[u8]) -> Vec<u8> {
    let mut report = vec![0u8; REPORT_LEN];
    report[0] = OpCode::SetMacros.to_byte();
    report[3] = package_num;
    report[4] = package_index;
    report[5] = (payload.len() & 0xff) as u8;
    report[6] = (payload.len() >> 8) as u8;
    for (i, &byte) in payload.iter().enumerate() {
        report[7 + i] = byte;
    }
    report
}

/// `raw` is the full feature-report read, report-ID byte included — always strips
/// exactly 1 byte (every observed response includes it), same as the Node port.
pub fn parse_response(raw: &[u8]) -> ParsedResponse {
    let bytes = &raw[1..];
    let cmd_id = bytes[0];
    let byte1 = bytes[1];
    let cmd_val = bytes[2];
    let data_length = (bytes[5] as u16) | ((bytes[6] as u16) << 8);
    let start = RESPONSE_HEADER_LEN;
    let end = start + data_length as usize;
    let payload = bytes[start..end].to_vec();
    ParsedResponse {
        cmd_id,
        byte1,
        cmd_val,
        data_length,
        payload,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::macros::MACRO_PAGE_LEN;

    #[test]
    fn build_request_lays_out_the_7_byte_header_and_payload() {
        let opts = RequestOptions {
            byte1: 0x02,
            cmd_val: 0x00,
            data_length: KEYMATRIX_BUFFER_LEN as u16,
            payload: vec![],
        };
        let report = build_request(OpCode::GetKeyMatrix, &opts);
        assert_eq!(report.len(), REPORT_LEN);
        assert_eq!(report[0], 131); // opcode
        assert_eq!(report[1], 0x02); // byte1
        assert_eq!(report[2], 0x00); // cmdVal
        assert_eq!(report[3], 1); // fixed marker
        assert_eq!(report[5], (KEYMATRIX_BUFFER_LEN as u16 & 0xff) as u8);
        assert_eq!(report[6], (KEYMATRIX_BUFFER_LEN as u16 >> 8) as u8);
    }

    #[test]
    fn build_request_with_report_id_prepends_report_id() {
        let report = build_request_with_report_id(OpCode::SetKeyMatrix, &RequestOptions::default());
        assert_eq!(report[0], REPORT_ID);
        assert_eq!(report[1], 3); // opcode
        assert_eq!(report.len(), REPORT_LEN + 1);
    }

    #[test]
    fn parse_response_strips_report_id_and_reads_the_header() {
        let mut raw = vec![0u8; 1 + RESPONSE_HEADER_LEN + 4];
        raw[0] = REPORT_ID;
        raw[1] = 131; // cmdId
        raw[2] = 0x02; // byte1
        raw[3] = 0x00; // cmdVal
        raw[4] = 1; // fixed marker
        raw[6] = 4; // dataLength low byte
        raw[7] = 0; // dataLength high byte
        raw[8..12].copy_from_slice(&[0xAA, 0xBB, 0xCC, 0xDD]);

        let parsed = parse_response(&raw);
        assert_eq!(parsed.cmd_id, 131);
        assert_eq!(parsed.byte1, 0x02);
        assert_eq!(parsed.cmd_val, 0x00);
        assert_eq!(parsed.data_length, 4);
        assert_eq!(parsed.payload, vec![0xAA, 0xBB, 0xCC, 0xDD]);
    }

    #[test]
    fn get_macros_opcode_is_133() {
        assert_eq!(OpCode::GetMacros.to_byte(), 133);
    }

    #[test]
    fn set_macros_opcode_is_5() {
        assert_eq!(OpCode::SetMacros.to_byte(), 5);
    }

    #[test]
    fn build_macro_get_page_request_puts_the_page_index_in_byte_4() {
        let report = build_macro_get_page_request(3);
        assert_eq!(report.len(), REPORT_LEN);
        assert_eq!(report[0], 133); // opcode
        assert_eq!(report[3], 1); // fixed marker
        assert_eq!(report[4], 3); // page index
        assert_eq!(report[5], (MACRO_PAGE_LEN & 0xff) as u8);
        assert_eq!(report[6], (MACRO_PAGE_LEN >> 8) as u8);
    }

    #[test]
    fn build_macro_set_page_request_lays_out_packagenum_packageindex_length_and_payload() {
        let payload = vec![0xAAu8, 0xBB, 0xCC];
        let report = build_macro_set_page_request(8, 2, &payload);
        assert_eq!(report.len(), REPORT_LEN);
        assert_eq!(report[0], 5); // opcode
        assert_eq!(report[3], 8); // packageNum
        assert_eq!(report[4], 2); // packageIndex
        assert_eq!(report[5], 3); // payload length low byte
        assert_eq!(report[6], 0); // payload length high byte
        assert_eq!(&report[7..10], &[0xAA, 0xBB, 0xCC]);
    }
}
