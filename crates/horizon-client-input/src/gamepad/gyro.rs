//! Gyro aiming, as JibbSmart's GyroWiki describes good gyro controls: the gyro is a mouse. The
//! pad turning turns the view by as much times the sensitivity (1 is one to one), every filter is
//! optional and off by default, and the view turns in "player space", so a pad held at any angle
//! turns the view the way the player turns it.
//! <http://gyrowiki.jibbsmart.com/blog:good-gyro-controls-part-1:the-gyro-is-a-mouse>
//! <http://gyrowiki.jibbsmart.com/blog:player-space-gyro-and-alternatives-explained>
//! <http://gyrowiki.jibbsmart.com/blog:finding-gravity-with-sensor-fusion>
//!
//! Axes are the pad's own: x to the right, y up, z towards the player; a positive rate turns
//! counterclockwise looking down its axis.

/// When the gyro turns the view.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum GyroMode {
    #[default]
    Off,
    /// Always, except while the gyro button is held (the "gyro off" button: hold it to move the
    /// pad without turning, as one lifts a mouse).
    On,
    /// Only while the gyro button is held.
    Hold,
    /// The gyro button turns it on and off.
    Toggle,
}
impl GyroMode {
    pub const ALL: [Self; 4] = [Self::Off, Self::On, Self::Hold, Self::Toggle];
    pub fn name(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::On => "on",
            Self::Hold => "hold",
            Self::Toggle => "toggle",
        }
    }
    pub fn parse(value: &str) -> Self {
        Self::ALL
            .into_iter()
            .find(|mode| mode.name() == value)
            .unwrap_or_default()
    }
}

/// Which way of turning the pad turns the view left and right.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum GyroSpace {
    /// Turning the pad as the player turns, whichever way it is held (yaw and roll blended by
    /// gravity, with some slack): the GyroWiki's recommendation for a controller.
    #[default]
    Player,
    /// The pad's own yaw axis: what a handheld's screen turning does.
    Local,
    /// Turning about the real vertical only.
    World,
}
impl GyroSpace {
    pub const ALL: [Self; 3] = [Self::Player, Self::Local, Self::World];
    pub fn name(self) -> &'static str {
        match self {
            Self::Player => "player",
            Self::Local => "local",
            Self::World => "world",
        }
    }
    pub fn parse(value: &str) -> Self {
        Self::ALL
            .into_iter()
            .find(|space| space.name() == value)
            .unwrap_or_default()
    }
}

/// The gyro's settings (`horizon-controller.txt`), in GyroWiki's units so a player's numbers
/// carry over from other games and JoyShockMapper.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GyroSettings {
    pub mode: GyroMode,
    pub space: GyroSpace,
    /// Degrees the view turns for each degree the pad turns.
    pub sensitivity: f32,
    /// Up and down's share of `sensitivity`.
    pub vertical: f32,
    /// Below this many degrees a second the turn is averaged over a moment (0: off).
    pub smoothing: f32,
    /// Below this many degrees a second the turn is softened towards nothing (0: off).
    pub tightening: f32,
    /// The sensitivity's multiple at 75 °/s and over, from 1 (none) up.
    pub acceleration: f32,
    /// The pad learns its gyro's offset whenever it lies still.
    pub auto_calibrate: bool,
    /// The right stick, leant past its deadzone, pauses the gyro.
    pub stick_pauses: bool,
    /// A finger on a DualSense's touchpad pauses the gyro.
    pub touch_pauses: bool,
    pub invert_x: bool,
    pub invert_y: bool,
    /// In menus the gyro moves the pointer: screen widths for a whole turn of the pad
    /// (JoyShockMapper's 2D unit; 8 is a screen for 45 degrees). 0: off. It works whatever
    /// the aiming mode: a player may point with the pad and aim with the stick.
    pub cursor: f32,
    /// The pointer's own smoothing and tightening (°/s), heavier than aiming's: a pointer resting
    /// on a slot holds still in a hand (the GyroWiki's 2D example).
    pub cursor_smoothing: f32,
    pub cursor_tightening: f32,
}
impl Default for GyroSettings {
    fn default() -> Self {
        GyroSettings {
            mode: GyroMode::Off,
            space: GyroSpace::Player,
            sensitivity: 2.0,
            // Most players aim less up and down than across (mikaela's playtest).
            vertical: 0.75,
            // A light touch: a hand's tremor is steadied, a deliberate turn passes untouched.
            smoothing: 3.0,
            // A little of each: a hand's tremor is steadied, a deliberate turn passes untouched.
            tightening: 2.0,
            acceleration: 1.0,
            auto_calibrate: true,
            stick_pauses: false,
            touch_pauses: true,
            invert_x: false,
            invert_y: false,
            // Off: a pointer that moves with the pad surprises a player who only wanted to aim.
            cursor: 0.0,
            cursor_smoothing: 8.0,
            cursor_tightening: 6.0,
        }
    }
}

impl GyroSettings {
    /// The settings the menu pointer turns by: one to one in degrees, steadied, no acceleration.
    pub fn for_cursor(&self) -> GyroSettings {
        GyroSettings {
            sensitivity: 1.0,
            smoothing: self.cursor_smoothing,
            tightening: self.cursor_tightening,
            acceleration: 1.0,
            ..*self
        }
    }
}

/// How far the menu pointer moves, in pixels of a screen `width` wide, for the view turn
/// `update` gave with `for_cursor` (degrees, yaw right and pitch down).
pub fn cursor_pixels(turn: (f32, f32), cursor: f32, width: f32) -> (f32, f32) {
    let px = cursor * width / 360.0;
    (turn.0 * px, turn.1 * px)
}

/// The sensors over a frame: the gyro's mean rate (°/s) and the accelerometer's mean (g), the
/// pad's axes, over `seconds`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct MotionSample {
    pub gyro: [f32; 3],
    pub accel: [f32; 3],
    pub seconds: f32,
}

type V3 = [f32; 3];
fn dot(a: V3, b: V3) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn len(a: V3) -> f32 {
    dot(a, a).sqrt()
}
fn sub(a: V3, b: V3) -> V3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn add_scaled(a: V3, b: V3, s: f32) -> V3 {
    [a[0] + b[0] * s, a[1] + b[1] * s, a[2] + b[2] * s]
}
fn scale(a: V3, s: f32) -> V3 {
    [a[0] * s, a[1] * s, a[2] * s]
}
fn cross(a: V3, b: V3) -> V3 {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
/// `v` rotated by `angle` radians about the unit `axis` (Rodrigues).
fn rotate(v: V3, axis: V3, angle: f32) -> V3 {
    let (sin, cos) = angle.sin_cos();
    let k = cross(axis, v);
    let d = dot(axis, v) * (1.0 - cos);
    [
        v[0] * cos + k[0] * sin + axis[0] * d,
        v[1] * cos + k[1] * sin + axis[1] * d,
        v[2] * cos + k[2] * sin + axis[2] * d,
    ]
}
/// The weight of `x` between `lo` (0) and `hi` (1).
fn ramp(x: f32, lo: f32, hi: f32) -> f32 {
    if hi <= lo {
        return if x >= hi { 1.0 } else { 0.0 };
    }
    ((x - lo) / (hi - lo)).clamp(0.0, 1.0)
}
/// The share of a value an exponential average with this half-life keeps after `seconds`.
fn keep(seconds: f32, half_life: f32) -> f32 {
    (-seconds / half_life).exp2()
}

/// Player space's slack: how far the pad can lean before its roll starts to count less than a
/// turn would (1.41 is about 45 degrees; the GyroWiki suggests 1.15 to 2).
const YAW_RELAX: f32 = 1.41;
/// The speed (°/s) where acceleration reaches its full multiple.
const ACCELERATION_SPEED: f32 = 75.0;
/// How long smoothing averages over.
const SMOOTH_SECONDS: f32 = 0.125;
const SMOOTH_SLOTS: usize = 64;

// Finding gravity (the GyroWiki's sensor fusion, with its constants).
const SMOOTH_ACCEL_HALF_LIFE: f32 = 0.25;
const SHAKY_MIN: f32 = 0.01;
const SHAKY_MAX: f32 = 0.4;
const CORRECT_STILL: f32 = 1.0;
const CORRECT_SHAKY: f32 = 0.1;
const CORRECT_GYRO_SHARE: f32 = 0.1;
const CLOSE_MIN: f32 = 0.05;
const CLOSE_MAX: f32 = 0.25;
const CORRECT_MIN: f32 = 0.01;

// Calibrating on its own: a pad lying on a table is far stiller than one in a hand.
/// How much the accelerometer may wander (g) for the pad to count as lying still.
const STILL_ACCEL: f32 = 0.008;
/// How far the gyro may stray from its own recent mean (°/s).
const STILL_GYRO: f32 = 0.6;
/// How long it must stay so before its mean is taken as the offset.
const STILL_SECONDS: f32 = 0.6;
/// Once still, how fast the offset follows the mean (half-life).
const OFFSET_HALF_LIFE: f32 = 0.5;
const GYRO_MEAN_HALF_LIFE: f32 = 0.2;

/// The gyro's state, between frames.
#[derive(Clone, Debug)]
pub struct Gyro {
    /// What the gyro reads at rest, taken off every reading.
    offset: V3,
    calibrated: bool,
    /// A manual calibration in progress: the readings summed and the seconds.
    calibrating: Option<(V3, f32)>,
    gyro_mean: V3,
    still_for: f32,
    /// Gravity's direction, down, in the pad's axes, about 1 long.
    gravity: V3,
    smooth_accel: V3,
    shakiness: f32,
    seen: bool,
    /// The smoothing's recent slow turns, (yaw, pitch) rates, and how long each lasted.
    smooth: [([f32; 2], f32); SMOOTH_SLOTS],
    next: usize,
    /// Toggle mode: the gyro is on.
    toggled: bool,
    button_was: bool,
}
impl Default for Gyro {
    fn default() -> Self {
        Gyro {
            offset: [0.0; 3],
            calibrated: false,
            calibrating: None,
            gyro_mean: [0.0; 3],
            still_for: 0.0,
            gravity: [0.0, -1.0, 0.0],
            smooth_accel: [0.0, 1.0, 0.0],
            shakiness: 0.0,
            seen: false,
            smooth: [([0.0; 2], 0.0); SMOOTH_SLOTS],
            next: 0,
            toggled: false,
            button_was: false,
        }
    }
}

impl Gyro {
    /// Whether the gyro turns the view now, for the gyro button being held. Call once a frame,
    /// held or not, so Toggle sees each press once.
    pub fn enabled(&mut self, mode: GyroMode, button: bool) -> bool {
        let pressed = button && !self.button_was;
        self.button_was = button;
        if mode != GyroMode::Toggle {
            self.toggled = false;
        } else if pressed {
            self.toggled = !self.toggled;
        }
        match mode {
            GyroMode::Off => false,
            GyroMode::On => !button,
            GyroMode::Hold => button,
            GyroMode::Toggle => self.toggled,
        }
    }

    /// Start measuring the offset: the pad should lie still until `finish_calibration`.
    pub fn start_calibration(&mut self) {
        self.calibrating = Some(([0.0; 3], 0.0));
    }

    /// Take the mean measured since `start_calibration` as the offset; false when nothing came in.
    pub fn finish_calibration(&mut self) -> bool {
        match self.calibrating.take() {
            Some((sum, seconds)) if seconds > 0.0 => {
                self.offset = scale(sum, 1.0 / seconds);
                self.calibrated = true;
                true
            }
            _ => false,
        }
    }

    pub fn calibrating(&self) -> bool {
        self.calibrating.is_some()
    }

    /// The offset taken off the gyro, °/s.
    pub fn offset(&self) -> [f32; 3] {
        self.offset
    }

    /// The pad's sensors over a frame. Returns how far the view turns, `(yaw, pitch)` in degrees
    /// (yaw to the right, pitch down, as `look_delta`), when `aiming`; the sensors are followed
    /// either way, so turning the gyro on doesn't start from a stale gravity.
    pub fn update(&mut self, m: MotionSample, s: &GyroSettings, aiming: bool) -> (f32, f32) {
        let t = m.seconds;
        if t <= 0.0 || !m.gyro.iter().chain(&m.accel).all(|v| v.is_finite()) {
            return (0.0, 0.0);
        }
        self.learn_offset(m, s.auto_calibrate);
        let gyro = sub(m.gyro, self.offset);
        self.find_gravity(gyro, m.accel, t);
        if !aiming || self.calibrating.is_some() {
            self.forget_smoothing();
            return (0.0, 0.0);
        }
        let [yaw, pitch] = self.turn(gyro, s.space);
        let speed = yaw.hypot(pitch);
        // Tightening: slow turns shrink towards nothing, smoothly, never cut off.
        let tight = if s.tightening > 0.0 && speed < s.tightening {
            speed / s.tightening
        } else {
            1.0
        };
        let [yaw, pitch] = self.smoothed([yaw * tight, pitch * tight], speed, s.smoothing, t);
        let accel = 1.0 + (s.acceleration.max(1.0) - 1.0) * ramp(speed, 0.0, ACCELERATION_SPEED);
        let gain = s.sensitivity * accel * t;
        let x = if s.invert_x { -1.0 } else { 1.0 };
        let y = if s.invert_y { -1.0 } else { 1.0 };
        (yaw * gain * x, pitch * gain * s.vertical * y)
    }

    /// The view's turn rates (yaw right, pitch down, °/s) for the pad's.
    fn turn(&self, gyro: V3, space: GyroSpace) -> [f32; 2] {
        let down = self.gravity;
        let pitch = -gyro[0];
        match space {
            GyroSpace::Local => [-gyro[1], pitch],
            GyroSpace::Player => {
                // The turn about the real vertical, from yaw and roll only, given room to lean.
                let world = gyro[1] * down[1] + gyro[2] * down[2];
                let most = gyro[1].hypot(gyro[2]);
                [world.signum() * (world.abs() * YAW_RELAX).min(most), pitch]
            }
            GyroSpace::World => {
                let yaw = dot(gyro, down);
                // The pitch axis laid flat; it fades out as the pad stands on its side.
                let flat = sub([1.0, 0.0, 0.0], scale(down, down[0]));
                let l = len(flat);
                let pitch = if l > 1e-4 {
                    let side = ramp(down[1].abs().max(down[2].abs()), 0.125, 0.25);
                    -side * dot(gyro, scale(flat, 1.0 / l))
                } else {
                    0.0
                };
                [yaw, pitch]
            }
        }
    }

    /// Soft tiered smoothing: a turn faster than `threshold` passes straight through, one under
    /// half of it is averaged over `SMOOTH_SECONDS`, in between a blend. Every degree arrives.
    fn smoothed(&mut self, rate: [f32; 2], speed: f32, threshold: f32, t: f32) -> [f32; 2] {
        if threshold <= 0.0 {
            return rate;
        }
        let direct = ramp(speed, threshold / 2.0, threshold);
        let slow = [rate[0] * (1.0 - direct), rate[1] * (1.0 - direct)];
        self.smooth[self.next] = (slow, t);
        self.next = (self.next + 1) % SMOOTH_SLOTS;
        // The mean over the last SMOOTH_SECONDS, newest first.
        let (mut sum, mut time) = ([0.0f32; 2], 0.0f32);
        for i in 0..SMOOTH_SLOTS {
            let (r, dt) = self.smooth[(self.next + SMOOTH_SLOTS - 1 - i) % SMOOTH_SLOTS];
            let dt = dt.min(SMOOTH_SECONDS - time);
            if dt <= 0.0 {
                break;
            }
            sum = [sum[0] + r[0] * dt, sum[1] + r[1] * dt];
            time += dt;
        }
        // Over the whole window, quiet before the first slow turn included: each turn is spread
        // over the window's length and arrives whole.
        let mean = [sum[0] / SMOOTH_SECONDS, sum[1] / SMOOTH_SECONDS];
        [rate[0] * direct + mean[0], rate[1] * direct + mean[1]]
    }

    fn forget_smoothing(&mut self) {
        self.smooth = [([0.0; 2], 0.0); SMOOTH_SLOTS];
    }

    /// Manual calibration sums the readings; otherwise, with `auto` on, a pad lying still long
    /// enough has its offset follow the gyro's mean.
    fn learn_offset(&mut self, m: MotionSample, auto: bool) {
        let t = m.seconds;
        if let Some((sum, seconds)) = &mut self.calibrating {
            *sum = add_scaled(*sum, m.gyro, t);
            *seconds += t;
            return;
        }
        let k = keep(t, GYRO_MEAN_HALF_LIFE);
        self.gyro_mean = add_scaled(scale(self.gyro_mean, k), m.gyro, 1.0 - k);
        let wobble = len(sub(m.gyro, self.gyro_mean));
        let still = self.seen && self.shakiness < STILL_ACCEL && wobble < STILL_GYRO;
        self.still_for = if still { self.still_for + t } else { 0.0 };
        if auto && self.still_for >= STILL_SECONDS {
            if self.calibrated {
                let k = keep(t, OFFSET_HALF_LIFE);
                self.offset = add_scaled(scale(self.offset, k), self.gyro_mean, 1.0 - k);
            } else {
                self.offset = self.gyro_mean;
                self.calibrated = true;
            }
        }
    }

    /// Follow gravity: carried round by the gyro, pulled towards the accelerometer, more firmly
    /// the stiller the pad is.
    fn find_gravity(&mut self, gyro: V3, accel: V3, t: f32) {
        if !self.seen {
            self.seen = true;
            self.smooth_accel = accel;
            if len(accel) > 0.5 {
                self.gravity = scale(accel, -1.0 / len(accel));
            }
            return;
        }
        let w = scale(gyro, std::f32::consts::PI / 180.0);
        let speed = len(w);
        if speed > 0.0 {
            // The pad turned one way: in its own axes the world turned back.
            let axis = scale(w, -1.0 / speed);
            self.gravity = rotate(self.gravity, axis, speed * t);
            self.smooth_accel = rotate(self.smooth_accel, axis, speed * t);
        }
        let k = keep(t, SMOOTH_ACCEL_HALF_LIFE);
        self.shakiness = (self.shakiness * k).max(len(sub(accel, self.smooth_accel)));
        self.smooth_accel = add_scaled(scale(accel, 1.0 - k), self.smooth_accel, k);
        let target = scale(accel, -1.0);
        let delta = sub(target, self.gravity);
        let dist = len(delta);
        if dist <= 0.0 {
            return;
        }
        let mut rate = CORRECT_STILL
            + (CORRECT_SHAKY - CORRECT_STILL) * ramp(self.shakiness, SHAKY_MIN, SHAKY_MAX);
        let limit = speed * len(self.gravity) * CORRECT_GYRO_SHARE;
        if rate > limit {
            rate += (limit - rate) * ramp(dist, CLOSE_MIN, CLOSE_MAX);
        }
        let step = rate.max(CORRECT_MIN) * t;
        self.gravity = if step < dist {
            add_scaled(self.gravity, delta, step / dist)
        } else {
            target
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FLAT: V3 = [0.0, 1.0, 0.0];

    fn on() -> GyroSettings {
        GyroSettings {
            mode: GyroMode::On,
            sensitivity: 1.0,
            vertical: 1.0,
            smoothing: 0.0,
            auto_calibrate: false,
            ..GyroSettings::default()
        }
    }

    /// `frames` frames of `seconds` with the pad turning at `gyro` and reading `accel`.
    fn run(g: &mut Gyro, s: &GyroSettings, gyro: V3, accel: V3, frames: usize) -> (f32, f32) {
        let mut total = (0.0, 0.0);
        for _ in 0..frames {
            let (yaw, pitch) = g.update(
                MotionSample {
                    gyro,
                    accel,
                    seconds: 0.01,
                },
                s,
                true,
            );
            total = (total.0 + yaw, total.1 + pitch);
        }
        total
    }

    #[test]
    fn sensitivity_one_turns_the_view_as_far_as_the_pad() {
        let mut g = Gyro::default();
        // Lying flat, turned to the left at 90 °/s for a second, then tipped up at 30 °/s.
        let (yaw, pitch) = run(&mut g, &on(), [0.0, 90.0, 0.0], FLAT, 100);
        assert!(
            (yaw + 90.0).abs() < 0.01 && pitch.abs() < 0.01,
            "{yaw} {pitch}"
        );
        let mut g = Gyro::default();
        let (yaw, pitch) = run(&mut g, &on(), [30.0, 0.0, 0.0], FLAT, 100);
        assert!(
            yaw.abs() < 0.01 && (pitch + 30.0).abs() < 0.01,
            "{yaw} {pitch}"
        );
        // Twice the sensitivity, twice the turn; half the vertical, half the pitch.
        let s = GyroSettings {
            sensitivity: 2.0,
            vertical: 0.5,
            ..on()
        };
        let mut g = Gyro::default();
        let (yaw, pitch) = run(&mut g, &s, [30.0, -45.0, 0.0], FLAT, 100);
        assert!(
            (yaw - 90.0).abs() < 0.01 && (pitch + 30.0).abs() < 0.01,
            "{yaw} {pitch}"
        );
    }

    #[test]
    fn player_space_turns_a_pad_held_upright_by_its_roll() {
        // Held up facing the player (gravity along -z), turning the body left rolls the pad.
        let upright = [0.0, 0.0, 1.0];
        let mut g = Gyro::default();
        let (yaw, _) = run(&mut g, &on(), [0.0, 0.0, 60.0], upright, 100);
        assert!((yaw + 60.0).abs() < 0.5, "{yaw}");
        // Local space ignores that roll.
        let mut g = Gyro::default();
        let local = GyroSettings {
            space: GyroSpace::Local,
            ..on()
        };
        let (yaw, _) = run(&mut g, &local, [0.0, 0.0, 60.0], upright, 100);
        assert!(yaw.abs() < 0.01, "{yaw}");
    }

    #[test]
    fn the_modes_and_the_gyro_button() {
        let mut g = Gyro::default();
        assert!(!g.enabled(GyroMode::Off, false));
        assert!(g.enabled(GyroMode::On, false));
        assert!(!g.enabled(GyroMode::On, true));
        assert!(!g.enabled(GyroMode::Hold, false));
        assert!(g.enabled(GyroMode::Hold, true));
        g.enabled(GyroMode::Toggle, false);
        // A press turns it on and stays on while held and after; the next press turns it off.
        assert!(g.enabled(GyroMode::Toggle, true));
        assert!(g.enabled(GyroMode::Toggle, true));
        assert!(g.enabled(GyroMode::Toggle, false));
        assert!(!g.enabled(GyroMode::Toggle, true));
        assert!(!g.enabled(GyroMode::Toggle, false));
    }

    #[test]
    fn a_calibrated_offset_stops_the_drift() {
        let mut g = Gyro::default();
        let drift = [0.4, -1.2, 0.3];
        g.start_calibration();
        run(&mut g, &on(), drift, FLAT, 50);
        assert!(g.finish_calibration());
        let (yaw, pitch) = run(&mut g, &on(), drift, FLAT, 100);
        assert!(yaw.abs() < 1e-3 && pitch.abs() < 1e-3, "{yaw} {pitch}");
    }

    #[test]
    fn a_pad_lying_still_calibrates_itself_but_a_steady_turn_does_not() {
        let auto = GyroSettings {
            auto_calibrate: true,
            ..on()
        };
        let drift = [0.0, 0.5, 0.0];
        let mut g = Gyro::default();
        run(&mut g, &auto, drift, FLAT, 200);
        let (yaw, _) = run(&mut g, &auto, drift, FLAT, 100);
        assert!(yaw.abs() < 0.02, "{yaw}");
        // A slow deliberate turn wobbles the hand: the accelerometer isn't still.
        let mut g = Gyro::default();
        let mut total = 0.0;
        for i in 0..300 {
            let wobble = if i % 2 == 0 { 0.03 } else { -0.03 };
            total += run(&mut g, &auto, [0.0, 5.0, 0.0], [wobble, 1.0, 0.0], 1).0;
        }
        assert!((total + 15.0).abs() < 0.5, "{total}");
    }

    #[test]
    fn tightening_softens_slow_turns_and_leaves_fast_ones() {
        let s = GyroSettings {
            tightening: 10.0,
            ..on()
        };
        let mut g = Gyro::default();
        // 5 °/s is half the threshold: half the turn comes through.
        let (yaw, _) = run(&mut g, &s, [0.0, -5.0, 0.0], FLAT, 100);
        assert!((yaw - 2.5).abs() < 0.01, "{yaw}");
        let (yaw, _) = run(&mut g, &s, [0.0, -50.0, 0.0], FLAT, 100);
        assert!((yaw - 50.0).abs() < 0.01, "{yaw}");
    }

    #[test]
    fn smoothing_delays_slow_turns_but_loses_nothing() {
        let s = GyroSettings {
            smoothing: 10.0,
            ..on()
        };
        let mut g = Gyro::default();
        let first = run(&mut g, &s, [0.0, -2.0, 0.0], FLAT, 1).0;
        // The first frame of a slow turn arrives spread out.
        assert!(first < 0.02 * 0.5, "{first}");
        let rest = run(&mut g, &s, [0.0, -2.0, 0.0], FLAT, 99).0;
        let tail = run(&mut g, &s, [0.0; 3], FLAT, 20).0;
        assert!((first + rest + tail - 2.0).abs() < 1e-3);
    }

    #[test]
    fn acceleration_raises_fast_turns() {
        let s = GyroSettings {
            acceleration: 2.0,
            ..on()
        };
        let mut g = Gyro::default();
        let (yaw, _) = run(&mut g, &s, [0.0, -150.0, 0.0], FLAT, 100);
        assert!((yaw - 300.0).abs() < 0.01, "{yaw}");
    }

    #[test]
    fn a_turn_of_45_degrees_crosses_the_screen_at_cursor_8() {
        let (x, y) = cursor_pixels((45.0, -22.5), 8.0, 400.0);
        assert!(
            (x - 400.0).abs() < 1e-3 && (y + 200.0).abs() < 1e-3,
            "{x} {y}"
        );
    }

    #[test]
    fn nothing_turns_while_the_gyro_is_off() {
        let mut g = Gyro::default();
        let m = MotionSample {
            gyro: [0.0, 90.0, 0.0],
            accel: FLAT,
            seconds: 0.01,
        };
        assert_eq!(g.update(m, &on(), false), (0.0, 0.0));
    }
}
