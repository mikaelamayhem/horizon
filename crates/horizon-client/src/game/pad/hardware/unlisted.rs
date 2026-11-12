//! A DualSense that gilrs doesn't list. On Windows, joy.cpl can show the pad while
//! Windows.Gaming.Input (gilrs' backend there) doesn't. The output thread then reads the pad's
//! buttons and sticks from its own reports, the same reports the gyro reads.
use super::{Devices, TAKEOVER, is_dualsense};
use crate::game::pad::{Connection, Pads};
use horizon_client::controller_ui::Device;
use horizon_client::gamepad::{Button, GlyphStyle, PadState};
use horizon_dualsense::Link;
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
pub(super) fn wanted() -> bool {
    match std::env::var_os("HORIZON_DUALSENSE_HID") {
        Some(v) => v == "1",
        None => cfg!(windows),
    }
}

/// Reading an unlisted pad through a `Link`: asking for its controls, looking for it again
/// while it is gone, and saying when it came or went. P1's own reader and splitscreen's router
/// each keep one.
#[derive(Default)]
pub(super) struct Watch {
    /// The link was asked to read the buttons and sticks.
    reading: bool,
    /// A report came in at the last read: the pad is there.
    present: bool,
    next_scan: Option<Instant>,
    /// When reports stopped, while that is shorter than [`GRACE`].
    missing_since: Option<Instant>,
}

impl Watch {
    /// The pad's controls now, and whether it just came (`Some(true)`) or went (`Some(false)`).
    pub(super) fn poll(&mut self, link: &Link) -> (Option<PadState>, Option<bool>) {
        if !self.reading {
            link.read_input(true);
            self.reading = true;
        }
        let now = Instant::now();
        if link.open().is_none() && self.next_scan.is_none_or(|at| now >= at) {
            if self.next_scan.is_some() {
                link.scan();
            }
            self.next_scan = Some(now + RESCAN);
        }
        let state = link.input().map(|input| state(&input));
        let mut change = None;
        if state.is_some() {
            self.missing_since = None;
            if !self.present {
                self.present = true;
                log::info!("Controller connected: DualSense (read over HID)");
                change = Some(true);
            }
        } else if self.present && now - *self.missing_since.get_or_insert(now) >= GRACE {
            self.present = false;
            self.missing_since = None;
            log::info!("Controller disconnected: DualSense (read over HID)");
            change = Some(false);
        }
        (state, change)
    }

    /// It is reading an unlisted pad's controls.
    pub(super) fn reading(&self) -> bool {
        self.reading
    }

    /// The pad is there (or gone for less than [`GRACE`]).
    pub(super) fn present(&self) -> bool {
        self.present
    }

    /// Stop reading the controls (gilrs lists the pad now).
    pub(super) fn stop(&mut self, link: Option<&Link>) {
        if self.reading
            && let Some(link) = link
        {
            link.read_input(false);
        }
        *self = Watch::default();
    }
}

/// The unlisted pad's part of `Devices`.
#[derive(Default)]
pub(super) struct Unlisted {
    watch: Watch,
    /// It is the pad being played.
    pub(super) active: bool,
    /// The splitscreen router's pad it is, while this player borrows its link (`direct.rs`).
    pub(super) key: Option<u32>,
    /// Something on it was held at the last read, so a press only takes over once.
    touched: bool,
}

impl Unlisted {
    /// Its link reads the pad's controls for the game (so nothing else may stop that).
    pub(super) fn reading(&self) -> bool {
        self.watch.reading() || self.key.is_some()
    }

    /// The output closed (controllers off, splitscreen took the pads): start again.
    pub(super) fn reset(&mut self) {
        *self = Unlisted::default();
    }
}

/// A DualSense gilrs lists, left alone where the game reads every DualSense directly (so it
/// isn't two pads).
pub(super) fn ignored(pad: &gilrs::Gamepad) -> bool {
    wanted() && is_dualsense(pad)
}

impl Pads {
    /// An unlisted DualSense's controls, starting its output to read them, and its connection
    /// notices. `None` when gilrs lists a DualSense or none is plugged in.
    pub(super) fn read_unlisted(&mut self) -> Option<PadState> {
        self.devices.gilrs.as_ref()?;
        let d: &mut Devices = &mut self.devices;
        if !wanted() {
            d.unlisted.watch.stop(d.output.as_ref());
            d.unlisted.reset();
            return None;
        }
        if d.output.is_none() {
            d.output = Some(Link::start_unlisted());
            d.reading_motion = false;
            d.unlisted.reset();
        }
        let (state, change) = d.unlisted.watch.poll(d.output.as_ref()?);
        match change {
            Some(true) => self
                .connections
                .push(Connection::Connected(GlyphStyle::PlayStation)),
            Some(false) => {
                self.connections.push(Connection::Disconnected {
                    active: d.unlisted.active,
                });
                d.unlisted.active = false;
            }
            None => {}
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
    add_touchpad(&mut state, input);
    state.left = input.stick(0);
    state.right = input.stick(1);
    state.triggers = [input.trigger(0), input.trigger(1)];
    state
}

/// The touchpad's click and touch, which gilrs doesn't report, from a DualSense's own report.
pub(super) fn add_touchpad(state: &mut PadState, input: &RawInput) {
    state.set(Button::TouchpadClick, input.pressed(DsButton::Touchpad));
    state.set(Button::TouchpadTouch, input.touching);
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
            touching: false,
        };
        assert!(!touched(&state(&resting)));
        let cross = RawInput {
            buttons: [0x08 | 0x20, 0, 0],
            ..resting
        };
        let s = state(&cross);
        assert!(touched(&s) && s.held(Button::South) && !s.held(Button::DPadUp));
        // A finger on the touchpad is a button of its own, to bind.
        let finger = state(&RawInput {
            touching: true,
            ..resting
        });
        assert!(finger.held(Button::TouchpadTouch) && !finger.held(Button::TouchpadClick));
    }
}
