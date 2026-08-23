//! Reconnect-budget accounting and provider-neutral confidence defaults
//! shared by the streaming providers.

use std::time::Duration;

/// A connection that stayed up at least this long was healthy; its later
/// failure must not count against the session reconnect budget. Without the
/// reset, five transient drops spread over a whole sermon permanently killed
/// streaming even though every drop reconnected successfully.
pub(crate) const HEALTHY_CONNECTION_UPTIME: Duration = Duration::from_secs(30);

/// Advance the reconnect-attempt budget after a failed connection.
///
/// A connection that stayed up for [`HEALTHY_CONNECTION_UPTIME`] or longer
/// resets the budget before this failure is counted, so only rapid
/// back-to-back failures exhaust `MAX_RECONNECT_ATTEMPTS`.
pub(crate) fn track_reconnect_attempt(attempts: u32, connection_uptime: Duration) -> u32 {
    if connection_uptime >= HEALTHY_CONNECTION_UPTIME {
        1
    } else {
        attempts.saturating_add(1)
    }
}

/// Confidence reported for final results when the provider supplies no word
/// scores. Shared so identical downstream gates mean the same thing for every
/// provider (Soniox and Speechmatics previously claimed 1.0 while the Vosk
/// worker used 0.75, so the same confidence threshold behaved differently
/// per provider).
pub(crate) const UNSCORED_FINAL_CONFIDENCE: f64 = 0.75;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rapid_failures_accumulate() {
        assert_eq!(track_reconnect_attempt(0, Duration::from_secs(0)), 1);
        assert_eq!(track_reconnect_attempt(1, Duration::from_secs(2)), 2);
        assert_eq!(track_reconnect_attempt(4, Duration::from_secs(10)), 5);
    }

    #[test]
    fn healthy_connection_resets_the_budget() {
        // A connection that survived a sermon-scale stretch earns a fresh
        // budget: its eventual failure counts as the first attempt.
        assert_eq!(
            track_reconnect_attempt(4, HEALTHY_CONNECTION_UPTIME),
            1,
            "uptime == threshold must reset"
        );
        assert_eq!(
            track_reconnect_attempt(4, Duration::from_secs(3_600)),
            1,
            "one-hour connection must reset"
        );
        // Just under the threshold still counts.
        assert_eq!(
            track_reconnect_attempt(
                4,
                HEALTHY_CONNECTION_UPTIME
                    .checked_sub(Duration::from_millis(1))
                    .expect("threshold is far above zero")
            ),
            5
        );
    }
}
