//! The system clock adapter.

use chrono::{DateTime, Utc};
use overcut_application::ports::Clock;

/// Reads the current time from the operating system.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> DateTime<Utc> {
        Utc::now()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn given_the_system_clock_when_now_is_read_twice_then_time_does_not_go_backwards() {
        let clock = SystemClock;
        let first = clock.now();
        let second = clock.now();
        assert!(second >= first);
    }
}
