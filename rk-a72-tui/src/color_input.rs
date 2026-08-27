// ColorInput is not yet wired into any UI dialog — the interactive color-edit widget is
// future work (see main.rs's module doc comment). Kept `#[allow(dead_code)]` rather than
// silently ignored so it's clear this is expected, not an oversight.
#![allow(dead_code)]

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorChannel {
    R,
    G,
    B,
}

pub struct ColorInput {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub focused: ColorChannel,
    /// Hex digits typed for the focused channel since it last gained focus or committed a
    /// full byte — at most 2. A fresh digit past the second replaces the buffer (matches
    /// "typing keeps overwriting the low nibble" hex-input convention).
    entry_buffer: String,
}

impl ColorInput {
    pub fn new(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b, focused: ColorChannel::R, entry_buffer: String::new() }
    }

    pub fn focus_next(&mut self) {
        self.focused = match self.focused {
            ColorChannel::R => ColorChannel::G,
            ColorChannel::G => ColorChannel::B,
            ColorChannel::B => ColorChannel::R,
        };
        self.entry_buffer.clear();
    }

    pub fn focus_prev(&mut self) {
        self.focused = match self.focused {
            ColorChannel::R => ColorChannel::B,
            ColorChannel::G => ColorChannel::R,
            ColorChannel::B => ColorChannel::G,
        };
        self.entry_buffer.clear();
    }

    fn focused_mut(&mut self) -> &mut u8 {
        match self.focused {
            ColorChannel::R => &mut self.r,
            ColorChannel::G => &mut self.g,
            ColorChannel::B => &mut self.b,
        }
    }

    pub fn increment_focused(&mut self) {
        let v = self.focused_mut();
        *v = v.saturating_add(1);
        self.entry_buffer.clear();
    }

    pub fn decrement_focused(&mut self) {
        let v = self.focused_mut();
        *v = v.saturating_sub(1);
        self.entry_buffer.clear();
    }

    /// Only `0-9a-fA-F` are accepted; anything else is ignored. Two digits set the
    /// channel's byte value directly; a third digit starts a fresh two-digit entry
    /// (the buffer never holds more than 2 characters).
    pub fn type_hex_digit(&mut self, digit: char) {
        if !digit.is_ascii_hexdigit() {
            return;
        }
        if self.entry_buffer.len() >= 2 {
            self.entry_buffer.clear();
        }
        self.entry_buffer.push(digit);
        if self.entry_buffer.len() == 2 {
            let value = u8::from_str_radix(&self.entry_buffer, 16).expect("validated hex digits");
            *self.focused_mut() = value;
        }
    }

    pub fn rgb(&self) -> (u8, u8, u8) {
        (self.r, self.g, self.b)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_starts_focused_on_r() {
        let input = ColorInput::new(1, 2, 3);
        assert_eq!(input.focused, ColorChannel::R);
        assert_eq!(input.rgb(), (1, 2, 3));
    }

    #[test]
    fn focus_next_cycles_r_g_b_r() {
        let mut input = ColorInput::new(0, 0, 0);
        assert_eq!(input.focused, ColorChannel::R);
        input.focus_next();
        assert_eq!(input.focused, ColorChannel::G);
        input.focus_next();
        assert_eq!(input.focused, ColorChannel::B);
        input.focus_next();
        assert_eq!(input.focused, ColorChannel::R);
    }

    #[test]
    fn focus_prev_cycles_r_b_g_r() {
        let mut input = ColorInput::new(0, 0, 0);
        input.focus_prev();
        assert_eq!(input.focused, ColorChannel::B);
        input.focus_prev();
        assert_eq!(input.focused, ColorChannel::G);
        input.focus_prev();
        assert_eq!(input.focused, ColorChannel::R);
    }

    #[test]
    fn increment_focused_channel_by_one() {
        let mut input = ColorInput::new(10, 0, 0);
        input.increment_focused();
        assert_eq!(input.r, 11);
    }

    #[test]
    fn increment_clamps_at_255() {
        let mut input = ColorInput::new(255, 0, 0);
        input.increment_focused();
        assert_eq!(input.r, 255);
    }

    #[test]
    fn decrement_focused_channel_by_one() {
        let mut input = ColorInput::new(10, 0, 0);
        input.decrement_focused();
        assert_eq!(input.r, 9);
    }

    #[test]
    fn decrement_clamps_at_zero() {
        let mut input = ColorInput::new(0, 0, 0);
        input.decrement_focused();
        assert_eq!(input.r, 0);
    }

    #[test]
    fn typing_two_hex_digits_sets_the_focused_channel() {
        let mut input = ColorInput::new(0, 0, 0);
        input.type_hex_digit('a');
        input.type_hex_digit('f');
        assert_eq!(input.r, 0xaf);
    }

    #[test]
    fn typing_uppercase_hex_digits_works() {
        let mut input = ColorInput::new(0, 0, 0);
        input.type_hex_digit('F');
        input.type_hex_digit('F');
        assert_eq!(input.r, 0xFF);
    }

    #[test]
    fn a_third_digit_starts_a_fresh_two_digit_entry() {
        let mut input = ColorInput::new(0, 0, 0);
        input.type_hex_digit('a');
        input.type_hex_digit('f');
        assert_eq!(input.r, 0xaf);
        input.type_hex_digit('0');
        input.type_hex_digit('1');
        assert_eq!(input.r, 0x01);
    }

    #[test]
    fn non_hex_characters_are_ignored() {
        let mut input = ColorInput::new(0, 0, 0);
        input.type_hex_digit('g');
        input.type_hex_digit('z');
        input.type_hex_digit(' ');
        assert_eq!(input.r, 0);
    }

    #[test]
    fn changing_focus_clears_the_pending_digit_entry() {
        let mut input = ColorInput::new(0, 0, 0);
        input.type_hex_digit('a'); // one digit typed, not yet committed
        input.focus_next();
        input.focus_prev();
        // back on R, but the single pending 'a' should have been cleared by the focus
        // change — typing one more digit should NOT combine with the stale 'a'.
        input.type_hex_digit('1');
        input.type_hex_digit('2');
        assert_eq!(input.r, 0x12);
    }

    #[test]
    fn typing_only_affects_the_focused_channel() {
        let mut input = ColorInput::new(0, 0, 0);
        input.focus_next(); // now on G
        input.type_hex_digit('5');
        input.type_hex_digit('5');
        assert_eq!(input.r, 0);
        assert_eq!(input.g, 0x55);
        assert_eq!(input.b, 0);
    }
}
