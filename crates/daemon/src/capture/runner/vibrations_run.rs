//! The motion of a vibrations profile: the limits are changed, the toolhead is put in the middle
//! of the travel, and each angle runs its speed steps one after the other, recording once for the
//! whole angle.

use std::time::{Duration, Instant};

use wobblescope_core::eligibility::Coverage;
use wobblescope_session::meta::{StepMeta, TimeWindow};

use super::{Current, Failure, Run, SWEEP_TIMEOUT_MARGIN_SECONDS, SegmentOutcome};
use crate::capture::live::Phase;
use crate::capture::position::{window_end, window_start_plain};
use crate::capture::vibrations::{feed, step_script};
use crate::hold::settings::{Saved, num};
use crate::moonraker::rpc::Rpc;

/// The square corner velocity of the run, in mm/s.
const CORNER_VELOCITY: &str = "5.0";

/// How far the toolhead stays from the middle of the travel before the first step, in mm.
const MIDDLE_OFFSET_MM: f64 = 15.0;

/// A positioning script: the G-code state is saved around the moves, which are absolute.
fn motion_script(moves: &str) -> String {
    format!(
        "SAVE_GCODE_STATE NAME=wobblescope\nG90\n{moves}\nM400\nRESTORE_GCODE_STATE NAME=wobblescope"
    )
}

/// The timeout of one script of a vibrations run. The plan ignores `max_segment_seconds`, so a
/// step that is slower than it must not time out for that reason.
pub(super) fn step_timeout(max_segment_seconds: u32, slowest_step_seconds: f64) -> Duration {
    let longest = f64::from(max_segment_seconds).max(slowest_step_seconds);
    Duration::from_secs_f64(longest + SWEEP_TIMEOUT_MARGIN_SECONDS as f64)
}

impl Run {
    /// Reads what the run changes and refuses it when the printer has no input shaper, which the
    /// run leaves on. The limits it returns are what the restore puts back; the shaper is not
    /// part of it, because the run never changes it.
    pub(super) async fn read_vibrations_saved(&mut self, control: &Rpc) -> Result<Saved, Failure> {
        let saved =
            crate::capture::calibration::read_saved(control, self.r.timings.positioning.query)
                .await
                .map_err(Failure::certain)?;
        Ok(Saved {
            shaper: None,
            ..saved
        })
    }

    /// Runs both angles. `saved` holds the limits to put back.
    pub(super) async fn run_vibrations(&mut self, saved: Saved) -> Result<(), Failure> {
        let Some(plan) = self.plan.vibrations.clone() else {
            return Err(Failure::certain("the plan is not a vibrations profile"));
        };
        self.check_stop()?;
        self.check_chips_alive()?;
        let mut setup = format!("SET_VELOCITY_LIMIT ACCEL={}", num(plan.run.accel));
        if saved.minimum_cruise_ratio.is_some() {
            setup.push_str(" MINIMUM_CRUISE_RATIO=0");
        }
        setup.push_str(&format!(" SQUARE_CORNER_VELOCITY={CORNER_VELOCITY}"));
        self.change_settings(&saved, &setup).await?;

        self.current = Some(Current {
            index: 0,
            start: None,
            began: Instant::now(),
        });
        let first = self.plan.segments[0];
        self.publish(Phase::Positioning);
        let travel = plan.run.travel_speed;
        let moves = [
            format!("G1 Z{} F{}", num(plan.run.z_height), feed(travel / 10.0)),
            format!(
                "G1 X{} Y{} F{}",
                num(plan.mid[0] - MIDDLE_OFFSET_MM),
                num(plan.mid[1] - MIDDLE_OFFSET_MM),
                feed(travel)
            ),
        ];
        for moves in moves {
            self.check_stop()?;
            self.run_script(motion_script(&moves), &first, Phase::Positioning)
                .await?;
        }
        self.check_stop()?;
        self.run_script("G4 P500\nM400".to_owned(), &first, Phase::Positioning)
            .await?;

        for (index, &angle) in plan.angles.iter().enumerate() {
            let segment = self.plan.segments[index];
            self.angle_steps.clear();
            self.current = Some(Current {
                index,
                start: None,
                began: Instant::now(),
            });
            if index > 0 {
                self.recheck().await?;
            }
            self.check_stop()?;
            self.check_chips_alive()?;
            for rec in &self.recorders {
                rec.begin_segment();
            }
            let query = self.r.timings.positioning.query;
            for &speed in &plan.speeds {
                self.check_stop()?;
                self.check_chips_alive()?;
                let start = window_start_plain(&self.control()?, query).await?;
                if let Some(current) = &mut self.current {
                    current.start.get_or_insert(start);
                }
                self.publish(Phase::Measuring);
                self.run_script(step_script(&plan, angle, speed), &segment, Phase::Measuring)
                    .await?;
                let end = window_end(&self.control()?, query)
                    .await
                    .map_err(|e| Failure::certain(format!("cannot read the window end: {e}")))?;
                self.angle_steps.push(StepMeta {
                    speed,
                    window: TimeWindow { start, end },
                });
            }
            let (Some(first), Some(last)) = (self.angle_steps.first(), self.angle_steps.last())
            else {
                return Err(Failure::certain("the angle has no speed step"));
            };
            let (start, end) = (first.window.start, last.window.end);
            let finished = std::mem::take(&mut self.angle_steps);
            self.drain_and_store(&segment, start, end, finished).await?;
            self.current = None;
        }
        Ok(())
    }

    /// Writes the angle that was running when the run failed, cut to the steps that finished.
    /// Nothing is written when no step of it finished.
    pub(super) async fn add_interrupted_angle(&mut self) {
        let (Some(first), Some(last)) = (self.angle_steps.first(), self.angle_steps.last()) else {
            return;
        };
        let Some(index) = self.current.as_ref().map(|c| c.index) else {
            return;
        };
        let (start, end) = (first.window.start, last.window.end);
        let steps = std::mem::take(&mut self.angle_steps);
        // The data of the last step may still be on its way.
        let drain = self.r.timings.drain;
        let waits = self.recorders.iter().map(|r| r.wait_past(end, drain));
        let _ = futures_util::future::join_all(waits).await;
        let mut outcome = SegmentOutcome::default();
        for (chip, rec) in self.recorders.iter().enumerate() {
            // The unfinished step kept delivering; only the finished steps are written.
            let taken = rec.take_within(start, end);
            if taken.samples.is_empty() {
                continue;
            }
            let coverage = Coverage::Incomplete {
                missing: "the test was interrupted".to_owned(),
            };
            outcome.add(chip, taken, coverage);
        }
        if outcome.recordings.is_empty() {
            return;
        }
        let segment = self.plan.segments[index];
        let at = self.segments_stored();
        self.incomplete
            .push(format!("angle {} was interrupted", at + 1));
        // A failure to write it leaves the finished angle, which is already journaled.
        let _ = self
            .store_segment(&segment, at, outcome, TimeWindow { start, end }, steps)
            .await;
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::step_timeout;

    #[test]
    fn a_tiny_segment_limit_gives_the_slowest_step_plus_a_minute() {
        assert_eq!(step_timeout(5, 140.2), Duration::from_secs_f64(200.2));
    }

    #[test]
    fn a_long_segment_limit_wins_over_a_short_step() {
        assert_eq!(step_timeout(600, 12.0), Duration::from_secs(660));
    }
}
