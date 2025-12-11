use std::fmt::Display;

/// Errors that can occur during camera operations.
///
/// This enum represents various failures that may occur when initializing,
/// configuring, or capturing frames from a camera device.
///
/// # Variants
///
/// - `PoisonedLock`: Mutex or RwLock was poisoned.
/// - `IndexError`: Invalid camera device index.
/// - `OpenError`: Failed to open the camera device.
/// - `ClosedCamera`: Camera is not open or has been closed.
/// - `CameraConfigFailed`: Failed to configure camera properties.
pub enum CameraError {
    /// A poisoned mutex or RwLock was encountered.
    PoisonedLock,

    /// Camera device index is invalid or out of range.
    IndexError,

    /// Failed to open the camera device with error details.
    OpenError(String),

    /// Camera is closed or not available.
    ClosedCamera,

    /// Failed to set camera configuration (resolution, FPS, etc.).
    CameraConfigFailed,
}

/// Formats a readable representation of the camera error.
impl Display for CameraError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> Result<(), std::fmt::Error> {
        match self {
            Self::PoisonedLock => write!(f, "Error: \"Poisoned lock\""),
            Self::IndexError => write!(f, "Error: \"Device index is too large\""),
            Self::OpenError(e) => write!(f, "Error: \"Failed to open camera\": {e}"),
            Self::ClosedCamera => write!(f, "Error: \"Camera is closed\""),
            Self::CameraConfigFailed => write!(f, "Error: \"Failed to set camera configuration\""),
        }
    }
}
