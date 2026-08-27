use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlotState {
    Clean,
    Customized,
    Dirty,
}

pub struct AppState {
    pub device_keymap: HashMap<u8, HashMap<u16, u32>>,
    pub device_led: Vec<u8>,
    pub working_keymap: HashMap<u8, HashMap<u16, u32>>,
    pub working_led: Vec<u8>,
    pub factory_keymap: HashMap<u8, HashMap<u16, u32>>,
}

impl AppState {
    pub fn new(
        device_keymap: HashMap<u8, HashMap<u16, u32>>,
        device_led: Vec<u8>,
        factory_keymap: HashMap<u8, HashMap<u16, u32>>,
    ) -> Self {
        let working_keymap = device_keymap.clone();
        let working_led = device_led.clone();
        Self {
            device_keymap,
            device_led,
            working_keymap,
            working_led,
            factory_keymap,
        }
    }

    fn slot_value(map: &HashMap<u8, HashMap<u16, u32>>, layer: u8, slot: u16) -> u32 {
        map.get(&layer).and_then(|l| l.get(&slot)).copied().unwrap_or(0)
    }

    /// The current display state of one keymap slot, dirty taking priority over
    /// customized when a slot is both (working differs from device AND device differs
    /// from factory).
    pub fn keymap_slot_state(&self, layer: u8, slot: u16) -> SlotState {
        let working = Self::slot_value(&self.working_keymap, layer, slot);
        let device = Self::slot_value(&self.device_keymap, layer, slot);
        let factory = Self::slot_value(&self.factory_keymap, layer, slot);
        if working != device {
            SlotState::Dirty
        } else if device != factory {
            SlotState::Customized
        } else {
            SlotState::Clean
        }
    }

    /// Whether one LED slot's color differs between working and device state. LED has no
    /// factory baseline to diff against, unlike keymap.
    pub fn led_slot_dirty(&self, slot: u16) -> bool {
        let led_colors_slot_count = self.device_led.len() / 3;
        let (r_off, g_off, b_off) = (
            slot as usize,
            slot as usize + led_colors_slot_count,
            slot as usize + led_colors_slot_count * 2,
        );
        self.working_led.get(r_off) != self.device_led.get(r_off)
            || self.working_led.get(g_off) != self.device_led.get(g_off)
            || self.working_led.get(b_off) != self.device_led.get(b_off)
    }

    /// Whether anything at all — any keymap slot on any layer, or any LED slot — is dirty.
    /// Used to decide whether Save has anything to do.
    pub fn any_dirty(&self) -> bool {
        self.working_keymap != self.device_keymap || self.working_led != self.device_led
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn map(entries: &[(u8, u16, u32)]) -> HashMap<u8, HashMap<u16, u32>> {
        let mut out: HashMap<u8, HashMap<u16, u32>> = HashMap::new();
        for &(layer, slot, val) in entries {
            out.entry(layer).or_default().insert(slot, val);
        }
        out
    }

    #[test]
    fn new_clones_device_state_into_working_state() {
        let device_keymap = map(&[(0, 7, 0xAA)]);
        let device_led = vec![1, 2, 3];
        let factory_keymap = map(&[(0, 7, 0xAA)]);
        let state = AppState::new(device_keymap.clone(), device_led.clone(), factory_keymap);
        assert_eq!(state.working_keymap, device_keymap);
        assert_eq!(state.working_led, device_led);
    }

    #[test]
    fn keymap_slot_matching_factory_is_clean() {
        let keymap = map(&[(0, 7, 0xAA)]);
        let state = AppState::new(keymap.clone(), vec![], keymap);
        assert_eq!(state.keymap_slot_state(0, 7), SlotState::Clean);
    }

    #[test]
    fn keymap_slot_differing_from_factory_but_matching_device_is_customized() {
        let device_keymap = map(&[(0, 7, 0xBB)]);
        let factory_keymap = map(&[(0, 7, 0xAA)]);
        let state = AppState::new(device_keymap, vec![], factory_keymap);
        assert_eq!(state.keymap_slot_state(0, 7), SlotState::Customized);
    }

    #[test]
    fn keymap_slot_edited_this_session_is_dirty() {
        let device_keymap = map(&[(0, 7, 0xAA)]);
        let factory_keymap = map(&[(0, 7, 0xAA)]);
        let mut state = AppState::new(device_keymap, vec![], factory_keymap);
        state.working_keymap.get_mut(&0).unwrap().insert(7, 0xCC);
        assert_eq!(state.keymap_slot_state(0, 7), SlotState::Dirty);
    }

    #[test]
    fn dirty_wins_over_customized_when_a_slot_is_both() {
        // device already differs from factory (customized), AND the user has edited it
        // further this session (dirty) — dirty must win.
        let device_keymap = map(&[(0, 7, 0xBB)]);
        let factory_keymap = map(&[(0, 7, 0xAA)]);
        let mut state = AppState::new(device_keymap, vec![], factory_keymap);
        state.working_keymap.get_mut(&0).unwrap().insert(7, 0xCC);
        assert_eq!(state.keymap_slot_state(0, 7), SlotState::Dirty);
    }

    #[test]
    fn led_slot_unedited_is_not_dirty() {
        // 2 slots: R[2] G[2] B[2]
        let led = vec![10, 20, 30, 40, 50, 60];
        let state = AppState::new(HashMap::new(), led, HashMap::new());
        assert!(!state.led_slot_dirty(0));
        assert!(!state.led_slot_dirty(1));
    }

    #[test]
    fn led_slot_edited_is_dirty() {
        let led = vec![10, 20, 30, 40, 50, 60];
        let mut state = AppState::new(HashMap::new(), led, HashMap::new());
        state.working_led[0] = 99; // R of slot 0
        assert!(state.led_slot_dirty(0));
        assert!(!state.led_slot_dirty(1));
    }

    #[test]
    fn any_dirty_is_false_immediately_after_load() {
        let keymap = map(&[(0, 7, 0xAA)]);
        let led = vec![1, 2, 3];
        let state = AppState::new(keymap, led, HashMap::new());
        assert!(!state.any_dirty());
    }

    #[test]
    fn any_dirty_is_true_after_a_keymap_edit() {
        let keymap = map(&[(0, 7, 0xAA)]);
        let mut state = AppState::new(keymap, vec![], HashMap::new());
        state.working_keymap.get_mut(&0).unwrap().insert(7, 0xCC);
        assert!(state.any_dirty());
    }

    #[test]
    fn any_dirty_is_true_after_an_led_edit() {
        let led = vec![1, 2, 3];
        let mut state = AppState::new(HashMap::new(), led, HashMap::new());
        state.working_led[0] = 99;
        assert!(state.any_dirty());
    }
}
