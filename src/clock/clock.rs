use std::time::Instant;

/// A clock for measuring elapsed time from creation.
///
/// `Clock` provides a convenient way to track time relative to when it was created.
/// It uses `Instant` for high-resolution time measurement and returns elapsed time
/// in milliseconds since the clock was instantiated.
///
/// # Examples
///
/// ```ignore
/// let clock = Clock::new();
/// // ... do some work ...
/// let elapsed = clock.now();
/// println!("Elapsed: {} ms", elapsed);
/// ```
#[derive(Clone, Debug, )]
pub struct Clock {
    /// The starting instant for time measurement.
    instant: Instant,
    /// Initial elapsed time in milliseconds (used to adjust subsequent measurements).
    start_time: u128,
}

impl Clock {
    /// Creates a new clock and starts measuring time.
    ///
    /// The clock records the current instant and initializes the start time reference.
    /// All subsequent calls to `now()` will return time elapsed relative to this creation.
    ///
    /// # Returns
    ///
    /// A new `Clock` instance ready to measure elapsed time.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// let clock = Clock::new();
    /// ```
    pub fn new() -> Self {
        let instant = Instant::now();
        let start_time = instant.elapsed().as_millis();

        Self {
            instant,
            start_time,
        }
    }

    /// Returns the elapsed time in milliseconds since the clock was created.
    ///
    /// This method calculates the time difference between the current moment
    /// and when the clock was instantiated, returning the result in milliseconds.
    ///
    /// # Returns
    ///
    /// The elapsed time in milliseconds as a `u128`.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// let clock = Clock::new();
    /// let elapsed = clock.now();
    /// assert!(elapsed >= 0);
    /// ```
    pub fn now(&self) -> u128 {
        let now = self.instant.elapsed().as_millis();
        now - self.start_time
    }
}