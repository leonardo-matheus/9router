use std::time::Duration;
use tracing::info;

#[derive(Debug, Clone)]
pub struct RetryPolicy {
    max_attempts: u32,
    current_attempt: u32,
    initial_delay: Duration,
    max_delay: Duration,
    multiplier: f64,
}

impl RetryPolicy {
    pub fn new(max_attempts: u32, initial_delay_ms: u64) -> Self {
        Self {
            max_attempts,
            current_attempt: 0,
            initial_delay: Duration::from_millis(initial_delay_ms),
            max_delay: Duration::from_secs(30),
            multiplier: 2.0,
        }
    }

    pub fn should_retry(&self) -> bool {
        self.current_attempt < self.max_attempts
    }

    #[allow(dead_code)]
    pub fn current_attempt(&self) -> u32 {
        self.current_attempt
    }

    #[allow(dead_code)]
    pub fn max_attempts(&self) -> u32 {
        self.max_attempts
    }

    pub fn next_delay(&mut self) -> Duration {
        self.current_attempt += 1;
        let delay = self.initial_delay.as_millis() as f64;
        let exponential_delay = delay * self.multiplier.powi(self.current_attempt as i32 - 1);
        let capped_delay = exponential_delay.min(self.max_delay.as_millis() as f64) as u64;
        info!(
            "Retry attempt {} (max: {}), next delay: {}ms",
            self.current_attempt, self.max_attempts, capped_delay
        );
        Duration::from_millis(capped_delay)
    }

    pub fn reset(&mut self) {
        self.current_attempt = 0;
        info!("Retry counter reset");
    }
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self::new(3, 1000)
    }
}
