//! The direction a resonance test shakes the toolhead along.

use serde::{Deserialize, Serialize};

use crate::fit::Axis;

/// A test direction: a bed axis, or an arbitrary direction vector such as a CoreXY belt.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub enum Direction {
    X,
    Y,
    /// Direction vector passed to `TEST_RESONANCES AXIS=`.
    Vector([i32; 3]),
    Angle(u32),
}

impl Direction {
    /// Belt A of a CoreXY printer.
    pub const BELT_A: Direction = Direction::Vector([1, -1, 0]);
    /// Belt B of a CoreXY printer.
    pub const BELT_B: Direction = Direction::Vector([1, 1, 0]);

    /// The bed axis this direction is, or `None` for a vector.
    pub fn axis(self) -> Option<Axis> {
        match self {
            Direction::X => Some(Axis::X),
            Direction::Y => Some(Axis::Y),
            Direction::Vector(_) | Direction::Angle(_) => None,
        }
    }

    /// The direction for a bed angle in degrees: `X` for 0, `Y` for 90, belt B for 45, belt A
    /// for 135 and `Angle` in thousandths of a degree otherwise.
    /// An angle outside 0 to 180 degrees saturates in the thousandths and NaN becomes `Angle(0)`.
    pub fn from_degrees(angle: f64) -> Direction {
        match angle {
            0.0 => Direction::X,
            90.0 => Direction::Y,
            45.0 => Direction::BELT_B,
            135.0 => Direction::BELT_A,
            a => Direction::Angle((a * 1000.0).round() as u32),
        }
    }

    /// The inverse of `from_degrees`; `None` for a vector that is not a belt.
    pub fn degrees(self) -> Option<f64> {
        match self {
            Direction::X => Some(0.0),
            Direction::Y => Some(90.0),
            Direction::Angle(n) => Some(f64::from(n) / 1000.0),
            d if d == Direction::BELT_B => Some(45.0),
            d if d == Direction::BELT_A => Some(135.0),
            Direction::Vector(_) => None,
        }
    }

    /// The value for `TEST_RESONANCES AXIS=`; a zero z component is omitted.
    pub fn klipper_arg(self) -> String {
        match self {
            Direction::X => "X".into(),
            Direction::Y => "Y".into(),
            Direction::Vector([x, y, 0]) => format!("{x},{y}"),
            Direction::Vector([x, y, z]) => format!("{x},{y},{z}"),
            Direction::Angle(_) => {
                panic!(
                    "an Angle direction is never sent to TEST_RESONANCES; vibrations steps do not use klipper_arg"
                )
            }
        }
    }
}

impl From<Axis> for Direction {
    fn from(axis: Axis) -> Self {
        match axis {
            Axis::X => Direction::X,
            Axis::Y => Direction::Y,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn x_and_y_serialise_as_before() {
        assert_eq!(serde_json::to_string(&Direction::X).unwrap(), "\"X\"");
        assert_eq!(serde_json::to_string(&Direction::Y).unwrap(), "\"Y\"");
    }

    #[test]
    fn a_vector_serialises_as_a_tagged_array() {
        let json = serde_json::to_string(&Direction::BELT_A).unwrap();
        assert_eq!(json, r#"{"Vector":[1,-1,0]}"#);
        assert_eq!(
            serde_json::from_str::<Direction>(&json).unwrap(),
            Direction::BELT_A
        );
    }

    #[test]
    fn klipper_arg_drops_a_zero_z() {
        assert_eq!(Direction::X.klipper_arg(), "X");
        assert_eq!(Direction::BELT_A.klipper_arg(), "1,-1");
        assert_eq!(Direction::Vector([1, 0, 1]).klipper_arg(), "1,0,1");
    }

    #[test]
    fn axis_is_none_for_a_vector() {
        assert_eq!(Direction::X.axis(), Some(Axis::X));
        assert_eq!(Direction::BELT_B.axis(), None);
    }

    #[test]
    fn from_degrees_uses_the_axis_and_belt_labels() {
        assert_eq!(Direction::from_degrees(0.0), Direction::X);
        assert_eq!(Direction::from_degrees(90.0), Direction::Y);
        assert_eq!(Direction::from_degrees(45.0), Direction::BELT_B);
        assert_eq!(Direction::from_degrees(135.0), Direction::BELT_A);
    }

    #[test]
    fn from_degrees_22_5_is_angle_22500() {
        assert_eq!(Direction::from_degrees(22.5), Direction::Angle(22500));
    }

    #[test]
    fn degrees_round_trips_every_grid_angle() {
        for step in [45.0, 22.5, 15.0] {
            let mut angle = 0.0;
            while angle < 180.0 {
                let direction = Direction::from_degrees(angle);
                assert_eq!(direction.degrees(), Some(angle), "{angle} at step {step}");
                angle += step;
            }
        }
    }

    #[test]
    fn an_other_vector_has_no_degrees() {
        assert_eq!(Direction::Vector([1, 2, 0]).degrees(), None);
    }

    #[test]
    fn an_angle_serialises_as_a_tagged_number() {
        let json = serde_json::to_string(&Direction::Angle(22500)).unwrap();
        assert_eq!(json, r#"{"Angle":22500}"#);
        assert_eq!(
            serde_json::from_str::<Direction>(&json).unwrap(),
            Direction::Angle(22500)
        );
    }
}
