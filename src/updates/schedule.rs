use std::time::{Duration, Instant};

pub const MIN_INTERVAL_MINUTES: u16 = 5;
pub const MAX_INTERVAL_MINUTES: u16 = 10_080;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Settings {
    pub auto_check: bool,
    pub interval_minutes: u16,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            auto_check: true,
            interval_minutes: 60,
        }
    }
}

impl Settings {
    pub fn interval(self) -> Duration {
        Duration::from_secs(
            u64::from(
                self.interval_minutes
                    .clamp(MIN_INTERVAL_MINUTES, MAX_INTERVAL_MINUTES),
            ) * 60,
        )
    }
}

pub struct Schedule {
    settings: Settings,
    last_check: Option<Instant>,
}

impl Schedule {
    pub fn new(settings: Settings) -> Self {
        Self {
            settings,
            last_check: None,
        }
    }

    pub fn settings(&self) -> Settings {
        self.settings
    }

    pub fn configure(&mut self, settings: Settings) {
        self.settings = settings;
    }

    pub fn record_check(&mut self, now: Instant) {
        self.last_check = Some(now);
    }

    pub fn delay(&self, now: Instant) -> Option<Duration> {
        if !self.settings.auto_check {
            return None;
        }
        // Preserve the last completed attempt across interval/enable edits, including failed and
        // manual checks, so preference changes cannot create a rapid retry loop.
        Some(
            self.last_check
                .map(|last| (last + self.settings.interval()).saturating_duration_since(now))
                .unwrap_or(Duration::ZERO),
        )
    }
}
