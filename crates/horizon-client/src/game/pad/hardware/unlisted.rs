//! A DualSense that gilrs doesn't list. On Windows, joy.cpl can show the pad while
//! Windows.Gaming.Input (gilrs' backend there) doesn't. The output thread then reads the pad's
//! buttons and sticks from its own reports, the same reports the gyro reads.
use super::{Devices, TAKEOVER, is_dualsense};
use crate::game::pad::{Connection, Pads};
use horizon_client::controller_ui::Device;
use horizon_client::gamepad::{Button, GlyphStyle, PadState};
use horizon_dualsense::input::{DsButton, RawInput};
use std::time::{Duration, Instant};

/// How often to look for an unlisted pad while none is open: nothing else says when one is
/// plugged in.
const RESCAN: Duration = Duration::from_secs(3);
/// How long the pad may be gone before it counts as unplugged: Windows sometimes drops the
/// handle of a pad still plugged in, and the thread opens it again within seconds.
const GRACE: Duration = Duration::from_secs(10);

/// Whether this system needs it: elsewhere gilrs lists every DualSense, and the HID scan would
/// be wasted work. `HORIZON_DUALSENSE_HID=1` turns it on anywhere, `0` off.
fn wanted() -> bool {
    match std::env::var_os("HORIZON_DUALSENSE_HID") {
        Some(v) => v == "1",
        None => cfg!(windows),
    }
}

/// The unlisted pad's part of `Devices`.
#[derive(Default)]
pub(super) struct Unlisted {
    /// The output thread was asked to read the buttons and sticks.
    reading: bool,
    /// A report came in at the last read: the pad is there.
    present: bool,
    /// It is the pad being played.
    pub(super) active: bool,
    /// Something on it was held at the last read, so a press only takes over once.
    touched: bool,
    next_scan: Option<Instant>,
    /// When reports stopped, while that is shorter than [`GRACE`].
    missing_since: Option<Instant>,
}

impl Unlisted {
    /// The output closed (controllers off, splitscreen took the pads): start again.
    pub(super) fn reset(&mut self) {
        *self = Unlisted::default();
    }
}

impl Pads {
    /// An unlisted DualSense's controls, starting its output to read them, and its connection
    /// notices. `None` when gilrs lists a DualSense or none is plugged in.
    pub(super) fn read_unlisted(&mut self) -> Option<PadState> {
        let listed = self
            .devices
            .gilrs
            .as_ref()?
            .gamepads()
            .any(|(_, p)| is_dualsense(&p));
        let d: &mut Devices = &mut self.devices;
        if listed || !wanted() {
            if d.unlisted.reading
                && let Some(output) = &d.output
            {
                output.read_input(false);
            }
            d.unlisted.reset();
            return None;
        }
        if d.output.is_none() {
            d.output = Some(horizon_dualsense::Link::start_unlisted());
            d.reading_motion = false;
            d.unlisted.reset();
        }
        let output = d.output.as_ref()?;
        if !d.unlisted.reading {
            output.read_input(true);
            d.unlisted.reading = true;
        }
        let now = Instant::now();
        if output.open().is_none() && d.unlisted.next_scan.is_none_or(|at| now >= at) {
            if d.unlisted.next_scan.is_some() {
                output.scan();
            }
            d.unlisted.next_scan = Some(now + RESCAN);
        }
        let state = output.input().map(|input| state(&input));
        if state.is_some() {
            d.unlisted.missing_since = None;
            if !d.unlisted.present {
                d.unlisted.present = true;
                log::info!("Controller connected: DualSense (read over HID)");
                self.connections
                    .push(Connection::Connected(GlyphStyle::PlayStation));
            }
        } else if d.unlisted.present && now - *d.unlisted.missing_since.get_or_insert(now) >= GRACE
        {
            d.unlisted.present = false;
            d.unlisted.missing_since = None;
            log::info!("Controller disconnected: DualSense (read over HID)");
            self.connections.push(Connection::Disconnected {
                active: d.unlisted.active,
            });
            d.unlisted.active = false;
        }
        let touched = state.as_ref().is_some_and(touched);
        if touched && !d.unlisted.touched {
            d.unlisted.active = true;
            d.active = None;
        }
        d.unlisted.touched = touched;
        state
    }
}

/// What the controller settings say about it.
pub(super) fn device() -> Device {
    Device {
        name: "DualSense".to_owned(),
        vendor: Some(horizon_dualsense::SONY),
        product: Some(horizon_dualsense::DUALSENSE),
    }
}

/// A report's controls as a pad's state.
fn state(input: &RawInput) -> PadState {
    use {Button as B, DsButton as D};
    let mut state = PadState::default();
    for (button, ds) in [
        (B::South, D::Cross),
        (B::East, D::Circle),
        (B::West, D::Square),
        (B::North, D::Triangle),
        (B::LeftBumper, D::L1),
        (B::RightBumper, D::R1),
        (B::Select, D::Create),
        (B::Start, D::Options),
        (B::LeftStick, D::L3),
        (B::RightStick, D::R3),
        (B::DPadUp, D::Up),
        (B::DPadDown, D::Down),
        (B::DPadLeft, D::Left),
        (B::DPadRight, D::Right),
    ] {
        state.set(button, input.pressed(ds));
    }
    state.left = input.stick(0);
    state.right = input.stick(1);
    state.triggers = [input.trigger(0), input.trigger(1)];
    state
}

/// Someone is using it: a button, or a stick or trigger past the takeover.
fn touched(state: &PadState) -> bool {
    state.buttons != 0
        || [state.left, state.right, state.triggers]
            .iter()
            .flatten()
            .any(|v| v.abs() > TAKEOVER)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_resting_pad_doesnt_take_over_and_a_press_does() {
        let resting = RawInput {
            sticks: [[128, 128], [130, 126]],
            triggers: [0, 3],
            buttons: [0x08, 0, 0],
        };
        assert!(!touched(&state(&resting)));
        let cross = RawInput {
            buttons: [0x08 | 0x20, 0, 0],
            ..resting
        };
        let s = state(&cross);
        assert!(touched(&s) && s.held(Button::South) && !s.held(Button::DPadUp));
    }
}
