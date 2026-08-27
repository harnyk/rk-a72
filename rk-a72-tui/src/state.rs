use std::collections::HashMap;
use rk_a72_keymap::{patch_buffer, KeyMatrixRepository, LedColorRepository, KEYMATRIX_BUFFER_LEN};

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

    /// Layer numbers (0/1/2) that have at least one keymap slot dirty, in ascending order.
    pub fn dirty_layers(&self) -> Vec<u8> {
        let mut layers: Vec<u8> = self
            .working_keymap
            .keys()
            .filter(|&&layer| {
                let working = self.working_keymap.get(&layer).cloned().unwrap_or_default();
                let device = self.device_keymap.get(&layer).cloned().unwrap_or_default();
                working != device
            })
            .copied()
            .collect();
        layers.sort_unstable();
        layers
    }

    /// The full KEYMATRIX_BUFFER_LEN-byte buffer for one layer, built from
    /// `working_keymap[layer]` — every slot that layer's working map mentions is patched
    /// in; every other slot is zeroed, matching a freshly reset device's layout.
    pub fn build_layer_buffer(&self, layer: u8) -> Vec<u8> {
        let mut buffer = vec![0u8; KEYMATRIX_BUFFER_LEN];
        if let Some(slot_map) = self.working_keymap.get(&layer) {
            patch_buffer(&mut buffer, slot_map);
        }
        buffer
    }

    /// Writes every dirty keymap layer and, if any LED slot is dirty, the LED color
    /// buffer, to the device. On success, `device_keymap`/`device_led` become clones of
    /// the just-written `working_*` (clearing all dirty flags). On failure, `working_*` is
    /// left untouched so no in-progress edit is lost, and the error is returned for the
    /// caller to display — dirty flags remain so the caller can retry.
    pub fn save(
        &mut self,
        keymap_repo: &KeyMatrixRepository,
        led_repo: &LedColorRepository,
    ) -> hidapi::HidResult<()> {
        for layer in self.dirty_layers() {
            let buffer = self.build_layer_buffer(layer);
            keymap_repo.write_layer(layer, &buffer)?;
        }

        let led_dirty = (0..self.working_led.len() / 3).any(|slot| self.led_slot_dirty(slot as u16));
        if led_dirty {
            led_repo.enter_self_define()?;
            led_repo.write_colors(&self.working_led)?;
        }

        self.device_keymap = self.working_keymap.clone();
        self.device_led = self.working_led.clone();
        Ok(())
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

    #[test]
    fn dirty_layers_lists_only_layers_with_at_least_one_dirty_slot() {
        let device_keymap = map(&[(0, 7, 0xAA), (1, 8, 0xBB)]);
        let mut state = AppState::new(device_keymap, vec![], HashMap::new());
        state.working_keymap.get_mut(&0).unwrap().insert(7, 0xCC); // layer 0 dirty
        // layer 1 untouched
        assert_eq!(state.dirty_layers(), vec![0]);
    }

    #[test]
    fn dirty_layers_is_empty_when_nothing_changed() {
        let device_keymap = map(&[(0, 7, 0xAA)]);
        let state = AppState::new(device_keymap, vec![], HashMap::new());
        assert!(state.dirty_layers().is_empty());
    }

    #[test]
    fn build_layer_buffer_patches_working_slots_onto_a_zeroed_buffer() {
        use rk_a72_keymap::KEYMATRIX_BUFFER_LEN;
        let device_keymap = map(&[(0, 7, 0xAABBCCDD)]);
        let state = AppState::new(device_keymap, vec![], HashMap::new());
        let buf = state.build_layer_buffer(0);
        assert_eq!(buf.len(), KEYMATRIX_BUFFER_LEN);
        let slot_offset = 7 * 4;
        assert_eq!(&buf[slot_offset..slot_offset + 4], &[0xAA, 0xBB, 0xCC, 0xDD]);
    }

    #[test]
    fn build_layer_buffer_leaves_unmentioned_slots_zeroed() {
        use rk_a72_keymap::KEYMATRIX_BUFFER_LEN;
        let device_keymap = map(&[(0, 7, 0xAABBCCDD)]);
        let state = AppState::new(device_keymap, vec![], HashMap::new());
        let buf = state.build_layer_buffer(0);
        assert_eq!(buf.len(), KEYMATRIX_BUFFER_LEN);
        let other_offset = 8 * 4;
        assert_eq!(&buf[other_offset..other_offset + 4], &[0, 0, 0, 0]);
    }

    #[test]
    fn build_layer_buffer_reflects_working_not_device_state() {
        use rk_a72_keymap::KEYMATRIX_BUFFER_LEN;
        let device_keymap = map(&[(0, 7, 0xAABBCCDD)]);
        let mut state = AppState::new(device_keymap, vec![], HashMap::new());
        state.working_keymap.get_mut(&0).unwrap().insert(7, 0x11223344);
        let buf = state.build_layer_buffer(0);
        assert_eq!(buf.len(), KEYMATRIX_BUFFER_LEN);
        let slot_offset = 7 * 4;
        assert_eq!(&buf[slot_offset..slot_offset + 4], &[0x11, 0x22, 0x33, 0x44]);
    }
}
