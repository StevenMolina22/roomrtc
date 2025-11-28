mod media_pipeline;
mod camera;
mod error;

pub mod frame_handler;
pub use error::MediaPipelineError;
pub use camera::Camera;
pub use camera::FrameSource;
pub use media_pipeline::MediaPipeline;