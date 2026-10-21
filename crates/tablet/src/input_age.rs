//! How old pen samples are when the app starts a frame: the part of input-to-photon time the app
//! can measure from the input side (`stroke_trace` shows the rest of the frame).
//!
//! The platform glue records each sample's age when it reaches the app (the time since the digitizer
//! event, from the OS's own clock); [`InputAges::take`] reports, at the start of a frame, how old
//! the newest sample has become, which is the input queue, batching and the wait for the frame.

use std::time::{Duration, Instant};

/// Ages of the pen samples since the last [`InputAges::take`], in ms.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct InputAge {
    /// The newest sample's age now: the least any of them lags.
    pub newest_ms: f64,
    /// The oldest sample's age now: how long batching held the first of them.
    pub oldest_ms: f64,
}

#[derive(Debug, Default)]
pub struct InputAges {
    /// When the oldest and the newest sample since the last `take` happened.
    span: Option<(Instant, Instant)>,
}

impl InputAges {
    pub const fn new() -> Self {
        Self { span: None }
    }

    /// A sample that happened `age` before `arrived` (when it reached the app).
    pub fn record(&mut self, arrived: Instant, age: Duration) {
        let happened = arrived.checked_sub(age).unwrap_or(arrived);
        self.span = Some(match self.span {
            Some((oldest, newest)) => (oldest.min(happened), newest.max(happened)),
            None => (happened, happened),
        });
    }

    /// The ages at `now` of the samples recorded since the last call, or `None` without any.
    pub fn take(&mut self, now: Instant) -> Option<InputAge> {
        let (oldest, newest) = self.span.take()?;
        let ms = |t: Instant| now.saturating_duration_since(t).as_secs_f64() * 1000.0;
        Some(InputAge { newest_ms: ms(newest), oldest_ms: ms(oldest) })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_frame_sees_the_ages_of_the_samples_since_the_last_one() {
        let t0 = Instant::now();
        let mut ages = InputAges::new();
        assert_eq!(ages.take(t0), None);
        // Two samples that happened 10 ms and 4 ms before they arrived at t0 + 20 ms.
        ages.record(t0 + Duration::from_millis(20), Duration::from_millis(10));
        ages.record(t0 + Duration::from_millis(20), Duration::from_millis(4));
        let age = ages.take(t0 + Duration::from_millis(30)).expect("samples");
        assert!((age.newest_ms - 14.0).abs() < 1e-6 && (age.oldest_ms - 20.0).abs() < 1e-6, "{age:?}");
        assert_eq!(ages.take(t0 + Duration::from_millis(40)), None, "taken once");
    }

    #[test]
    fn samples_in_any_order_and_odd_ages_are_harmless() {
        let t0 = Instant::now() + Duration::from_secs(1);
        let mut ages = InputAges::new();
        ages.record(t0, Duration::from_millis(2));
        ages.record(t0, Duration::from_millis(8));
        // Older than the clock itself: counts as arriving now.
        ages.record(t0, Duration::MAX);
        let age = ages.take(t0).expect("samples");
        assert!((age.oldest_ms - 8.0).abs() < 1e-6 && (age.newest_ms - 0.0).abs() < 1e-6, "{age:?}");
        // A clock that reads earlier than the sample never gives a negative age.
        ages.record(t0, Duration::ZERO);
        assert_eq!(ages.take(t0 - Duration::from_millis(5)), Some(InputAge { newest_ms: 0.0, oldest_ms: 0.0 }));
    }
}
