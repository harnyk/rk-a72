#[derive(Debug, thiserror::Error)]
pub enum KeymapError {
    #[error("unknown modifier \"{0}\"")]
    UnknownModifier(String),

    #[error("HCL parse error: {0}")]
    Hcl(String),

    #[error("HCL layer \"{0}\": the A72 has only the \"normal\" and \"fn\" layers")]
    HclUnknownLayer(String),

    #[error("HCL {context}: {detail}")]
    HclValidation { context: String, detail: String },

    // Parsing and validation of this section succeed, but the core has no write path
    // for it yet — the opcode is either un-reversed or never exercised against real
    // hardware. `reason` names the missing opcode so the message is actionable.
    #[error("HCL: {feature} was parsed and validated, but the core cannot flash it yet ({reason})")]
    HclUnsupported { feature: String, reason: String },
}
