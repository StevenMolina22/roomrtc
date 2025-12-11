use std::time::Instant;

#[derive(Clone, Debug, )]
pub struct Clock {
    instant: Instant,
    start_time: u128,
}

impl Clock {
    pub fn new() -> Self {
        let instant = Instant::now();
        let start_time = instant.elapsed().as_millis();

        Self {
            instant,
            start_time,
        }
    }

    pub fn now(&self) -> u128 {
        let now = self.instant.elapsed().as_millis();
        now - self.start_time
    }
}