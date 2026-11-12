//! The DualSense's motion sensors, read from the input reports the pad sends anyway: its gyro
//! (degrees per second) and accelerometer (g). Reading costs the pad and the bus nothing more;
//! the reports arrive whether anyone reads them or not.
//!
//! The layout and the calibration are the Linux kernel's `hid-playstation.c` and SDL's
//! `SDL_hidapi_ps5.c`. Axes are the pad's own (SDL's): x to the right, y up, z towards the
//! player; a positive rotation turns counterclockwise looking down that axis, so turning the pad
//! to the left is a positive y.

/// Offsets in the input report's state block, after the report ID (USB 0x01) or the ID and a
/// tag byte (Bluetooth 0x31).
const GYRO: usize = 15;
const ACCEL: usize = 21;
const CLOCK: usize = 27;
/// The feature report holding the factory calibration, and its length with the ID.
pub const CALIBRATION_REPORT: u8 = 0x05;
pub const CALIBRATION_LEN: usize = 41;
/// The sensor clock counts in thirds of a microsecond.
const CLOCK_HZ: f64 = 3_000_000.0;
/// A gap between two reports longer than this (a dropped Bluetooth packet, a pad waking up)
/// counts as this much: a stale rate mustn't spin the view.
const MAX_STEP: f64 = 0.02;
/// The step a report counts for when the pad's clock can't say (the first report).
const FIRST_STEP: f64 = 0.004;

/// One report's sensors, raw.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RawMotion {
    pub gyro: [i16; 3],
    pub accel: [i16; 3],
    pub clock: u32,
}

/// The state block of an input report: USB 0x01 after its ID, Bluetooth 0x31 after its ID and
/// tag. The simple Bluetooth report 0x01 (a pad no host has switched over) is shorter and has no
/// sensors.
pub fn state_block(report: &[u8]) -> Option<&[u8]> {
    let block = match report {
        [0x01, block @ ..] if block.len() >= 63 => block,
        [0x31, _, block @ ..] => block,
        _ => return None,
    };
    (block.len() >= CLOCK + 4).then_some(block)
}

/// The sensors in an input report.
pub fn parse(report: &[u8]) -> Option<RawMotion> {
    let block = state_block(report)?;
    let axis = |at: usize, i: usize| i16::from_le_bytes([block[at + 2 * i], block[at + 2 * i + 1]]);
    Some(RawMotion {
        gyro: [0, 1, 2].map(|i| axis(GYRO, i)),
        accel: [0, 1, 2].map(|i| axis(ACCEL, i)),
        clock: u32::from_le_bytes(block[CLOCK..CLOCK + 4].try_into().ok()?),
    })
}

/// From raw counts to degrees per second and g, each axis its own scale and (for the
/// accelerometer) offset. The gyro's factory offset is left to the game's own calibration, which
/// measures the pad as it is now (the kernel ignores the DualSense's too).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Calibration {
    pub gyro_scale: [f32; 3],
    pub accel_scale: [f32; 3],
    pub accel_bias: [f32; 3],
}

impl Default for Calibration {
    /// The sensors' nominal ranges: ±2000 °/s and ±4 g over the 16 bits.
    fn default() -> Self {
        Calibration {
            gyro_scale: [2000.0 / 32768.0; 3],
            accel_scale: [1.0 / 8192.0; 3],
            accel_bias: [0.0; 3],
        }
    }
}

impl Calibration {
    /// Feature report 0x05, with its ID. An axis whose numbers make no sense keeps its nominal
    /// scale.
    pub fn parse(report: &[u8]) -> Calibration {
        let mut c = Calibration::default();
        if report.len() < CALIBRATION_LEN || report[0] != CALIBRATION_REPORT {
            return c;
        }
        let at = |i: usize| f32::from(i16::from_le_bytes([report[i], report[i + 1]]));
        // Pitch, yaw, roll: each axis' reading at +speed and -speed.
        let speed_2x = at(19) + at(21);
        for (axis, plus) in [7, 11, 15].into_iter().enumerate() {
            let range = at(plus) - at(plus + 2);
            let scale = speed_2x / range;
            if range > 0.0 && scale.is_finite() && (0.01..0.5).contains(&scale) {
                c.gyro_scale[axis] = scale;
            }
        }
        // x, y, z: each axis' reading at +1 g and -1 g.
        for (axis, plus) in [23, 27, 31].into_iter().enumerate() {
            let range_2g = at(plus) - at(plus + 2);
            if range_2g > 1000.0 {
                c.accel_scale[axis] = 2.0 / range_2g;
                c.accel_bias[axis] = at(plus) - range_2g / 2.0;
            }
        }
        c
    }

    pub fn gyro(&self, raw: [i16; 3]) -> [f32; 3] {
        [0, 1, 2].map(|i| f32::from(raw[i]) * self.gyro_scale[i])
    }

    pub fn accel(&self, raw: [i16; 3]) -> [f32; 3] {
        [0, 1, 2].map(|i| (f32::from(raw[i]) - self.accel_bias[i]) * self.accel_scale[i])
    }
}

/// The sensors over a stretch of time: the gyro's mean rate and the accelerometer's mean, and
/// how long it lasted. The rate times the time is exactly how far the pad turned, however many
/// reports came in.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Motion {
    /// Degrees per second, the pad's axes.
    pub gyro: [f32; 3],
    /// g, the pad's axes; a pad lying flat reads about (0, 1, 0).
    pub accel: [f32; 3],
    pub seconds: f32,
}

/// Reports summed until the game takes them.
#[derive(Clone, Debug, Default)]
pub struct MotionSum {
    turned: [f64; 3],
    accel: [f64; 3],
    seconds: f64,
    clock: Option<u32>,
}

impl MotionSum {
    pub fn push(&mut self, raw: RawMotion, calibration: &Calibration) {
        let step = match self.clock {
            Some(last) => f64::from(raw.clock.wrapping_sub(last)) / CLOCK_HZ,
            None => FIRST_STEP,
        };
        self.clock = Some(raw.clock);
        // A report repeated with the same clock adds nothing.
        if step <= 0.0 {
            return;
        }
        let step = step.min(MAX_STEP);
        let gyro = calibration.gyro(raw.gyro);
        let accel = calibration.accel(raw.accel);
        for i in 0..3 {
            self.turned[i] += f64::from(gyro[i]) * step;
            self.accel[i] += f64::from(accel[i]) * step;
        }
        self.seconds += step;
    }

    /// What came in since the last take; `None` when nothing did.
    pub fn take(&mut self) -> Option<Motion> {
        if self.seconds <= 0.0 {
            return None;
        }
        let t = self.seconds;
        let motion = Motion {
            gyro: self.turned.map(|v| (v / t) as f32),
            accel: self.accel.map(|v| (v / t) as f32),
            seconds: t as f32,
        };
        self.turned = [0.0; 3];
        self.accel = [0.0; 3];
        self.seconds = 0.0;
        Some(motion)
    }

    /// The pad went away: the next one starts its own clock.
    pub fn restart(&mut self) {
        *self = MotionSum::default();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn usb_report(gyro: [i16; 3], accel: [i16; 3], clock: u32) -> Vec<u8> {
        let mut r = vec![0u8; 64];
        r[0] = 0x01;
        for i in 0..3 {
            r[1 + GYRO + 2 * i..1 + GYRO + 2 * i + 2].copy_from_slice(&gyro[i].to_le_bytes());
            r[1 + ACCEL + 2 * i..1 + ACCEL + 2 * i + 2].copy_from_slice(&accel[i].to_le_bytes());
        }
        r[1 + CLOCK..1 + CLOCK + 4].copy_from_slice(&clock.to_le_bytes());
        r
    }

    #[test]
    fn both_transports_put_the_sensors_at_the_same_place_in_the_state_block() {
        let usb = usb_report([1, -2, 3], [4, 5, -6], 77);
        let mut bt = vec![0u8; 78];
        bt[0] = 0x31;
        bt[2..2 + 63].copy_from_slice(&usb[1..64]);
        let expected = RawMotion {
            gyro: [1, -2, 3],
            accel: [4, 5, -6],
            clock: 77,
        };
        assert_eq!(parse(&usb), Some(expected));
        assert_eq!(parse(&bt), Some(expected));
        // Bluetooth's simple report has no sensors.
        assert_eq!(parse(&[0x01, 0, 0, 0, 0, 0, 0, 0, 0, 0]), None);
    }

    #[test]
    fn the_sum_gives_how_far_the_pad_turned_by_its_own_clock() {
        let c = Calibration::default();
        let mut sum = MotionSum::default();
        let raw = |clock| RawMotion {
            gyro: [0, 16384, 0], // 1000 °/s at the nominal scale
            accel: [0, 8192, 0], // 1 g
            clock,
        };
        sum.push(raw(0), &c);
        sum.take();
        // Four reports 4 ms apart: 16 ms at 1000 °/s is 16 degrees.
        for i in 1..=4 {
            sum.push(raw(i * 12_000), &c);
        }
        let m = sum.take().unwrap();
        assert!((m.seconds - 0.016).abs() < 1e-6, "{}", m.seconds);
        assert!((m.gyro[1] * m.seconds - 16.0).abs() < 1e-3);
        assert!((m.accel[1] - 1.0).abs() < 1e-4);
        assert_eq!(sum.take(), None);
        // The clock wrapping round counts as the step it is.
        let mut sum = MotionSum::default();
        sum.push(raw(u32::MAX - 5_999), &c);
        sum.take();
        sum.push(raw(6_000), &c);
        assert!((sum.take().unwrap().seconds - 0.004).abs() < 1e-6);
    }

    #[test]
    fn a_long_gap_counts_short() {
        let c = Calibration::default();
        let mut sum = MotionSum::default();
        let raw = |clock| RawMotion {
            gyro: [0, 1000, 0],
            accel: [0; 3],
            clock,
        };
        sum.push(raw(0), &c);
        sum.take();
        sum.push(raw(3_000_000), &c); // a second later
        assert!((sum.take().unwrap().seconds - MAX_STEP as f32).abs() < 1e-6);
    }

    #[test]
    fn the_factory_calibration_scales_each_axis() {
        let mut report = [0u8; CALIBRATION_LEN];
        report[0] = CALIBRATION_REPORT;
        let mut put = |at: usize, v: i16| report[at..at + 2].copy_from_slice(&v.to_le_bytes());
        // Gyro: ±8800 counts at ±540 °/s on every axis.
        for plus in [7, 11, 15] {
            put(plus, 8800);
            put(plus + 2, -8800);
        }
        put(19, 540);
        put(21, 540);
        // Accel: +1 g at 8300, -1 g at -8100 on every axis.
        for plus in [23, 27, 31] {
            put(plus, 8300);
            put(plus + 2, -8100);
        }
        let c = Calibration::parse(&report);
        let gyro = c.gyro([8800, -8800, 0]);
        assert!((gyro[0] - 540.0).abs() < 1e-3 && (gyro[1] + 540.0).abs() < 1e-3);
        let accel = c.accel([8300, -8100, 100]);
        assert!((accel[0] - 1.0).abs() < 1e-5 && (accel[1] + 1.0).abs() < 1e-5);
        assert!(accel[2].abs() < 1e-5);
        // Nonsense keeps the nominal scales.
        assert_eq!(Calibration::parse(&[0u8; 41]), Calibration::default());
    }
}
