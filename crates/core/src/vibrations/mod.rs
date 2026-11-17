//! The vibrations profile, computed the way Shake&Tune's `vibrations_computation.py` computes it
//! (at the commit pinned in `docs/specs/shaketune-deviations.md`).

mod steps;

pub use steps::{StepInput, StepPsd, step_psds};
