//! Session lifetime for the audio fanout thread.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

/// Identifies one audio-capture session and invalidates older sessions on claim.
pub struct AudioSessionGuard {
    generation: Arc<AtomicU64>,
    mine: u64,
}

impl AudioSessionGuard {
    /// Claims a new generation, making every previously issued guard stale.
    pub fn claim(generation: Arc<AtomicU64>) -> Self {
        let mine = generation.fetch_add(1, Ordering::SeqCst) + 1;
        Self { generation, mine }
    }

    /// Returns whether this guard still owns the active audio session.
    pub fn is_current(&self) -> bool {
        self.generation.load(Ordering::SeqCst) == self.mine
    }

    /// Sleeps in slices and returns false as soon as the session is retired.
    pub fn sleep_interruptible(&self, total: Duration, slice: Duration) -> bool {
        if slice.is_zero() {
            return false;
        }

        let mut remaining = total;
        while !remaining.is_zero() {
            if !self.is_current() {
                return false;
            }
            let step = slice.min(remaining);
            std::thread::sleep(step);
            remaining -= step;
        }
        self.is_current()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicU64;
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    fn generation() -> Arc<AtomicU64> {
        Arc::new(AtomicU64::new(0))
    }

    #[test]
    fn a_freshly_claimed_session_is_current() {
        let gen = generation();
        let guard = AudioSessionGuard::claim(gen);
        assert!(guard.is_current());
    }

    #[test]
    fn claiming_a_new_session_invalidates_the_previous_one() {
        let gen = generation();
        let first = AudioSessionGuard::claim(gen.clone());
        let second = AudioSessionGuard::claim(gen);

        assert!(!first.is_current(), "old fanout thread must exit");
        assert!(second.is_current());
    }

    #[test]
    fn invalidating_the_counter_retires_the_current_session() {
        let gen = generation();
        let guard = AudioSessionGuard::claim(gen.clone());

        gen.fetch_add(1, Ordering::SeqCst);

        assert!(!guard.is_current());
    }

    #[test]
    fn interruptible_sleep_runs_to_completion_while_current() {
        let gen = generation();
        let guard = AudioSessionGuard::claim(gen);

        let started = Instant::now();
        let completed =
            guard.sleep_interruptible(Duration::from_millis(120), Duration::from_millis(20));

        assert!(completed);
        assert!(started.elapsed() >= Duration::from_millis(100));
    }

    #[test]
    fn interruptible_sleep_wakes_early_when_the_session_is_retired() {
        let gen = generation();
        let guard = AudioSessionGuard::claim(gen.clone());

        let bumper = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(30));
            gen.fetch_add(1, Ordering::SeqCst);
        });

        let started = Instant::now();
        let completed =
            guard.sleep_interruptible(Duration::from_secs(5), Duration::from_millis(10));
        bumper.join().unwrap();

        assert!(!completed, "sleep must abandon a retired session");
        assert!(started.elapsed() < Duration::from_secs(1));
    }

    #[test]
    fn reconnect_claims_a_new_generation_and_drops_stale_fanout() {
        let gen = generation();
        let first = AudioSessionGuard::claim(gen.clone());
        let second = AudioSessionGuard::claim(gen.clone());
        let third = AudioSessionGuard::claim(gen);

        assert!(!first.is_current());
        assert!(!second.is_current());
        assert!(third.is_current());
    }

    #[test]
    fn stale_final_from_a_retired_session_is_detectable_by_generation() {
        let gen = generation();
        let live = AudioSessionGuard::claim(gen.clone());
        assert!(live.is_current());

        AudioSessionGuard::claim(gen);
        assert!(
            !live.is_current(),
            "a reconnect must invalidate finals that still carry the old generation"
        );
    }
}
