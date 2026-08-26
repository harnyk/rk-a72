use std::ffi::CString;
use std::time::Duration;

use hidapi::{HidApi, HidDevice, HidResult};

use crate::protocol::{
    build_request_with_report_id, parse_response, OpCode, ParsedResponse, RequestOptions,
    FEATURE_USAGE, FEATURE_USAGE_PAGE, REPORT_ID, REPORT_LEN,
};

/// Thin wrapper over one open `hidapi` device for the BeiYing wired feature-report
/// protocol: plain synchronous request/response over
/// send_feature_report()/get_feature_report() — no fragmentation, no events to wait on.
pub struct WiredSession {
    device: HidDevice,
    delay: Duration,
}

impl WiredSession {
    pub fn open(api: &HidApi, path: &std::ffi::CStr) -> HidResult<Self> {
        let device = api.open_path(path)?;
        Ok(Self {
            device,
            delay: Duration::from_millis(150),
        })
    }

    /// Sends a request and, if `read` is true, reads back and parses the response.
    /// Set-type opcodes with no meaningful response payload can pass `read: false`.
    pub fn request(
        &self,
        opcode: OpCode,
        opts: &RequestOptions,
        read: bool,
    ) -> HidResult<Option<ParsedResponse>> {
        let out = build_request_with_report_id(opcode, opts);
        self.device.send_feature_report(&out)?;
        if !read {
            return Ok(None);
        }

        std::thread::sleep(self.delay);
        let mut buf = vec![0u8; REPORT_LEN + 1];
        buf[0] = REPORT_ID;
        let n = self.device.get_feature_report(&mut buf)?;
        Ok(Some(parse_response(&buf[..n])))
    }

    /// Sends each of `pages` (already-built REPORT_LEN report bodies, report ID not yet
    /// prepended) as its own fire-and-forget feature report — no response read between or
    /// after pages. Unlike `request()`, which always reads a response back, a paged write
    /// never does — matching the official configurator, which only ever sets the feature
    /// report and never reads one back for SetMacros.
    pub fn send_pages(&self, pages: &[Vec<u8>]) -> HidResult<()> {
        for page in pages {
            let mut out = Vec::with_capacity(page.len() + 1);
            out.push(REPORT_ID);
            out.extend_from_slice(page);
            self.device.send_feature_report(&out)?;
        }
        Ok(())
    }

    /// Sends one already-built (no report ID) report body and reads back a parsed
    /// response — the same request/response shape `request()` uses, but for a caller
    /// that built the report body itself (e.g. GetMacros's per-page request, whose byte
    /// layout doesn't match `RequestOptions`). `request()` remains the right choice for
    /// every opcode using the standard layout; this is only for opcodes that don't.
    pub fn send_and_read(&self, report_body: &[u8]) -> HidResult<ParsedResponse> {
        let mut out = Vec::with_capacity(report_body.len() + 1);
        out.push(REPORT_ID);
        out.extend_from_slice(report_body);
        self.device.send_feature_report(&out)?;

        std::thread::sleep(self.delay);
        let mut buf = vec![0u8; REPORT_LEN + 1];
        buf[0] = REPORT_ID;
        let n = self.device.get_feature_report(&mut buf)?;
        Ok(parse_response(&buf[..n]))
    }
}

#[derive(Debug, Clone)]
pub struct DeviceMatch {
    pub vendor_id: u16,
    pub product_id: u16,
    pub path: CString,
    pub product: Option<String>,
}

/// Finds the Col08-equivalent (usagePage=0xff02, usage=1) collection for the given
/// vid/pid. Confirmed on real hardware: multiple 0xff02 collections exist, only the
/// LAST one enumerated accepts report ID 9.
pub fn find_wired_device(api: &HidApi, vendor_id: u16, product_id: u16) -> Option<DeviceMatch> {
    api.device_list()
        .filter(|d| {
            d.vendor_id() == vendor_id
                && d.product_id() == product_id
                && d.usage_page() == FEATURE_USAGE_PAGE
                && d.usage() == FEATURE_USAGE
        })
        .last()
        .map(|d| DeviceMatch {
            vendor_id: d.vendor_id(),
            product_id: d.product_id(),
            path: d.path().to_owned(),
            product: d.product_string().map(|s| s.to_string()),
        })
}
