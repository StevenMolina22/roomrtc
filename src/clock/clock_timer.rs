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
#[derive(Clone, Debug)]
pub struct Clock {
    /// The starting instant for time measurement.
    instant: Instant,
    /// Initial elapsed time in milliseconds (used to adjust subsequent measurements).
    start_time: u128,
}

impl Default for Clock {
    fn default() -> Self {
        Self::new()
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;
    use std::time::Duration;

    #[test]
    fn test_new_clock_starts_near_zero() {
        let clock = Clock::new();
        let elapsed = clock.now();

        assert!(
            elapsed < 5,
            "El reloj recién creado debería tener un tiempo cercano a 0, se obtuvo: {}",
            elapsed
        );
    }

    #[test]
    fn test_default_implementation() {
        let clock = Clock::default();
        let elapsed = clock.now();
        assert!(elapsed < 5);
    }

    #[test]
    fn test_time_progression() {
        let clock = Clock::new();
        let sleep_time_ms = 50;

        thread::sleep(Duration::from_millis(sleep_time_ms));

        let elapsed = clock.now();

        assert!(
            elapsed >= sleep_time_ms as u128,
            "Se esperaban al menos {}ms, pero el reloj marcó {}ms",
            sleep_time_ms,
            elapsed
        );
    }

    #[test]
    fn test_monotonicity() {
        let clock = Clock::new();

        let t1 = clock.now();
        thread::sleep(Duration::from_millis(10));
        let t2 = clock.now();

        assert!(
            t2 > t1,
            "El tiempo posterior (t2) debe ser mayor al anterior (t1)"
        );
    }

    #[test]
    fn test_clone_preserves_start_time() {
        let clock = Clock::new();

        thread::sleep(Duration::from_millis(50));

        let clock_clone = clock.clone();

        let elapsed_original = clock.now();
        let elapsed_clone = clock_clone.now();

        let diff = elapsed_original.abs_diff(elapsed_clone);

        assert!(
            diff <= 1,
            "El reloj clonado debería estar sincronizado con el original"
        );
        assert!(
            elapsed_clone >= 50,
            "El clon reinició el contador, lo cual es incorrecto"
        );
    }

    #[test]
    fn test_debug_format() {
        let clock = Clock::new();
        let debug_str = format!("{:?}", clock);
        assert!(debug_str.contains("Clock"));
        assert!(debug_str.contains("instant"));
        assert!(debug_str.contains("start_time"));
    }
}
