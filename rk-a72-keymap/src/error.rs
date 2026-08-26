#[derive(Debug, thiserror::Error)]
pub enum KeymapError {
    #[error("unknown modifier \"{0}\"")]
    UnknownModifier(String),

    #[error("Unknown physical key name \"{0}\" — run \"list-keys\" to see valid names.")]
    UnknownPhysicalKey(String),

    #[error("{name}.{layer}: unknown label \"{label}\" — run \"list-keys\" to see valid labels.")]
    UnknownLabel {
        name: String,
        layer: String,
        label: String,
    },

    #[error("{name}.{layer}: unknown KeyBoard key \"{key}\" — run \"list-keys\" to see valid key symbols.")]
    UnknownKeyboardSymbol {
        name: String,
        layer: String,
        key: String,
    },

    #[error("{name}.{layer}: {inner} in mod \"{mod_value}\" — run \"list-keys\" to see valid modifier names.")]
    UnknownModifierIn {
        name: String,
        layer: String,
        inner: String,
        mod_value: String,
    },

    #[error("{name}.{layer}: non-KeyBoard entries need a \"label\" (see \"list-keys\") or a \"raw\" hex value (\".comment\" alone isn't reliably invertible).")]
    MissingLabelOrRaw { name: String, layer: String },

    #[error("{name}.{layer}: expected an object")]
    ExpectedObject { name: String, layer: String },

    #[error("\"{0}\": expected an object with \"normal\"/\"fn\" keys")]
    ExpectedLayerObject(String),

    #[error("\"{name}\": unknown layer key \"{layer}\" (expected \"normal\" or \"fn\")")]
    UnknownLayerKey { name: String, layer: String },

    #[error("{name}.{layer}: invalid raw value {raw}")]
    InvalidRaw {
        name: String,
        layer: String,
        raw: String,
    },

    #[error("YAML parse error: {0}")]
    Yaml(#[from] serde_yaml_ng::Error),

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
