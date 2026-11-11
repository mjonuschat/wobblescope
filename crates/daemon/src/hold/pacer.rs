//! The host-clock pacer.
//!
//! No reading of the toolhead is fresher than Klipper's 0.25 s subscription timer, so the pacer
//! predicts how much motion is queued from what it sent. Time is an `f64` in seconds on the host
//! clock, which keeps the pacer pure and its tests exact.

/// The most motion the pacer lets be outstanding, counting the piece it is about to send, in
/// seconds. The first piece is exempt, because Klipper adds its own idle start buffer to it.
pub const AHEAD_SECONDS: f64 = 0.5;

/// What happened when a piece was sent.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Sent {
    /// When the piece starts to move, on the host clock.
    pub motion_start: f64,
    /// Whether the queue had run dry before this piece, so the motion had a gap.
    pub starved: bool,
}

/// Decides when the next piece may be sent. See the module documentation.
#[derive(Debug, Clone)]
pub struct Pacer {
    start_buffer: f64,
    /// The predicted end of the queued motion, once something was sent.
    end: Option<f64>,
}

impl Pacer {
    /// A pacer for a printer whose idle start buffer is `start_buffer` seconds.
    #[must_use]
    pub fn new(start_buffer: f64) -> Self {
        Self {
            start_buffer,
            end: None,
        }
    }

    /// The motion still to play at `now`, in seconds.
    #[must_use]
    pub fn outstanding(&self, now: f64) -> f64 {
        self.end.map_or(0.0, |end| (end - now).max(0.0))
    }

    /// Whether a piece of `duration` seconds may be sent at `now`.
    #[must_use]
    pub fn admits(&self, now: f64, duration: f64) -> bool {
        self.end.is_none() || self.outstanding(now) + duration <= AHEAD_SECONDS + 1e-9
    }

    /// The earliest time a piece of `duration` seconds is admitted, never before `now`.
    #[must_use]
    pub fn next_send_time(&self, now: f64, duration: f64) -> f64 {
        match self.end {
            Some(end) if !self.admits(now, duration) => end - (AHEAD_SECONDS - duration),
            _ => now,
        }
    }

    /// Records that a piece of `duration` seconds was sent at `now`.
    pub fn sent(&mut self, now: f64, duration: f64) -> Sent {
        let starved = self.end.is_some_and(|end| end <= now);
        let start = match self.end {
            Some(end) if end > now => end,
            _ => now + self.start_buffer,
        };
        self.end = Some(start + duration);
        Sent {
            motion_start: start,
            starved,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const EPS: f64 = 1e-9;

    #[test]
    fn the_first_piece_is_always_admitted() {
        let mut pacer = Pacer::new(0.37);
        assert!(pacer.admits(0.0, 0.25));
        let sent = pacer.sent(0.0, 0.25);
        assert!((sent.motion_start - 0.37).abs() < EPS);
        assert!(!sent.starved);
        assert!((pacer.outstanding(0.0) - 0.62).abs() < EPS);
    }

    #[test]
    fn later_pieces_must_fit_in_the_ahead_limit() {
        let mut pacer = Pacer::new(0.37);
        pacer.sent(0.0, 0.25);
        assert!(!pacer.admits(0.1, 0.25));
        assert!(pacer.admits(0.37, 0.25));
    }

    #[test]
    fn next_send_time_is_when_it_fits() {
        let mut pacer = Pacer::new(0.37);
        pacer.sent(0.0, 0.25);
        assert!((pacer.next_send_time(0.0, 0.25) - 0.37).abs() < EPS);
        assert_eq!(pacer.next_send_time(0.5, 0.25), 0.5);
    }

    #[test]
    fn steady_pacing_never_starves_and_stays_in_the_limit() {
        let mut pacer = Pacer::new(0.37);
        let mut now = 0.0;
        for i in 0..40 {
            now = pacer.next_send_time(now, 0.25);
            let sent = pacer.sent(now, 0.25);
            assert!(!sent.starved, "piece {i} starved");
            if i > 0 {
                assert!(pacer.outstanding(now) <= AHEAD_SECONDS + EPS, "piece {i}");
            }
        }
    }

    #[test]
    fn a_late_send_is_starved_and_restarts_the_buffer() {
        let mut pacer = Pacer::new(0.37);
        pacer.sent(0.0, 0.25);
        let sent = pacer.sent(2.0, 0.25);
        assert!(sent.starved);
        assert!((sent.motion_start - 2.37).abs() < EPS);
    }
}
