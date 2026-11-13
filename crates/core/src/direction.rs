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
}
