//! Parity of the vibrations profile with Shake&Tune's `VibrationsComputation`, on the fixtures
//! written by `tests/golden/vibrations_regen.py`.

#[macro_use]
mod common;

use common::{assert_close, close, unzstd};
use serde::Deserialize;
use wobblescope_core::RawSamples;
use wobblescope_core::vibrations::{StepInput, step_psds};

#[derive(Deserialize)]
struct Fixture {
    max_freq: f64,
    steps: Vec<FixtureStep>,
    target_freqs: Vec<f64>,
    psds: Vec<FixturePsd>,
    energies: Vec<FixtureEnergy>,
}

#[derive(Deserialize)]
struct FixtureStep {
    angle: f64,
    speed: f64,
    t: Vec<f64>,
    x: Vec<f64>,
    y: Vec<f64>,
    z: Vec<f64>,
}

#[derive(Deserialize)]
struct FixturePsd {
    angle: f64,
    speed: f64,
    psd: Vec<f64>,
}

#[derive(Deserialize)]
struct FixtureEnergy {
    angle: f64,
    speed: f64,
    energy: f64,
}

fn fixtures() -> Vec<(&'static str, Fixture)> {
    let cartesian = include_bytes!("data/vibrations_cartesian.json.zst");
    let corexy = include_bytes!("data/vibrations_corexy.json.zst");
    [("cartesian", &cartesian[..]), ("corexy", &corexy[..])]
        .into_iter()
        .map(|(name, bytes)| (name, serde_json::from_str(&unzstd(bytes)).unwrap()))
        .collect()
}

fn inputs(fixture: &Fixture) -> Vec<StepInput> {
    fixture
        .steps
        .iter()
        .map(|s| StepInput {
            angle: s.angle,
            speed: s.speed,
            samples: RawSamples {
                t: s.t.clone(),
                x: s.x.clone(),
                y: s.y.clone(),
                z: s.z.clone(),
            },
        })
        .collect()
}

parity_test! {
    fn step_energy_matches_shaketune() {
        for (name, fixture) in fixtures() {
            let (freqs, steps) = step_psds(&inputs(&fixture), fixture.max_freq).unwrap();
            assert_close(&format!("{name} target freqs"), &freqs, &fixture.target_freqs);
            assert_eq!(steps.len(), fixture.psds.len(), "{name}: one entry per step");
            for step in &steps {
                let key = format!("{name} angle {} speed {}", step.angle, step.speed);
                let psd = fixture
                    .psds
                    .iter()
                    .find(|p| p.angle == step.angle && p.speed == step.speed)
                    .unwrap_or_else(|| panic!("{key}: no PSD in the fixture"));
                let energy = fixture
                    .energies
                    .iter()
                    .find(|e| e.angle == step.angle && e.speed == step.speed)
                    .unwrap_or_else(|| panic!("{key}: no energy in the fixture"));
                assert_close(&format!("{key} psd"), &step.psd, &psd.psd);
                close(&format!("{key} energy"), step.energy, energy.energy);
            }
        }
    }
}

parity_test! {
    fn duplicate_speed_keeps_the_later_step() {
        let (_, fixture) = fixtures().remove(0);
        let mut steps = inputs(&fixture);
        let first = steps[0].clone();
        let mut later = first.clone();
        later.samples.x.iter_mut().for_each(|v| *v *= 2.0);
        let mut doubled = vec![later.clone()];
        doubled.extend(steps.iter().skip(1).cloned());
        let (_, want) = step_psds(&doubled, fixture.max_freq).unwrap();

        steps.push(later);
        let (_, got) = step_psds(&steps, fixture.max_freq).unwrap();

        assert_eq!(got.len(), fixture.steps.len(), "the duplicate adds no entry");
        assert_eq!(got[0].angle, first.angle);
        assert_eq!(got[0].speed, first.speed);
        assert_close("replaced psd", &got[0].psd, &want[0].psd);
        close("replaced energy", got[0].energy, want[0].energy);
    }
}
