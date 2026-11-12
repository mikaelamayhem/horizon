//! Controller Settings' gyro and flick stick rows (`Opt::PadGyro`): their names, values, sliders
//! and steps. Units are the GyroWiki's (sensitivity 1 turns the view as far as the pad, thresholds
//! in degrees a second), so numbers from other games and JoyShockMapper carry over.
use super::super::values::{generic, on_off, percent};
use super::super::{Kind, tr};
use crate::{
    gamepad::{GyroMode, GyroSpace},
    settings::{GyroOpt, Opt, Settings},
    text::Text,
};

impl GyroOpt {
    pub(crate) fn caption(self) -> &'static str {
        match self {
            GyroOpt::Mode => "horizon.options.gyro_mode",
            GyroOpt::Space => "horizon.options.gyro_space",
            GyroOpt::Sensitivity => "horizon.options.gyro_sensitivity",
            GyroOpt::Vertical => "horizon.options.gyro_vertical",
            GyroOpt::Smoothing => "horizon.options.gyro_smoothing",
            GyroOpt::Tightening => "horizon.options.gyro_tightening",
            GyroOpt::Acceleration => "horizon.options.gyro_acceleration",
            GyroOpt::AutoCalibrate => "horizon.options.gyro_auto_calibrate",
            GyroOpt::StickPauses => "horizon.options.gyro_stick_pauses",
            GyroOpt::TouchPauses => "horizon.options.gyro_touch_pauses",
            GyroOpt::InvertX => "horizon.options.gyro_invert_x",
            GyroOpt::InvertY => "horizon.options.gyro_invert_y",
            GyroOpt::Cursor => "horizon.options.gyro_cursor",
            GyroOpt::CursorSmoothing => "horizon.options.gyro_cursor_smoothing",
            GyroOpt::CursorTightening => "horizon.options.gyro_cursor_tightening",
            GyroOpt::FlickStick => "horizon.options.flick_stick",
        }
    }

    /// A slider's range and step: (low, high, steps).
    fn range(self) -> Option<(f32, f32, u32)> {
        Some(match self {
            GyroOpt::Sensitivity => (0.1, 10.0, 99),
            GyroOpt::Vertical => (0.0, 2.0, 40),
            GyroOpt::Smoothing
            | GyroOpt::Tightening
            | GyroOpt::Cursor
            | GyroOpt::CursorSmoothing
            | GyroOpt::CursorTightening => (0.0, 20.0, 20),
            GyroOpt::Acceleration => (1.0, 4.0, 30),
            _ => return None,
        })
    }

    fn value(self, s: &Settings) -> f32 {
        let g = &s.controller.gyro;
        match self {
            GyroOpt::Sensitivity => g.sensitivity,
            GyroOpt::Vertical => g.vertical,
            GyroOpt::Smoothing => g.smoothing,
            GyroOpt::Tightening => g.tightening,
            GyroOpt::Cursor => g.cursor,
            GyroOpt::CursorSmoothing => g.cursor_smoothing,
            GyroOpt::CursorTightening => g.cursor_tightening,
            _ => g.acceleration,
        }
    }

    /// The switches among the rows, for the screen's tick boxes.
    pub fn switch(self, s: &Settings) -> Option<bool> {
        let c = &s.controller;
        Some(match self {
            GyroOpt::AutoCalibrate => c.gyro.auto_calibrate,
            GyroOpt::StickPauses => c.gyro.stick_pauses,
            GyroOpt::TouchPauses => c.gyro.touch_pauses,
            GyroOpt::InvertX => c.gyro.invert_x,
            GyroOpt::InvertY => c.gyro.invert_y,
            GyroOpt::FlickStick => c.flick_stick,
            _ => return None,
        })
    }
}

/// A multiple, as JoyShockMapper writes it.
fn times(v: f32) -> Text {
    Text::literal(format!("{v:.1}x"))
}

/// The value after one step of a choice, round the ends.
fn step<T: Copy + PartialEq>(all: &[T], now: T, back: bool) -> T {
    let i = all.iter().position(|v| *v == now).unwrap_or(0);
    let n = all.len();
    all[if back { (i + n - 1) % n } else { (i + 1) % n }]
}

/// Each method answers for `Opt::PadGyro` and leaves every other option as Options' own default
/// arm would: a choice, no steps, its plain value, nothing to set.
impl Opt {
    pub(in super::super) fn gyro_kind(self, s: &Settings) -> Kind {
        let Opt::PadGyro(g) = self else {
            return Kind::Cycle;
        };
        match g.range() {
            Some((lo, hi, _)) => Kind::Slider(((g.value(s) - lo) / (hi - lo)).clamp(0.0, 1.0)),
            None => Kind::Cycle,
        }
    }

    pub(in super::super) fn gyro_slider_steps(self) -> Option<u32> {
        let Opt::PadGyro(g) = self else {
            return None;
        };
        g.range().map(|(_, _, steps)| steps)
    }

    pub(in super::super) fn gyro_set_slider(self, s: &mut Settings, v: f32) {
        let Opt::PadGyro(o) = self else {
            return;
        };
        let Some((lo, hi, steps)) = o.range() else {
            return;
        };
        let snapped = lo + (v.clamp(0.0, 1.0) * steps as f32).round() / steps as f32 * (hi - lo);
        let g = &mut s.controller.gyro;
        match o {
            GyroOpt::Sensitivity => g.sensitivity = snapped,
            GyroOpt::Vertical => g.vertical = snapped,
            GyroOpt::Smoothing => g.smoothing = snapped,
            GyroOpt::Tightening => g.tightening = snapped,
            GyroOpt::Cursor => g.cursor = snapped,
            GyroOpt::CursorSmoothing => g.cursor_smoothing = snapped,
            GyroOpt::CursorTightening => g.cursor_tightening = snapped,
            _ => g.acceleration = snapped,
        }
    }

    /// The whole label: a slider's own, else the caption and the value.
    pub(in super::super) fn gyro_label(self, s: &Settings, c: &str) -> Text {
        let slider = match self {
            Opt::PadGyro(o) if o.range().is_some() => o,
            _ => return generic(c, self.value_text(s)),
        };
        let v = slider.value(s);
        let off = || generic(c, tr("options.off"));
        match slider {
            GyroOpt::Vertical => percent(c, v),
            GyroOpt::Smoothing
            | GyroOpt::Tightening
            | GyroOpt::CursorSmoothing
            | GyroOpt::CursorTightening
                if v <= 0.0 =>
            {
                off()
            }
            GyroOpt::Smoothing
            | GyroOpt::Tightening
            | GyroOpt::CursorSmoothing
            | GyroOpt::CursorTightening => generic(
                c,
                Text::tr("horizon.options.degrees_per_second", &[&format!("{v:.0}")]),
            ),
            GyroOpt::Cursor if v <= 0.0 => off(),
            GyroOpt::Acceleration if v <= 1.0 => off(),
            _ => generic(c, times(v)),
        }
    }

    /// A choice's or a switch's value; nothing for other options.
    pub(in super::super) fn gyro_value_text(self, s: &Settings) -> Text {
        let Opt::PadGyro(o) = self else {
            return Text::empty();
        };
        if let Some(on) = o.switch(s) {
            return on_off(on);
        }
        let g = &s.controller.gyro;
        match o {
            GyroOpt::Mode => tr(match g.mode {
                GyroMode::Off => "options.off",
                GyroMode::On => "horizon.options.gyro_mode.on",
                GyroMode::Hold => "horizon.options.gyro_mode.hold",
                GyroMode::Toggle => "horizon.options.gyro_mode.toggle",
            }),
            GyroOpt::Space => tr(match g.space {
                GyroSpace::Player => "horizon.options.gyro_space.player",
                GyroSpace::Local => "horizon.options.gyro_space.local",
                GyroSpace::World => "horizon.options.gyro_space.world",
            }),
            _ => Text::empty(),
        }
    }

    /// A step of a choice or a flip of a switch.
    pub(in super::super) fn gyro_cycle(self, s: &mut Settings, back: bool) {
        let Opt::PadGyro(o) = self else {
            return;
        };
        let c = &mut s.controller;
        match o {
            GyroOpt::Mode => c.gyro.mode = step(&GyroMode::ALL, c.gyro.mode, back),
            GyroOpt::Space => c.gyro.space = step(&GyroSpace::ALL, c.gyro.space, back),
            GyroOpt::AutoCalibrate => c.gyro.auto_calibrate = !c.gyro.auto_calibrate,
            GyroOpt::StickPauses => c.gyro.stick_pauses = !c.gyro.stick_pauses,
            GyroOpt::TouchPauses => c.gyro.touch_pauses = !c.gyro.touch_pauses,
            GyroOpt::InvertX => c.gyro.invert_x = !c.gyro.invert_x,
            GyroOpt::InvertY => c.gyro.invert_y = !c.gyro.invert_y,
            GyroOpt::FlickStick => c.flick_stick = !c.flick_stick,
            _ => {}
        }
    }
}
