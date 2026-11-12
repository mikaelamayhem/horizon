//! The gyro's settings in `horizon-controller.txt`, kept for each kind of controller: pads differ
//! in how their gyro feels and in which buttons they have, so a player sets the gyro and flick
//! stick once per kind and the game brings back the right ones when that kind is used. Everything
//! else (look sensitivity, bindings...) stays shared.
use super::gyro::{GyroMode, GyroSettings, GyroSpace};

/// A kind of controller whose gyro settings are kept apart.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum GyroFamily {
    #[default]
    PlayStation,
    Nintendo,
    Xbox,
    Steam,
    Other,
}

impl GyroFamily {
    pub const ALL: [GyroFamily; 5] = [
        GyroFamily::PlayStation,
        GyroFamily::Nintendo,
        GyroFamily::Xbox,
        GyroFamily::Steam,
        GyroFamily::Other,
    ];

    pub fn name(self) -> &'static str {
        match self {
            GyroFamily::PlayStation => "playstation",
            GyroFamily::Nintendo => "nintendo",
            GyroFamily::Xbox => "xbox",
            GyroFamily::Steam => "steam",
            GyroFamily::Other => "other",
        }
    }

    pub fn parse(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|f| f.name() == name)
    }

    /// The kind of a controller, by its USB ids: Sony, Nintendo, Microsoft, and Valve or HORI's
    /// Horipad for Steam.
    pub fn of(vendor: Option<u16>, product: Option<u16>) -> Self {
        match (vendor, product) {
            (Some(0x054c), _) => GyroFamily::PlayStation,
            (Some(0x057e), _) => GyroFamily::Nintendo,
            (Some(0x045e), _) => GyroFamily::Xbox,
            (Some(0x28de), _) | (Some(0x0f0d), Some(0x01ab)) => GyroFamily::Steam,
            _ => GyroFamily::Other,
        }
    }
}

/// One kind of controller's gyro settings and flick stick.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct GyroProfile {
    pub gyro: GyroSettings,
    pub flick_stick: bool,
}

fn bool_text(v: bool) -> &'static str {
    if v { "true" } else { "false" }
}

impl GyroProfile {
    /// Read one line's setting (`key` without any family prefix); false when it isn't one of the
    /// gyro's. A value out of range is clamped, one that doesn't parse keeps the old.
    pub fn read(&mut self, key: &str, value: &str) -> bool {
        let flag = |d: bool| match value {
            "true" => true,
            "false" => false,
            _ => d,
        };
        let bounded = |default: f32, min: f32, max: f32| {
            value
                .parse::<f32>()
                .ok()
                .filter(|v| v.is_finite())
                .map_or(default, |v| v.clamp(min, max))
        };
        let g = &mut self.gyro;
        match key {
            "flickStick" => self.flick_stick = flag(self.flick_stick),
            "gyroMode" => g.mode = GyroMode::parse(value),
            "gyroSpace" => g.space = GyroSpace::parse(value),
            "gyroSensitivity" => g.sensitivity = bounded(g.sensitivity, 0.1, 20.0),
            "gyroVertical" => g.vertical = bounded(g.vertical, 0.0, 2.0),
            "gyroSmoothing" => g.smoothing = bounded(g.smoothing, 0.0, 50.0),
            "gyroTightening" => g.tightening = bounded(g.tightening, 0.0, 50.0),
            "gyroAcceleration" => g.acceleration = bounded(g.acceleration, 1.0, 4.0),
            "gyroAutoCalibrate" => g.auto_calibrate = flag(g.auto_calibrate),
            "gyroStickPauses" => g.stick_pauses = flag(g.stick_pauses),
            "gyroTouchPauses" => g.touch_pauses = flag(g.touch_pauses),
            "gyroInvertX" => g.invert_x = flag(g.invert_x),
            "gyroInvertY" => g.invert_y = flag(g.invert_y),
            "gyroCursor" => g.cursor = bounded(g.cursor, 0.0, 20.0),
            "gyroCursorSmoothing" => g.cursor_smoothing = bounded(g.cursor_smoothing, 0.0, 50.0),
            "gyroCursorTightening" => g.cursor_tightening = bounded(g.cursor_tightening, 0.0, 50.0),
            _ => return false,
        }
        true
    }

    /// Its lines, each key after `prefix`.
    pub fn lines(&self, prefix: &str) -> String {
        let g = &self.gyro;
        [
            ("flickStick", bool_text(self.flick_stick).to_owned()),
            ("gyroMode", g.mode.name().to_owned()),
            ("gyroSpace", g.space.name().to_owned()),
            ("gyroSensitivity", g.sensitivity.to_string()),
            ("gyroVertical", g.vertical.to_string()),
            ("gyroSmoothing", g.smoothing.to_string()),
            ("gyroTightening", g.tightening.to_string()),
            ("gyroAcceleration", g.acceleration.to_string()),
            ("gyroAutoCalibrate", bool_text(g.auto_calibrate).to_owned()),
            ("gyroStickPauses", bool_text(g.stick_pauses).to_owned()),
            ("gyroTouchPauses", bool_text(g.touch_pauses).to_owned()),
            ("gyroInvertX", bool_text(g.invert_x).to_owned()),
            ("gyroInvertY", bool_text(g.invert_y).to_owned()),
            ("gyroCursor", g.cursor.to_string()),
            ("gyroCursorSmoothing", g.cursor_smoothing.to_string()),
            ("gyroCursorTightening", g.cursor_tightening.to_string()),
        ]
        .into_iter()
        .map(|(key, value)| format!("{prefix}{key}:{value}\n"))
        .collect()
    }
}

/// The prefix of another kind's lines: `gyro.<family>.`.
pub fn family_prefix(family: GyroFamily) -> String {
    format!("gyro.{}.", family.name())
}

/// A line of another kind's settings: that kind and the key after the prefix.
pub fn family_key(key: &str) -> Option<(GyroFamily, &str)> {
    let (family, rest) = key.strip_prefix("gyro.")?.split_once('.')?;
    Some((GyroFamily::parse(family)?, rest))
}
