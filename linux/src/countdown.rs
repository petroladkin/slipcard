//! The Close button's countdown: "Close (N)" ticking down from the configured
//! seconds; at zero the popup closes, which is never an answer. Once the user
//! reacts (the pointer moves over the popup, or Show) it stops for good and
//! the button reads "Close". Plain logic: the time (Unix milliseconds) is
//! passed in. Same rules as Countdown.swift.

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Countdown {
    seconds: u32,
    started_ms: i64,
    stopped: bool,
}

impl Countdown {
    pub fn new(seconds: u32, started_ms: i64) -> Self {
        Countdown { seconds, started_ms, stopped: false }
    }

    pub fn is_stopped(&self) -> bool {
        self.stopped
    }

    /// Whole seconds left at `now_ms`, rounded up (the full count at the
    /// start, 1 in the last second, 0 at the end); None once stopped.
    pub fn remaining(&self, now_ms: i64) -> Option<u32> {
        if self.stopped {
            return None;
        }
        let left_ms = i64::from(self.seconds) * 1000 - (now_ms - self.started_ms);
        Some(if left_ms <= 0 { 0 } else { ((left_ms + 999) / 1000) as u32 })
    }

    /// The Close button's label at `now_ms`.
    pub fn label(&self, now_ms: i64) -> String {
        match self.remaining(now_ms) {
            Some(left) => format!("Close ({left})"),
            None => "Close".into(),
        }
    }

    /// The popup should close now: the count reached zero and nobody reacted.
    pub fn is_expired(&self, now_ms: i64) -> bool {
        self.remaining(now_ms) == Some(0)
    }

    /// Final: a stopped countdown never runs again.
    pub fn stop(&mut self) {
        self.stopped = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const T0: i64 = 1_800_000_000_000;

    #[test]
    fn countdown_starts_at_the_configured_seconds() {
        assert_eq!(Countdown::new(30, T0).label(T0), "Close (30)");
        assert_eq!(Countdown::new(45, T0).label(T0), "Close (45)");
    }

    #[test]
    fn countdown_ticks_down_on_whole_seconds() {
        let c = Countdown::new(30, T0);
        assert_eq!(c.label(T0 + 500), "Close (30)");
        assert_eq!(c.label(T0 + 1_000), "Close (29)");
        assert_eq!(c.label(T0 + 10_000), "Close (20)");
        assert_eq!(c.label(T0 + 29_500), "Close (1)");
        assert_eq!(c.label(T0 + 30_000), "Close (0)");
    }

    #[test]
    fn countdown_expires_at_zero_not_before() {
        let c = Countdown::new(30, T0);
        assert!(!c.is_expired(T0 + 29_900));
        assert!(c.is_expired(T0 + 30_000));
        assert!(c.is_expired(T0 + 300_000));
    }

    #[test]
    fn a_stopped_countdown_reads_close_and_never_expires() {
        let mut c = Countdown::new(30, T0);
        c.stop();
        assert_eq!(c.label(T0 + 5_000), "Close");
        assert_eq!(c.remaining(T0 + 5_000), None);
        assert!(!c.is_expired(T0 + 3_600_000));
    }
}
