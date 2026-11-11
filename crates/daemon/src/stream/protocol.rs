//! Messages exchanged with a stream viewer and the binary sample frame.

use serde::{Deserialize, Serialize};

use crate::config::Settings;

/// The most chips one viewer may subscribe to at once.
pub const MAX_CHIPS: usize = 4;

/// The least display update rate a viewer may request.
pub use crate::config::MIN_UPDATES_PER_SECOND;

/// The least top frequency a viewer may request, in Hz.
const MIN_MAX_HZ: f64 = 1.0;

/// A message from the viewer.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClientMessage {
    /// Start streaming a chip. Subscribing to a chip that is already subscribed changes that
    /// chip's settings instead of refusing.
    Subscribe {
        /// The chip name.
        chip: String,
        /// The top of the frequency range to show, in Hz.
        max_hz: f64,
        /// How many display updates to send per second.
        updates_per_second: f64,
    },
    /// Stop streaming a chip.
    Unsubscribe {
        /// The chip name.
        chip: String,
    },
}

/// What the daemon uses when the viewer does not choose.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct StreamDefaults {
    /// The top of the frequency range, in Hz.
    pub max_hz: f64,
    /// Display updates per second. This is the printing value while printing, else the idle one.
    pub updates_per_second: f64,
}

impl StreamDefaults {
    /// The defaults from `settings` for a printer that is `printing` or idle.
    pub fn for_state(settings: &Settings, printing: bool) -> Self {
        Self {
            max_hz: settings.stream_default_max_hz,
            updates_per_second: if printing {
                settings.stream_printing_updates_per_second
            } else {
                settings.stream_idle_updates_per_second
            },
        }
    }
}

/// The most a viewer may ask for.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct StreamLimits {
    /// The most display updates per second.
    pub max_updates_per_second: f64,
    /// The highest top frequency, in Hz.
    pub max_hz: f64,
    /// The most chips at once.
    pub max_chips: usize,
}

impl StreamLimits {
    /// The limits from `settings`.
    pub fn from_settings(settings: &Settings) -> Self {
        Self {
            max_updates_per_second: settings.stream_max_updates_per_second,
            max_hz: settings.stream_max_hz,
            max_chips: MAX_CHIPS,
        }
    }
}

/// A message to the viewer.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerMessage {
    /// Sent on connect.
    Hello {
        /// Whether a print is running.
        printing: bool,
        /// The values used when the viewer does not choose.
        defaults: StreamDefaults,
        /// The most the viewer may ask for.
        limits: StreamLimits,
    },
    /// A chip's filter was built or rebuilt.
    Stream {
        /// The chip name.
        chip: String,
        /// The index that tags this chip's binary frames.
        index: u16,
        /// The output sample rate, in Hz.
        rate_hz: f64,
        /// The chip's sample rate, in Hz.
        input_rate_hz: f64,
        /// The rate the chip is configured for, when known.
        configured_rate_hz: Option<f64>,
        /// The decimation factor from input to output.
        factor: usize,
        /// The top frequency in effect, in Hz.
        max_hz: f64,
        /// The display updates per second in effect.
        updates_per_second: f64,
    },
    /// Sent once a second per chip.
    Health {
        /// The chip name.
        chip: String,
        /// The sample rate measured over the last second, in Hz.
        measured_rate_hz: f64,
        /// The rate the chip is configured for, when known.
        configured_rate_hz: Option<f64>,
        /// The chip's overflow count, when known.
        overflows: Option<u64>,
        /// The chip's error count, when known.
        errors: Option<u64>,
        /// Frames dropped from the outgoing queue. The websocket handler fills this in. The
        /// pipeline emits 0.
        dropped_frames: u64,
        /// Problems worth showing to the viewer.
        warnings: Vec<String>,
    },
    /// A chip's stream ended.
    Closed {
        /// The chip name.
        chip: String,
        /// Why it ended.
        reason: String,
    },
    /// A subscription was refused.
    Refused {
        /// The chip name.
        chip: String,
        /// Why it was refused.
        reason: String,
    },
    /// Lines from the printer console.
    Console {
        /// The new lines.
        lines: Vec<String>,
    },
}

/// Encodes a binary frame, little endian: `u16` index, `u16` zero, `f64` start time,
/// `u32` sample count, then `3 * count` `f32` values (x, y, z per sample).
pub fn encode_frame(index: u16, t0: f64, xyz: &[f32]) -> Vec<u8> {
    let count = (xyz.len() / 3) as u32;
    let mut out = Vec::with_capacity(16 + xyz.len() * 4);
    out.extend_from_slice(&index.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&t0.to_le_bytes());
    out.extend_from_slice(&count.to_le_bytes());
    for value in xyz {
        out.extend_from_slice(&value.to_le_bytes());
    }
    out
}

/// `value` limited to `[low, high]`. NaN becomes `low`, and `low` wins if the range is empty.
fn bounded(value: f64, low: f64, high: f64) -> f64 {
    if value.is_nan() {
        low
    } else {
        value.min(high).max(low)
    }
}

/// Limits a viewer's request to what the daemon allows for a chip sampling at
/// `input_rate_hz`. The top frequency goes into `[1, min(limits.max_hz, input_rate_hz / 2)]` and the
/// update rate into `[MIN_UPDATES_PER_SECOND, limits.max_updates_per_second]`.
pub fn clamp_request(
    max_hz: f64,
    updates: f64,
    input_rate_hz: f64,
    limits: &StreamLimits,
) -> (f64, f64) {
    let top = limits.max_hz.min(input_rate_hz / 2.0);
    (
        bounded(max_hz, MIN_MAX_HZ, top),
        clamp_updates(updates, limits),
    )
}

/// `updates` limited to `[MIN_UPDATES_PER_SECOND, limits.max_updates_per_second]`. NaN becomes
/// the minimum.
pub fn clamp_updates(updates: f64, limits: &StreamLimits) -> f64 {
    bounded(
        updates,
        MIN_UPDATES_PER_SECOND,
        limits.max_updates_per_second,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn limits() -> StreamLimits {
        StreamLimits {
            max_updates_per_second: 20.0,
            max_hz: 1600.0,
            max_chips: MAX_CHIPS,
        }
    }

    #[test]
    fn encode_frame_lays_out_header_and_samples() {
        let bytes = encode_frame(3, 1.5, &[1.0, 2.0, 3.0, 4.0, 5.0, 6.0]);
        assert_eq!(bytes.len(), 40);
        assert_eq!(u16::from_le_bytes([bytes[0], bytes[1]]), 3);
        assert_eq!(u16::from_le_bytes([bytes[2], bytes[3]]), 0);
        assert_eq!(f64::from_le_bytes(bytes[4..12].try_into().unwrap()), 1.5);
        assert_eq!(u32::from_le_bytes(bytes[12..16].try_into().unwrap()), 2);
        assert_eq!(f32::from_le_bytes(bytes[16..20].try_into().unwrap()), 1.0);
        assert_eq!(f32::from_le_bytes(bytes[36..40].try_into().unwrap()), 6.0);
    }

    #[test]
    fn client_message_parses_subscribe() {
        let message: ClientMessage = serde_json::from_str(
            r#"{"type":"subscribe","chip":"adxl345","max_hz":200,"updates_per_second":5}"#,
        )
        .unwrap();
        assert_eq!(
            message,
            ClientMessage::Subscribe {
                chip: "adxl345".into(),
                max_hz: 200.0,
                updates_per_second: 5.0,
            }
        );
    }

    #[test]
    fn client_message_rejects_unknown_type() {
        assert!(serde_json::from_str::<ClientMessage>(r#"{"type":"bogus"}"#).is_err());
    }

    #[test]
    fn clamp_request_uses_full_range_of_a_3200_hz_chip() {
        assert_eq!(
            clamp_request(5000.0, 100.0, 3200.0, &limits()),
            (1600.0, 20.0)
        );
    }

    #[test]
    fn clamp_request_keeps_values_in_range() {
        assert_eq!(
            clamp_request(300.0, 100.0, 3200.0, &limits()),
            (300.0, 20.0)
        );
    }

    #[test]
    fn clamp_request_limits_to_nyquist_of_a_slow_chip() {
        assert_eq!(clamp_request(1000.0, 5.0, 800.0, &limits()), (400.0, 5.0));
    }

    #[test]
    fn clamp_request_falls_back_to_lower_bounds_on_nan_and_negatives() {
        assert_eq!(clamp_request(f64::NAN, -3.0, 3200.0, &limits()), (1.0, 0.5));
    }

    #[test]
    fn limits_and_defaults_come_from_settings() {
        let settings = Settings::default();
        assert_eq!(
            StreamLimits::from_settings(&settings),
            StreamLimits {
                max_updates_per_second: 20.0,
                max_hz: 1600.0,
                max_chips: MAX_CHIPS,
            }
        );
        assert_eq!(
            StreamDefaults::for_state(&settings, true),
            StreamDefaults {
                max_hz: 200.0,
                updates_per_second: 5.0
            }
        );
        assert_eq!(
            StreamDefaults::for_state(&settings, false).updates_per_second,
            10.0
        );
    }
}
