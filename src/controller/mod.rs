mod app_handler;
mod error;
#[cfg(test)]
mod mock_utils;

pub use app_handler::Controller;
pub use error::{ControllerError, ThreadsError};

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use crate::{controller::mock_utils::setup_peer, frame_handler::Frame};

    #[test]
    fn test_full_media_pipeline_e2e() {
        // Setup two peers with a mock camera
        let mut peer_a = setup_peer();
        let mut peer_b = setup_peer();

        // Manual handshake, swap ICE candidates
        let candidate_a = peer_a
            .controller
            .client
            .ice_agent
            .get_local_candidate()
            .unwrap()
            .clone();
        let candidate_b = peer_b
            .controller
            .client
            .ice_agent
            .get_local_candidate()
            .unwrap()
            .clone();

        peer_a
            .controller
            .client
            .ice_agent
            .add_remote_candidate(candidate_b)
            .unwrap();
        peer_b
            .controller
            .client
            .ice_agent
            .add_remote_candidate(candidate_a)
            .unwrap();

        // connectivity checks
        peer_a
            .controller
            .client
            .ice_agent
            .start_connectivity_checks()
            .unwrap();
        peer_b
            .controller
            .client
            .ice_agent
            .start_connectivity_checks()
            .unwrap();

        // Start the calls
        // This connects sockets and starts all media threads
        std::thread::scope(|s| {
            s.spawn(|| {
                if let Err(e) = peer_a.controller.start_call() {
                    panic!("Peer A failed to start call: {e}");
                }
            });
            s.spawn(|| {
                if let Err(e) = peer_b.controller.start_call() {
                    panic!("Peer B failed to start call: {e}");
                }
            });
        });

        // Send a frame from A to B
        let test_frame = Frame {
            data: vec![128; 640 * 480 * 3], // simple gray frame
            width: 640,
            height: 480,
            id: 42,
        };

        // Send the frame into Peer A's mock camera
        peer_a.tx_inject_frame.send(test_frame.clone()).unwrap();

        // Check if Peer B received the frame
        let received_frame = peer_b
            .rx_remote_frame
            .recv_timeout(Duration::from_secs(5))
            .expect("Test timed out waiting for remote frame");

        assert_eq!(received_frame.id, test_frame.id, "Frame ID mismatch");
        assert_eq!(
            received_frame.width, test_frame.width,
            "Frame width mismatch"
        );
        assert_eq!(
            received_frame.height, test_frame.height,
            "Frame height mismatch"
        );

        assert!(
            !received_frame.data.is_empty(),
            "Received frame data is empty"
        );

        peer_a.controller.shut_down().unwrap();
        peer_b.controller.shut_down().unwrap();

        // Check if Peer A got the "call ended" event
        let event = peer_a
            .rx_event
            .recv_timeout(Duration::from_secs(3))
            .expect("Peer A did not receive a connection closed event");

        assert!(event.contains("Connection closed"));
    }
}
