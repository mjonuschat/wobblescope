//! How a test ends on disk. A capture is sealed, or abandoned when no segment finished.

use wobblescope_session::meta::{Outcome, SessionMeta};

use super::live::{FinishedOutcome, FinishedTest};
use super::meta::apply_summaries;
use super::recorder::ChipSummary;
use crate::storage::capture::Capture;
use crate::storage::{Storage, run_blocking};

/// Discards a capture that holds no segment. The reason is the failure's, or a plain statement
/// that nothing finished.
pub(super) async fn abandon(
    storage: &Storage,
    id: &str,
    capture: Option<Capture>,
    reason: Option<String>,
) -> FinishedTest {
    if let Some(capture) = capture {
        let storage = storage.clone();
        let _ = run_blocking(move || storage.abandon_capture(capture)).await;
    }
    FinishedTest {
        id: id.to_owned(),
        outcome: FinishedOutcome::NoSession,
        reason: Some(reason.unwrap_or_else(|| "no segment finished".to_owned())),
        session_id: None,
        code: None,
        motion_uncertain: false,
    }
}

pub(super) struct Sealing {
    pub id: String,
    pub meta: SessionMeta,
    pub summaries: Vec<ChipSummary>,
    pub reason: Option<String>,
    pub incomplete: Vec<String>,
}

/// Writes the outcome into the metadata and seals the capture. A failure to seal is reported in
/// the result, since the next daemon start recovers the finished segments.
pub(super) async fn seal(
    storage: &Storage,
    capture: Option<Capture>,
    sealing: Sealing,
) -> FinishedTest {
    let Sealing {
        id,
        mut meta,
        summaries,
        reason,
        incomplete,
    } = sealing;
    apply_summaries(&mut meta, &summaries);
    let (outcome, kind) = match &reason {
        Some(r) => {
            meta.notes.push(r.clone());
            (
                Outcome::Failed { reason: r.clone() },
                FinishedOutcome::Failed,
            )
        }
        None if !incomplete.is_empty() => (
            Outcome::Incomplete {
                reason: incomplete.join("; "),
            },
            FinishedOutcome::Incomplete,
        ),
        None => (Outcome::Complete, FinishedOutcome::Complete),
    };
    meta.notes.extend(incomplete.iter().cloned());
    meta.outcome = outcome;
    let finished = |kind, reason: Option<String>, session: Option<String>| FinishedTest {
        id: id.clone(),
        outcome: kind,
        reason,
        session_id: session,
        code: None,
        motion_uncertain: false,
    };
    let Some(capture) = capture else {
        return finished(
            FinishedOutcome::Failed,
            Some("could not seal: the capture is not open".to_owned()),
            None,
        );
    };
    let storage = storage.clone();
    let sealed = run_blocking(move || {
        let result = storage.seal_capture(&capture, &meta);
        // The capture drops here, releasing its reservation either way.
        result.map(|summary| summary.id)
    })
    .await;
    match sealed {
        Ok(Ok(session)) => finished(
            kind,
            reason.or_else(|| (kind == FinishedOutcome::Incomplete).then(|| incomplete.join("; "))),
            Some(session),
        ),
        Ok(Err(e)) => finished(
            FinishedOutcome::Failed,
            Some(format!(
                "could not seal: {e}; the next daemon start recovers the finished segments"
            )),
            None,
        ),
        Err(e) => finished(
            FinishedOutcome::Failed,
            Some(format!("could not seal: {e}")),
            None,
        ),
    }
}
