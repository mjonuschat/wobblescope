#[macro_use]
mod common;

use common::{fixtures, inputs};
use wobblescope_core::vibrations::{
    EnergyByAngleSpeed, Kinematics, Model, direction_speed_grid, step_psds,
};

parity_test! {
    fn model_value_matches_the_grid_cells() {
        for (name, fixture) in fixtures() {
            let (_, steps) = step_psds(&inputs(&fixture), fixture.max_freq).unwrap();
            let mut energy: EnergyByAngleSpeed = Vec::new();
            for step in &steps {
                match energy.iter_mut().find(|(a, _)| *a == step.angle) {
                    Some((_, by_speed)) => by_speed.push((step.speed, step.energy)),
                    None => energy.push((step.angle, vec![(step.speed, step.energy)])),
                }
            }
            let k = Kinematics::parse(&fixture.kinematics).unwrap();
            let main = [fixture.main_angles[0], fixture.main_angles[1]];
            let grid = direction_speed_grid(&energy, k, main);
            let model = Model::new(&energy, k, main);
            assert_eq!(model.speed_axis(), grid.speeds, "{name}: speed axis");
            for (angle, row) in grid.angles.iter().zip(&grid.data) {
                for (speed, cell) in grid.speeds.iter().zip(row) {
                    assert_eq!(model.value(*angle, *speed), *cell, "{name}: {angle} {speed}");
                }
            }
        }
    }
}
