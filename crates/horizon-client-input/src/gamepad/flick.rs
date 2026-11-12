//! Flick stick (JibbSmart's, as JoyShockMapper does it): the right stick points the view. Push it
//! to the edge and the view turns to face where it points, relative to now, over a tenth of a
//! second; held at the edge, rotating it turns the view by as much as the stick turned. It only
//! turns the view left and right: up and down are the gyro's, or nothing's.
//! <http://gyrowiki.jibbsmart.com/blog:good-gyro-controls-part-2:the-flick-stick>

/// How far the stick leans before a flick starts.
pub const FLICK_START: f32 = 0.9;
/// How far it comes back before the next flick may start: a stick resting on the threshold
/// doesn't flick again and again.
pub const FLICK_RELEASE: f32 = 0.8;
/// How long a flick takes, in seconds (JoyShockMapper's default; 0.2 already feels slow).
pub const FLICK_TIME: f32 = 0.1;
/// Below this much stick turn in a frame (degrees), the turn goes through the smoothing; at half
/// of it, all of it does. A pad's 8-bit axes make a held stick's angle jitter by about a degree.
const SMOOTH_FULL: f32 = 6.0;
const SMOOTH_HALF: f32 = SMOOTH_FULL / 2.0;
/// Frames the smoothed part of a turn is spread over.
const SMOOTH_FRAMES: usize = 8;

/// The flick stick's state, between frames.
#[derive(Clone, Debug, Default)]
pub struct FlickStick {
    /// The stick is past the edge (between `FLICK_START` and `FLICK_RELEASE` it stays as it was).
    out: bool,
    /// The stick's angle at the last frame it was out, degrees clockwise from up.
    angle: f32,
    /// The flick in progress: its whole turn in degrees, and the seconds gone.
    flick: Option<(f32, f32)>,
    smooth: [f32; SMOOTH_FRAMES],
    next: usize,
}

/// The stick's direction in degrees clockwise from straight up (right is +90, left -90).
fn angle(stick: [f32; 2]) -> f32 {
    stick[0].atan2(stick[1]).to_degrees()
}

/// Ease out only, so the turn starts at once (`1 - (1 - t)²`).
fn ease(t: f32) -> f32 {
    let u = 1.0 - t.clamp(0.0, 1.0);
    1.0 - u * u
}

/// `degrees` brought into -180..=180.
fn wrap(degrees: f32) -> f32 {
    let d = (degrees + 180.0).rem_euclid(360.0) - 180.0;
    if d == -180.0 { 180.0 } else { d }
}

impl FlickStick {
    /// The view's turn to the right this frame, in degrees, for the stick's raw position (no
    /// deadzone: only its edge matters) after `seconds`.
    pub fn update(&mut self, stick: [f32; 2], seconds: f32) -> f32 {
        let lean = (stick[0] * stick[0] + stick[1] * stick[1]).sqrt();
        let mut yaw = 0.0;
        let was_out = self.out;
        self.out = if was_out {
            lean >= FLICK_RELEASE
        } else {
            lean >= FLICK_START
        };
        if self.out {
            let now = angle(stick);
            if was_out {
                yaw += self.smoothed(wrap(now - self.angle));
            } else {
                // A new flick replaces what was left of the last one.
                self.flick = Some((now, 0.0));
            }
            self.angle = now;
        } else if was_out {
            // Let go: the view stops at once, nothing left in the smoothing.
            self.smooth = [0.0; SMOOTH_FRAMES];
        }
        if let Some((size, gone)) = self.flick {
            let after = (gone + seconds.max(0.0)).min(FLICK_TIME);
            yaw += size * (ease(after / FLICK_TIME) - ease(gone / FLICK_TIME));
            self.flick = (after < FLICK_TIME).then_some((size, after));
        }
        yaw
    }

    /// Forget everything: the stick went to a menu, or the view stopped being the pad's.
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    /// Soft tiered smoothing: a big turn passes straight through, a small one (a held stick's
    /// jitter) is averaged over the last frames. Nothing is lost: every degree arrives.
    fn smoothed(&mut self, turn: f32) -> f32 {
        let direct = ((turn.abs() - SMOOTH_HALF) / (SMOOTH_FULL - SMOOTH_HALF)).clamp(0.0, 1.0);
        self.smooth[self.next] = turn * (1.0 - direct);
        self.next = (self.next + 1) % SMOOTH_FRAMES;
        turn * direct + self.smooth.iter().sum::<f32>() / SMOOTH_FRAMES as f32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Runs frames of `seconds` with the stick at `stick`, summing the turn.
    fn run(flick: &mut FlickStick, stick: [f32; 2], frames: usize, seconds: f32) -> f32 {
        (0..frames).map(|_| flick.update(stick, seconds)).sum()
    }

    #[test]
    fn a_flick_turns_to_face_the_stick_over_a_tenth_of_a_second() {
        let mut flick = FlickStick::default();
        // Right, at full tilt: a quarter turn to the right.
        let first = flick.update([1.0, 0.0], 1.0 / 60.0);
        assert!(first > 0.0 && first < 90.0, "{first}");
        let total = first + run(&mut flick, [1.0, 0.0], 30, 1.0 / 60.0);
        assert!((total - 90.0).abs() < 1e-3, "{total}");
        // Behind you, to the left: a half turn.
        let mut flick = FlickStick::default();
        let total = run(&mut flick, [-0.0001, -1.0], 30, 1.0 / 60.0);
        assert!((total.abs() - 180.0).abs() < 0.1, "{total}");
    }

    #[test]
    fn the_flick_finishes_after_the_stick_is_let_go() {
        let mut flick = FlickStick::default();
        let mut total = flick.update([0.0, -1.0], 0.01);
        total += run(&mut flick, [0.0, 0.0], 20, 0.01);
        assert!((total.abs() - 180.0).abs() < 1e-3, "{total}");
    }

    #[test]
    fn rotating_the_held_stick_turns_by_as_much() {
        let mut flick = FlickStick::default();
        // Flick forward: no turn.
        let mut total = run(&mut flick, [0.0, 1.0], 10, 0.02);
        assert!(total.abs() < 1e-4);
        // Rotate clockwise to the right in 45 steps of 2 degrees, then let go.
        for step in 1..=45 {
            let a = (step as f32 * 2.0).to_radians();
            total += flick.update([a.sin(), a.cos()], 0.02);
        }
        total += flick.update([0.0, 0.0], 0.02);
        // The smoothed part still arrives: everything but what the release dropped.
        assert!(total > 80.0 && total <= 90.0 + 1e-3, "{total}");
        // A fast rotation passes straight through, so nothing is dropped.
        let mut flick = FlickStick::default();
        run(&mut flick, [0.0, 1.0], 10, 0.02);
        let fast = flick.update([1.0, 0.0], 0.02);
        assert!((fast - 90.0).abs() < 1e-3, "{fast}");
    }

    #[test]
    fn a_stick_resting_on_the_edge_flicks_once() {
        let mut flick = FlickStick::default();
        let mut total = run(&mut flick, [0.95, 0.0], 20, 0.01);
        // Dipping below the start but not the release, and back: no second flick.
        total += run(&mut flick, [0.85, 0.0], 5, 0.01);
        total += run(&mut flick, [0.95, 0.0], 20, 0.01);
        assert!((total - 90.0).abs() < 1e-3, "{total}");
        // Back to the middle and out again: a second flick.
        total += run(&mut flick, [0.0, 0.0], 5, 0.01);
        total += run(&mut flick, [0.95, 0.0], 20, 0.01);
        assert!((total - 180.0).abs() < 1e-3, "{total}");
    }

    #[test]
    fn a_tilt_short_of_the_edge_does_nothing() {
        let mut flick = FlickStick::default();
        assert_eq!(run(&mut flick, [0.6, 0.6], 30, 0.01), 0.0);
    }
}
