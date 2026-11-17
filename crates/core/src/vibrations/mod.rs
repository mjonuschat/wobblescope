//! The vibrations profile, computed the way Shake&Tune's `vibrations_computation.py` computes it
//! (at the commit pinned in `docs/specs/shaketune-deviations.md`).

mod grid;
mod motor;
mod steps;
mod worst;
mod zones;

mod analysis;

pub use crate::resonance::{MechanicalParams, estimate_damping_above};
pub use analysis::{MotorResult, StepEnergy, VibrationsParams, VibrationsResult, analyze_chip};
pub use grid::{
    EnergyByAngleSpeed, Grid, Kinematics, SpeedCurves, angle_powers, direction_speed_grid,
    speed_powers, symmetry,
};
pub use motor::{PsdByAngleSpeed, motor_profiles};
pub use steps::{StepInput, StepPsd, step_psds};
pub use worst::worst_case;
pub use zones::{Zone, filter_and_split_zones, low_energy_zones};
