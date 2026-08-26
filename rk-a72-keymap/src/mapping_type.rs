#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyMappingType {
    KeyBoard,
    Mouse,
    Media,
    Macro,
    Custom,
    DpiKey,
    ProfileSwitch,
    SpecialFun,
    LightSwitch,
    ReportRate,
    SnipeKey,
    PressGun,
    FnKey,
    LodKey,
    Pc,
    /// Absent from the official configurator's own type enum (which tops out at Pc=16),
    /// but raw=285212672 (0x11000000, i.e. type 17) is used in practice for
    /// KEY_Touch_WWW/KEY_AI_MODE — confirmed on real hardware as the RK-logo
    /// "touch to open website" action.
    Touch,
    Unknown(u8),
}

impl KeyMappingType {
    pub fn from_byte(byte: u8) -> Self {
        match byte {
            0 => Self::KeyBoard,
            1 => Self::Mouse,
            2 => Self::Media,
            3 => Self::Macro,
            4 => Self::Custom,
            5 => Self::DpiKey,
            6 => Self::ProfileSwitch,
            7 => Self::SpecialFun,
            8 => Self::LightSwitch,
            9 => Self::ReportRate,
            10 => Self::SnipeKey,
            11 => Self::PressGun,
            13 => Self::FnKey,
            15 => Self::LodKey,
            16 => Self::Pc,
            17 => Self::Touch,
            other => Self::Unknown(other),
        }
    }

    pub fn to_byte(self) -> u8 {
        match self {
            Self::KeyBoard => 0,
            Self::Mouse => 1,
            Self::Media => 2,
            Self::Macro => 3,
            Self::Custom => 4,
            Self::DpiKey => 5,
            Self::ProfileSwitch => 6,
            Self::SpecialFun => 7,
            Self::LightSwitch => 8,
            Self::ReportRate => 9,
            Self::SnipeKey => 10,
            Self::PressGun => 11,
            Self::FnKey => 13,
            Self::LodKey => 15,
            Self::Pc => 16,
            Self::Touch => 17,
            Self::Unknown(b) => b,
        }
    }

    pub fn type_name(self) -> String {
        match self {
            Self::KeyBoard => "KeyBoard".to_string(),
            Self::Mouse => "Mousue".to_string(),
            Self::Media => "Media".to_string(),
            Self::Macro => "Macro".to_string(),
            Self::Custom => "Custom".to_string(),
            Self::DpiKey => "DPIKey".to_string(),
            Self::ProfileSwitch => "ProfileSwitch".to_string(),
            Self::SpecialFun => "SpecialFun".to_string(),
            Self::LightSwitch => "LightSwitch".to_string(),
            Self::ReportRate => "ReportRate".to_string(),
            Self::SnipeKey => "SnipeKey".to_string(),
            Self::PressGun => "PressGun".to_string(),
            Self::FnKey => "FnKey".to_string(),
            Self::LodKey => "LodKey".to_string(),
            Self::Pc => "Pc".to_string(),
            Self::Touch => "Touch".to_string(),
            Self::Unknown(b) => format!("Unknown({b})"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_byte_to_byte_round_trip_for_known_types() {
        let cases: &[(u8, KeyMappingType)] = &[
            (0, KeyMappingType::KeyBoard),
            (1, KeyMappingType::Mouse),
            (2, KeyMappingType::Media),
            (3, KeyMappingType::Macro),
            (4, KeyMappingType::Custom),
            (5, KeyMappingType::DpiKey),
            (6, KeyMappingType::ProfileSwitch),
            (7, KeyMappingType::SpecialFun),
            (8, KeyMappingType::LightSwitch),
            (9, KeyMappingType::ReportRate),
            (10, KeyMappingType::SnipeKey),
            (11, KeyMappingType::PressGun),
            (13, KeyMappingType::FnKey),
            (15, KeyMappingType::LodKey),
            (16, KeyMappingType::Pc),
            (17, KeyMappingType::Touch),
        ];
        for &(byte, kind) in cases {
            assert_eq!(KeyMappingType::from_byte(byte), kind);
            assert_eq!(kind.to_byte(), byte);
        }
    }

    #[test]
    fn from_byte_falls_back_to_unknown_for_unmapped_bytes() {
        assert_eq!(KeyMappingType::from_byte(12), KeyMappingType::Unknown(12));
        assert_eq!(KeyMappingType::Unknown(12).to_byte(), 12);
    }

    #[test]
    fn type_name_preserves_the_json_mousue_typo_for_mouse() {
        // Existing/future keymap.yaml files say `type: Mousue` for mouse-button
        // entries (a typo baked into the original site's code) — must not "fix" this,
        // or old and new YAML files stop being interchangeable.
        assert_eq!(KeyMappingType::Mouse.type_name(), "Mousue");
        assert_eq!(KeyMappingType::DpiKey.type_name(), "DPIKey");
        assert_eq!(KeyMappingType::SpecialFun.type_name(), "SpecialFun");
        assert_eq!(KeyMappingType::Unknown(12).type_name(), "Unknown(12)");
    }
}
