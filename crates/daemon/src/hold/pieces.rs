//! The G-code pieces of a held excitation, the frequency ramp and the limits checks.
//!
//! A piece is whole pairs of relative moves, `+d` then `-d`, with `d = accel_per_hz / (16 *
//! frequency)` mm, so a triangular move at `accel_per_hz * frequency` takes half a period. Every
//! function here is pure.

use serde::{Deserialize, Serialize};

/// The lowest frequency a hold may use, in Hz.
pub const MIN_HZ: f64 = 5.0;
/// The highest frequency a hold may use, in Hz.
pub const MAX_HZ: f64 = 200.0;
/// The longest piece, in seconds of motion.
pub const MAX_PIECE_SECONDS: f64 = 0.5;
/// The piece length used by default, in seconds of motion.
pub const DEFAULT_PIECE_SECONDS: f64 = 0.25;
/// The fastest a frequency change may ramp, in Hz per second.
pub const MAX_RAMP_HZ_PER_SECOND: f64 = 20.0;
/// How far the start-buffer calibration moves X, in mm.
pub const CALIBRATION_MM: f64 = 0.01;

/// The axis the toolhead moves along: X, Y, or the diagonals A (X+Y) and B (X-Y).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "UPPERCASE")]
pub enum HoldAxis {
    /// Along X.
    X,
    /// Along Y.
    Y,
    /// Along X and Y together.
    A,
    /// Along X and against Y.
    B,
}

/// What a hold drives: the axis, the frequency and the acceleration per Hz.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Drive {
    /// The axis.
    pub axis: HoldAxis,
    /// The frequency, in Hz.
    pub frequency: f64,
    /// Peak acceleration per Hz, in mm/s² per Hz.
    pub accel_per_hz: f64,
}

impl Drive {
    /// Refuses a frequency outside [`MIN_HZ`] to [`MAX_HZ`] or an acceleration per Hz that is not
    /// positive and finite.
    ///
    /// # Errors
    /// A message naming the field.
    pub fn validate(&self) -> Result<(), String> {
        if !self.frequency.is_finite() || !(MIN_HZ..=MAX_HZ).contains(&self.frequency) {
            return Err(format!(
                "the frequency must be between {MIN_HZ} and {MAX_HZ} Hz, not {}",
                self.frequency
            ));
        }
        if !self.accel_per_hz.is_finite() || self.accel_per_hz <= 0.0 {
            return Err(format!(
                "the acceleration per Hz must be positive, not {}",
                self.accel_per_hz
            ));
        }
        Ok(())
    }

    /// The peak acceleration, in mm/s².
    #[must_use]
    pub fn acceleration(&self) -> f64 {
        self.accel_per_hz * self.frequency
    }

    /// The distance of one move, in mm.
    #[must_use]
    pub fn half_period_mm(&self) -> f64 {
        self.accel_per_hz / (16.0 * self.frequency)
    }

    /// The peak velocity of a move, in mm/s.
    #[must_use]
    pub fn peak_velocity(&self) -> f64 {
        self.accel_per_hz / 4.0
    }

    /// The distance each moved axis travels, as `(x, y)` in mm.
    fn travel(&self) -> (f64, f64) {
        let d = self.half_period_mm();
        let diagonal = d / std::f64::consts::SQRT_2;
        match self.axis {
            HoldAxis::X => (d, 0.0),
            HoldAxis::Y => (0.0, d),
            HoldAxis::A | HoldAxis::B => (diagonal, diagonal),
        }
    }

    /// Whether the moves fit between `min` and `max` around `position`. The start-buffer
    /// calibration moves X by [`CALIBRATION_MM`] whatever the axis, so X needs that much room too.
    ///
    /// # Errors
    /// A message naming the axis and its limits.
    pub fn check_room(
        &self,
        position: [f64; 3],
        min: [f64; 3],
        max: [f64; 3],
    ) -> Result<(), String> {
        let (dx, dy) = self.travel();
        for (i, (name, need)) in [("X", dx.max(CALIBRATION_MM)), ("Y", dy)]
            .into_iter()
            .enumerate()
        {
            if position[i] - need < min[i] || position[i] + need > max[i] {
                return Err(format!(
                    "axis {name} at {:.3} has no room for {need:.4} mm either way (limits {} to {})",
                    position[i], min[i], max[i]
                ));
            }
        }
        Ok(())
    }

    /// Whether the peak velocity is within the printer's `max_velocity`.
    ///
    /// # Errors
    /// A message with both numbers.
    pub fn check_velocity(&self, max_velocity: f64) -> Result<(), String> {
        if self.peak_velocity() > max_velocity {
            return Err(format!(
                "the peak velocity {:.1} mm/s is over the printer's max_velocity {max_velocity}",
                self.peak_velocity()
            ));
        }
        Ok(())
    }
}

/// The pairs of moves that make a piece of about `piece_seconds` at `frequency`, at least one.
#[must_use]
pub fn pairs_for(piece_seconds: f64, frequency: f64) -> u32 {
    (piece_seconds * frequency).round().max(1.0) as u32
}

/// The motion time of `pairs` pairs at `frequency`, in seconds.
#[must_use]
pub fn piece_duration(pairs: u32, frequency: f64) -> f64 {
    f64::from(pairs) / frequency
}

/// `current` moved toward `target` by at most [`MAX_RAMP_HZ_PER_SECOND`] over `dt` seconds.
#[must_use]
pub fn ramp(current: f64, target: f64, dt: f64) -> f64 {
    let step = MAX_RAMP_HZ_PER_SECOND * dt;
    current + (target - current).clamp(-step, step)
}

/// The script for one piece: relative positioning, then `pairs` pairs of balanced moves. The feed
/// is four times the peak velocity, so it never limits a move.
#[must_use]
pub fn piece_script(drive: &Drive, pairs: u32) -> String {
    let (dx, dy) = drive.travel();
    let feed = (drive.accel_per_hz * 60.0).round() as i64;
    let sy = if drive.axis == HoldAxis::B { -1.0 } else { 1.0 };
    let line = |sign: f64| match drive.axis {
        HoldAxis::X => format!("G1 X{:.4} F{feed}", sign * dx),
        HoldAxis::Y => format!("G1 Y{:.4} F{feed}", sign * dy),
        HoldAxis::A | HoldAxis::B => {
            format!("G1 X{:.4} Y{:.4} F{feed}", sign * dx, sign * sy * dy)
        }
    };
    let mut out = String::from("G91");
    for _ in 0..pairs {
        out.push('\n');
        out.push_str(&line(1.0));
        out.push('\n');
        out.push_str(&line(-1.0));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn drive(axis: HoldAxis, frequency: f64, accel_per_hz: f64) -> Drive {
        Drive {
            axis,
            frequency,
            accel_per_hz,
        }
    }

    #[test]
    fn half_period_is_accel_per_hz_over_16_f() {
        let d = drive(HoldAxis::X, 60.0, 60.0);
        assert!((d.half_period_mm() - 0.0625).abs() < 1e-12);
        assert_eq!(d.acceleration(), 3600.0);
        assert_eq!(d.peak_velocity(), 15.0);
    }

    #[test]
    fn a_piece_is_whole_pairs_and_balanced() {
        let script = piece_script(&drive(HoldAxis::X, 60.0, 60.0), 15);
        let lines: Vec<&str> = script.lines().collect();
        assert_eq!(lines[0], "G91");
        assert_eq!(lines.len(), 31);
        for (i, line) in lines[1..].iter().enumerate() {
            let want = if i % 2 == 0 {
                "G1 X0.0625 F3600"
            } else {
                "G1 X-0.0625 F3600"
            };
            assert_eq!(*line, want);
        }
    }

    #[test]
    fn diagonals_split_the_distance() {
        let a = piece_script(&drive(HoldAxis::A, 60.0, 60.0), 1);
        assert_eq!(
            a.lines().collect::<Vec<_>>(),
            [
                "G91",
                "G1 X0.0442 Y0.0442 F3600",
                "G1 X-0.0442 Y-0.0442 F3600"
            ]
        );
        let b = piece_script(&drive(HoldAxis::B, 60.0, 60.0), 1);
        assert_eq!(
            b.lines().collect::<Vec<_>>(),
            [
                "G91",
                "G1 X0.0442 Y-0.0442 F3600",
                "G1 X-0.0442 Y0.0442 F3600"
            ]
        );
        let y = piece_script(&drive(HoldAxis::Y, 60.0, 60.0), 1);
        assert_eq!(
            y.lines().collect::<Vec<_>>(),
            ["G91", "G1 Y0.0625 F3600", "G1 Y-0.0625 F3600"]
        );
    }

    #[test]
    fn pairs_follow_the_piece_length() {
        assert_eq!(pairs_for(0.25, 60.0), 15);
        assert_eq!(pairs_for(0.5, 150.0), 75);
        assert_eq!(pairs_for(0.25, 5.0), 1);
        assert!((piece_duration(15, 60.0) - 0.25).abs() < 1e-12);
    }

    #[test]
    fn ramp_moves_at_most_the_limit() {
        assert_eq!(ramp(25.0, 60.0, 0.5), 35.0);
        assert_eq!(ramp(60.0, 25.0, 0.5), 50.0);
        assert_eq!(ramp(25.0, 26.0, 0.5), 26.0);
    }

    #[test]
    fn validate_refuses_nonsense() {
        let bad = [
            (4.9, 60.0, "frequency"),
            (200.1, 60.0, "frequency"),
            (f64::NAN, 60.0, "frequency"),
            (60.0, 0.0, "acceleration per Hz"),
            (60.0, -1.0, "acceleration per Hz"),
            (60.0, f64::NAN, "acceleration per Hz"),
        ];
        for (f, a, field) in bad {
            let e = drive(HoldAxis::X, f, a).validate().unwrap_err();
            assert!(e.contains(field), "{e} should name {field}");
        }
        assert!(drive(HoldAxis::X, 5.0, 60.0).validate().is_ok());
        assert!(drive(HoldAxis::X, 200.0, 60.0).validate().is_ok());
    }

    #[test]
    fn room_includes_the_calibration_move() {
        let min = [0.0, 0.0, 0.0];
        let max = [250.0, 250.0, 250.0];
        let y = drive(HoldAxis::Y, 60.0, 60.0);
        // The calibration moves X by 0.01 mm whatever the axis.
        assert!(y.check_room([249.995, 100.0, 20.0], min, max).is_err());
        assert!(y.check_room([0.005, 100.0, 20.0], min, max).is_err());
        assert!(y.check_room([245.0, 100.0, 20.0], min, max).is_ok());
        let x = drive(HoldAxis::X, 60.0, 60.0);
        assert!(x.check_room([249.99, 100.0, 20.0], min, max).is_err());
        assert!(x.check_room([245.0, 100.0, 20.0], min, max).is_ok());
        let a = drive(HoldAxis::A, 60.0, 60.0);
        assert!(a.check_room([100.0, 249.99, 20.0], min, max).is_err());
        assert!(a.check_room([249.99, 100.0, 20.0], min, max).is_err());
        assert!(a.check_room([100.0, 100.0, 20.0], min, max).is_ok());
    }

    #[test]
    fn velocity_must_fit() {
        assert!(drive(HoldAxis::X, 60.0, 60.0).check_velocity(10.0).is_err());
        assert!(drive(HoldAxis::X, 60.0, 20.0).check_velocity(10.0).is_ok());
    }
}
