use std::collections::HashMap;

#[derive(serde::Deserialize)]
struct RawVisualOverrides {
    keycode: HashMap<String, String>,
    physical: HashMap<String, String>,
    label: HashMap<String, String>,
}

/// Old, human-readable glyphs for the symbolic names that were renamed to
/// shell-safe identifiers (e.g. keycode 53's canonical name is "Backtick", its
/// visual name is "`"). Visual names are display-only — never accepted as input.
pub struct VisualOverrides {
    pub(crate) keycode: HashMap<u32, String>,
    pub(crate) physical: HashMap<u32, String>,
    pub(crate) label: HashMap<u32, String>,
}

fn parse_id_map(raw: HashMap<String, String>) -> HashMap<u32, String> {
    raw.into_iter()
        .filter_map(|(k, v)| k.parse::<u32>().ok().map(|id| (id, v)))
        .collect()
}

impl VisualOverrides {
    pub fn new() -> Self {
        let raw: RawVisualOverrides =
            serde_json::from_str(include_str!("../data/visual_overrides.json"))
                .expect("visual_overrides.json must be valid JSON");
        Self {
            keycode: parse_id_map(raw.keycode),
            physical: parse_id_map(raw.physical),
            label: parse_id_map(raw.label),
        }
    }

    pub fn keycode(&self, id: u32, canonical: &str) -> String {
        self.keycode
            .get(&id)
            .cloned()
            .unwrap_or_else(|| canonical.to_string())
    }

    pub fn physical(&self, id: u32, canonical: &str) -> String {
        self.physical
            .get(&id)
            .cloned()
            .unwrap_or_else(|| canonical.to_string())
    }

    pub fn label(&self, id: u32, canonical: &str) -> String {
        self.label
            .get(&id)
            .cloned()
            .unwrap_or_else(|| canonical.to_string())
    }
}

impl Default for VisualOverrides {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keycode_override_is_used_when_present() {
        let overrides = VisualOverrides::new();
        assert_eq!(overrides.keycode(53, "Backtick"), "`");
    }

    #[test]
    fn keycode_falls_back_to_canonical_when_absent() {
        let overrides = VisualOverrides::new();
        assert_eq!(overrides.keycode(4, "A"), "A");
    }

    #[test]
    fn physical_override_is_used_when_present() {
        let overrides = VisualOverrides::new();
        assert_eq!(overrides.physical(13, "Digit1"), "1!");
    }

    #[test]
    fn physical_falls_back_to_canonical_when_absent() {
        let overrides = VisualOverrides::new();
        assert_eq!(overrides.physical(7, "Esc"), "Esc");
    }

    #[test]
    fn label_override_is_used_when_present() {
        let overrides = VisualOverrides::new();
        assert_eq!(overrides.label(117440522, "EMITest"), "EMI Test");
    }

    #[test]
    fn label_falls_back_to_canonical_when_absent() {
        let overrides = VisualOverrides::new();
        assert_eq!(overrides.label(117440665, "Mute"), "Mute");
    }
}
