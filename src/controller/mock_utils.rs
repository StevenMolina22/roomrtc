// use std::path::Path;
// use std::sync::mpsc::{self, Receiver, Sender};
// use std::sync::{Arc, Mutex};
// 
// use crate::media::FrameSource;
// use crate::config::Config;
// use crate::controller::{AppHandler, ControllerError};
// use crate::media::frame_handler::Frame;
// 
// /// Mock camera to manually send frames in tests.
// pub struct MockCamera {
//     pub rx_frames: Option<Receiver<Frame>>,
// }
// 
// /// Holds all the parts needed to test one peer.
// pub struct TestPeer {
//     pub controller: AppHandler,
//     pub tx_inject_frame: Sender<Frame>, // Send frames to the camera
//     pub rx_remote_frame: Receiver<Frame>, // Receive frames from the controller
//     pub rx_event: Receiver<String>,     // Receive error events
//     _rx_local: Receiver<Frame>,         // Keep this channel open
// }
// 
// impl MockCamera {
//     /// Creates a new mock camera.
//     fn new(rx_frames: Receiver<Frame>) -> Self {
//         Self {
//             rx_frames: Some(rx_frames),
//         }
//     }
// }
// 
// impl FrameSource for MockCamera {
//     fn start(&mut self) -> Result<Receiver<Frame>, ControllerError> {
//         // Give the receiver to the controller when it starts
//         self.rx_frames.take().ok_or_else(|| {
//             ControllerError::MapError("MockCamera::start() called more than once".to_string())
//         })
//     }
// 
//     fn stop(&self) -> Result<(), ControllerError> {
//         // Do nothing
//         Ok(())
//     }
// }
// 
// /// Helper function to build a complete test peer.
// pub fn setup_peer() -> Result<TestPeer, ControllerError> {
//     let (tx_local, rx_local) = mpsc::channel();
//     let (tx_remote, rx_remote) = mpsc::channel();
//     let (tx_event, rx_event) = mpsc::channel();
// 
//     // This channel is for injecting frames into the mock camera
//     let (tx_inject_frame, rx_for_camera) = mpsc::channel();
// 
//     let config = Arc::new(
//         Config::load(Path::new("room_rtc.conf"))
//             .map_err(|e| ControllerError::MapError(e.to_string()))?,
//     );
// 
//     let mut controller = AppHandler::new(tx_local, tx_remote, tx_event, config.clone())?;
// 
//     // Make a mock camera and swap it with the real one
//     let mock_cam = MockCamera::new(rx_for_camera);
//     controller.camera = Arc::new(Mutex::new(Box::new(mock_cam)));
// 
//     Ok(TestPeer {
//         controller,
//         tx_inject_frame,
//         rx_remote_frame: rx_remote,
//         rx_event,
//         _rx_local: rx_local,
//     })
// }
