use std::time::Duration;

use hidapi::HidResult;

use crate::macros::{decode_macro_table, encode_macro_table, Macro, MACRO_BUFFER_LEN, MACRO_PAGE_LEN};
use crate::protocol::{
    build_macro_get_page_request, build_macro_set_page_request, OpCode, RequestOptions,
    KEYMATRIX_BUFFER_LEN, LED_COLORS_BUFFER_LEN, PROFILE_BUFFER_LEN,
    PROFILE_LED_MODE_SELECTION_OFFSET, PROFILE_SELF_DEFINE_MARKER_OFFSET,
    PROFILE_SELF_DEFINE_MARKER_VALUE,
};
use crate::session::WiredSession;

/// How long to wait after each `SetProfile` write in `LedColorRepository::enter_self_define`
/// before sending the next command — see that method's doc comment for why.
const SELF_DEFINE_STEP_DELAY: Duration = Duration::from_millis(200);

/// Owns the `byte1 = (table << 2) | layer` address packing GetKeyMatrix/SetKeyMatrix
/// expect, so callers never need to know it.
pub struct KeyMatrixRepository {
    session: WiredSession,
    table: u8,
    board: u8,
}

impl KeyMatrixRepository {
    pub fn new(session: WiredSession) -> Self {
        Self {
            session,
            table: 0,
            board: 0,
        }
    }

    #[allow(clippy::identity_op)]
    fn byte1(&self, layer: u8) -> u8 {
        ((self.table << 2) | layer) & 0xff
    }

    pub fn read_layer(&self, layer: u8) -> HidResult<Vec<u8>> {
        let opts = RequestOptions {
            byte1: self.byte1(layer),
            cmd_val: self.board,
            data_length: KEYMATRIX_BUFFER_LEN as u16,
            payload: vec![],
        };
        let response = self
            .session
            .request(OpCode::GetKeyMatrix, &opts, true)?
            .expect("read_layer always requests a response");
        assert_eq!(
            response.payload.len(),
            KEYMATRIX_BUFFER_LEN,
            "GetKeyMatrix returned {} bytes, expected {KEYMATRIX_BUFFER_LEN} — device firmware mismatch or a truncated read",
            response.payload.len()
        );
        Ok(response.payload)
    }

    pub fn write_layer(&self, layer: u8, buffer: &[u8]) -> HidResult<()> {
        let opts = RequestOptions {
            byte1: self.byte1(layer),
            cmd_val: self.board,
            data_length: buffer.len() as u16,
            payload: buffer.to_vec(),
        };
        self.session.request(OpCode::SetKeyMatrix, &opts, false)?;
        Ok(())
    }
}

/// Reads/writes the per-key custom-colour buffer (`SetLedColors`/`GetLedColors`) and
/// switches the active profile into SelfDefine mode so the device actually displays
/// those colours.
pub struct LedColorRepository {
    session: WiredSession,
    board: u8,
}

impl LedColorRepository {
    pub fn new(session: WiredSession) -> Self {
        Self { session, board: 0 }
    }

    pub fn read_colors(&self) -> HidResult<Vec<u8>> {
        let opts = RequestOptions {
            byte1: 0,
            cmd_val: self.board,
            data_length: LED_COLORS_BUFFER_LEN as u16,
            payload: vec![],
        };
        let response = self
            .session
            .request(OpCode::GetLedColors, &opts, true)?
            .expect("read_colors always requests a response");
        assert_eq!(
            response.payload.len(),
            LED_COLORS_BUFFER_LEN,
            "GetLedColors returned {} bytes, expected {LED_COLORS_BUFFER_LEN} — device firmware mismatch or a truncated read",
            response.payload.len()
        );
        Ok(response.payload)
    }

    pub fn write_colors(&self, buffer: &[u8]) -> HidResult<()> {
        let opts = RequestOptions {
            byte1: 0,
            cmd_val: self.board,
            data_length: buffer.len() as u16,
            payload: buffer.to_vec(),
        };
        self.session.request(OpCode::SetLedColors, &opts, false)?;
        Ok(())
    }

    fn get_profile(&self) -> HidResult<Vec<u8>> {
        let opts = RequestOptions {
            byte1: 0,
            cmd_val: self.board,
            data_length: PROFILE_BUFFER_LEN as u16,
            payload: vec![],
        };
        let response = self
            .session
            .request(OpCode::GetProfile, &opts, true)?
            .expect("get_profile always requests a response");
        let profile = response.payload;
        assert_eq!(
            profile.len(),
            PROFILE_BUFFER_LEN,
            "GetProfile returned {} bytes, expected {PROFILE_BUFFER_LEN} — device firmware mismatch or a truncated read",
            profile.len()
        );
        Ok(profile)
    }

    fn set_profile(&self, profile: Vec<u8>) -> HidResult<()> {
        let opts = RequestOptions {
            byte1: 0,
            cmd_val: self.board,
            data_length: profile.len() as u16,
            payload: profile,
        };
        self.session.request(OpCode::SetProfile, &opts, false)?;
        Ok(())
    }

    /// Flips the profile into SelfDefine (`LedModeSelection = 1`, plus the paired
    /// unnamed marker byte) — the mode-select step the real frontend always does
    /// before `SetLedColors`, without which a correct colour write is displayed as
    /// nothing (confirmed on real hardware after a factory reset). The other 126
    /// profile bytes are round-tripped untouched.
    ///
    /// Sleeps briefly after the `SetProfile` write: it's fire-and-forget (`read:
    /// false`, so `WiredSession` applies no delay of its own), and sending the next
    /// command immediately was observed to leave the device showing no visible change
    /// even though every individual write/read succeeded — the device needs time to
    /// actually apply the mode change before it can process what comes next.
    pub fn enter_self_define(&self) -> HidResult<()> {
        let mut profile = self.get_profile()?;
        profile[PROFILE_LED_MODE_SELECTION_OFFSET] = 1;
        profile[PROFILE_SELF_DEFINE_MARKER_OFFSET] = PROFILE_SELF_DEFINE_MARKER_VALUE;
        self.set_profile(profile)?;
        std::thread::sleep(SELF_DEFINE_STEP_DELAY);

        Ok(())
    }
}

/// Reads/writes the 4096-byte macro table (`GetMacros`/`SetMacros`, opcodes 133/5).
/// Unlike every other buffer this crate writes, the macro table doesn't fit in one
/// feature report — both directions are paged across `MACRO_BUFFER_LEN / MACRO_PAGE_LEN`
/// (8) pages. Reads use ordinary request/response per page; writes are paged
/// fire-and-forget (no response read), matching what the official configurator does.
pub struct MacroRepository {
    session: WiredSession,
}

impl MacroRepository {
    pub fn new(session: WiredSession) -> Self {
        Self { session }
    }

    pub fn read_macros(&self) -> HidResult<Vec<Macro>> {
        let pages = MACRO_BUFFER_LEN / MACRO_PAGE_LEN;
        let mut buffer = vec![0u8; MACRO_BUFFER_LEN];
        for page in 0..pages {
            let report = build_macro_get_page_request(page as u8);
            let parsed = self.session.send_and_read(&report)?;

            let start = page * MACRO_PAGE_LEN;
            let end = (start + MACRO_PAGE_LEN).min(MACRO_BUFFER_LEN);
            let copy_len = (end - start).min(parsed.payload.len());
            buffer[start..start + copy_len].copy_from_slice(&parsed.payload[..copy_len]);
        }
        Ok(decode_macro_table(&buffer))
    }

    pub fn write_macros(&self, macros: &[Macro]) -> HidResult<()> {
        let buffer = encode_macro_table(macros);
        let package_num = buffer.len().div_ceil(MACRO_PAGE_LEN) as u8;
        let mut pages = Vec::with_capacity(package_num as usize);
        for (i, chunk) in buffer.chunks(MACRO_PAGE_LEN).enumerate() {
            pages.push(build_macro_set_page_request(package_num, i as u8, chunk));
        }
        self.session.send_pages(&pages)
    }
}
