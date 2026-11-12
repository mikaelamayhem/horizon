//! The DualSense's buttons, sticks and triggers, read from the same input reports as its motion
//! (`motion.rs`). The game reads a pad here only when the system's controller API doesn't list
//! it (seen on Windows: joy.cpl has the pad, Windows.Gaming.Input doesn't).
//!
//! The state block's first bytes, as SDL's `PS5StatePacketCommon_t` and the kernel's
//! `dualsense_input_report`: the sticks (0..4, 0 is left and up), the triggers (4, 5), then the
//! hat and the buttons (7..10), and the touchpad's two touches (32, 36: the top bit set while
//! that finger is up).
use crate::motion::state_block;

/// One report's controls, raw.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RawInput {
    /// Left then right stick, x then y; 128 is the middle.
    pub sticks: [[u8; 2]; 2],
    /// Left then right trigger, 0 (released) to 255.
    pub triggers: [u8; 2],
    /// The hat and face buttons, then the shoulders, menu buttons and stick clicks, then the PS
    /// button, the touchpad's click and mute.
    pub buttons: [u8; 3],
    /// A finger rests on the touchpad.
    pub touching: bool,
}

/// The pad's buttons, by their PlayStation names.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DsButton {
    Cross,
    Circle,
    Square,
    Triangle,
    L1,
    R1,
    Create,
    Options,
    L3,
    R3,
    Ps,
    Touchpad,
    Up,
    Down,
    Left,
    Right,
}

/// The controls in an input report.
pub fn parse(report: &[u8]) -> Option<RawInput> {
    let b = state_block(report)?;
    Some(RawInput {
        sticks: [[b[0], b[1]], [b[2], b[3]]],
        triggers: [b[4], b[5]],
        buttons: [b[7], b[8], b[9]],
        touching: [32, 36]
            .iter()
            .any(|&at| b.get(at).is_some_and(|t| t & 0x80 == 0)),
    })
}

impl RawInput {
    pub fn pressed(&self, button: DsButton) -> bool {
        use DsButton as B;
        // The hat: 0 is up, then clockwise in eighths; 8 is centred.
        let hat = self.buttons[0] & 0x0f;
        let toward = |dirs: [u8; 3]| dirs.contains(&hat);
        let bit = |byte: usize, bit: u8| self.buttons[byte] & (1 << bit) != 0;
        match button {
            B::Square => bit(0, 4),
            B::Cross => bit(0, 5),
            B::Circle => bit(0, 6),
            B::Triangle => bit(0, 7),
            B::L1 => bit(1, 0),
            B::R1 => bit(1, 1),
            B::Create => bit(1, 4),
            B::Options => bit(1, 5),
            B::L3 => bit(1, 6),
            B::R3 => bit(1, 7),
            B::Ps => bit(2, 0),
            B::Touchpad => bit(2, 1),
            B::Up => toward([7, 0, 1]),
            B::Right => toward([1, 2, 3]),
            B::Down => toward([3, 4, 5]),
            B::Left => toward([5, 6, 7]),
        }
    }

    /// A stick (0 left, 1 right) from -1 to 1, up positive.
    pub fn stick(&self, which: usize) -> [f32; 2] {
        let axis = |v: u8| ((f32::from(v) - 128.0) / 127.0).clamp(-1.0, 1.0);
        let [x, y] = self.sticks[which];
        [axis(x), -axis(y)]
    }

    /// A trigger (0 left, 1 right) from 0 to 1.
    pub fn trigger(&self, which: usize) -> f32 {
        f32::from(self.triggers[which]) / 255.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_usb_report_gives_the_pads_controls() {
        let mut r = vec![0u8; 64];
        r[0] = 0x01;
        // Left stick full left and up, right stick resting, R2 fully pulled.
        r[1..7].copy_from_slice(&[0, 0, 128, 128, 0, 255]);
        // Hat down-right with cross; R1 and options; the PS button.
        r[8] = 0x03 | 0x20;
        r[9] = 0x02 | 0x20;
        r[10] = 0x01;
        // One finger up, the other down.
        r[33] = 0x80;
        r[37] = 0x01;
        let input = parse(&r).unwrap();
        assert!(input.touching);
        let held: Vec<DsButton> = [
            DsButton::Cross,
            DsButton::Circle,
            DsButton::Square,
            DsButton::Triangle,
            DsButton::L1,
            DsButton::R1,
            DsButton::Create,
            DsButton::Options,
            DsButton::L3,
            DsButton::R3,
            DsButton::Ps,
            DsButton::Touchpad,
            DsButton::Up,
            DsButton::Down,
            DsButton::Left,
            DsButton::Right,
        ]
        .into_iter()
        .filter(|b| input.pressed(*b))
        .collect();
        assert_eq!(
            held,
            [
                DsButton::Cross,
                DsButton::R1,
                DsButton::Options,
                DsButton::Ps,
                DsButton::Down,
                DsButton::Right
            ]
        );
        assert_eq!(input.stick(0), [-1.0, 1.0]);
        assert_eq!(input.stick(1), [0.0, 0.0]);
        assert_eq!((input.trigger(0), input.trigger(1)), (0.0, 1.0));
    }
}
