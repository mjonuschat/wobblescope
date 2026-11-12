//! Chip availability: a chip is available only when its subscription succeeds and valid samples
//! arrive within a timeout. The open connections are kept, so the subscription that records is
//! the one that was proven.

use std::time::Duration;

use time::OffsetDateTime;
use wobblescope_core::numeric::{median_rate, strictly_increasing};

use crate::moonraker::Moonraker;
use crate::moonraker::bridge::{Batch, Bridge, BridgeError, Event};
use crate::moonraker::chips::{Availability, Chip};
use crate::moonraker::tls::Tls;

/// How long one chip may take to prove itself.
pub const AVAILABILITY_TIMEOUT: Duration = Duration::from_secs(3);

/// How long the call that fetches a one-shot token may take.
const TOKEN_TIMEOUT: Duration = Duration::from_secs(5);

/// Fewest samples that give a rate. Real batches hold dozens, so one batch is normally enough.
const MIN_SAMPLES: usize = 2;

/// A chip whose subscription is open and delivering.
pub struct OpenChip {
    /// The chip.
    pub chip: Chip,
    /// The live subscription.
    pub bridge: Bridge,
    /// The sample rate seen while proving the chip, in Hz.
    pub observed_rate_hz: f64,
    /// The batches received while proving the chip.
    pub first_batches: Vec<Batch>,
}

/// What the check found for one chip.
pub type ChipOutcome = Result<OpenChip, String>;

/// Why a check could not start at all.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CheckError {
    /// There is no control connection to Moonraker.
    #[error("Moonraker is not connected")]
    NotConnected,
}

/// Opens every chip in parallel and waits up to `timeout` for valid samples from each.
///
/// Returns one outcome per chip, in the order of `chips`.
///
/// # Errors
/// [`CheckError::NotConnected`] when there is no control connection.
pub async fn open_chips(
    moonraker: &Moonraker,
    chips: &[Chip],
    timeout: Duration,
) -> Result<Vec<(Chip, ChipOutcome)>, CheckError> {
    let control = moonraker.control().ok_or(CheckError::NotConnected)?;
    let checks = chips.iter().map(|chip| {
        let control = control.clone();
        async move {
            let proof = tokio::time::timeout(timeout, async {
                let token = control
                    .call("access.oneshot_token", None, TOKEN_TIMEOUT.min(timeout))
                    .await
                    .map_err(|e| format!("could not get a bridge token from Moonraker: {e}"))?;
                let token = token
                    .as_str()
                    .ok_or_else(|| "Moonraker's bridge token is not a string".to_owned())?;
                prove(moonraker.bridge_url(token), moonraker.tls(), chip, timeout).await
            })
            .await;
            let outcome = proof.unwrap_or_else(|_| {
                Err(format!(
                    "no valid samples arrived within {} s",
                    timeout.as_secs_f64()
                ))
            });
            (chip.clone(), outcome)
        }
    });
    Ok(futures_util::future::join_all(checks).await)
}

/// Subscribes to `chip` and reads until its samples are valid and give a rate.
async fn prove(url: String, tls: &Tls, chip: &Chip, timeout: Duration) -> ChipOutcome {
    let mut bridge = Bridge::open(
        &url,
        &chip.dump_endpoint,
        chip.sensor.as_deref(),
        timeout,
        tls,
    )
    .await
    .map_err(|e| match e {
        BridgeError::Subscribe(message) => message,
        other => other.to_string(),
    })?;
    let mut batches: Vec<Batch> = Vec::new();
    let mut times: Vec<f64> = Vec::new();
    loop {
        match bridge.recv().await {
            Event::Closed(reason) => return Err(reason),
            Event::Batch(batch) => {
                times.extend_from_slice(&batch.samples.t);
                batches.push(batch);
                if times.len() >= MIN_SAMPLES {
                    break;
                }
            }
        }
    }
    let rate = rate_from_times(&times).ok_or_else(|| {
        "the chip's samples are not valid: timestamps must be finite and increasing".to_owned()
    })?;
    Ok(OpenChip {
        chip: chip.clone(),
        bridge,
        observed_rate_hz: rate,
        first_batches: batches,
    })
}

/// The sample rate from timestamps that must be finite and strictly increasing, or `None`.
#[must_use]
pub fn rate_from_times(times: &[f64]) -> Option<f64> {
    if !strictly_increasing(times) {
        return None;
    }
    median_rate(times)
}

/// The availability a finished check leaves on a chip.
#[must_use]
pub fn availability_of(outcome: &ChipOutcome, now: OffsetDateTime) -> Availability {
    match outcome {
        Ok(open) => Availability::Available {
            observed_rate_hz: open.observed_rate_hz,
            checked_at: super::iso_utc(now),
        },
        Err(reason) => Availability::Unavailable {
            reason: reason.clone(),
        },
    }
}
