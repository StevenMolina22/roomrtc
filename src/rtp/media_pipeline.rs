use gstreamer as gst;
use gstreamer_app as gst_app;
use gstreamer_video as gst_video;
use opencv::prelude::*;
use std::error::Error;

use gst::prelude::*;

pub struct MediaPipeline {
    pipeline: gst::Pipeline,
    appsrc: gst_app::AppSrc,
}

impl MediaPipeline {
    /// Creates a new sending pipeline.
    /// This pipeline takes raw video frames from `appsrc`,
    /// converts, encodes (H264), packetizes (RTP), and sends (UDP).
    pub fn new_send_pipeline(
        target_host: &str,
        target_port: u16,
    ) -> Result<Self, Box<dyn Error>> {
        gst::init()?;

        let pipeline_str = format!(
            "appsrc name=rust_src ! \
             videoconvert ! \
             video/x-raw,format=I420 ! \
             openh264enc ! \
             rtph264pay config-interval=1 ! \
             udpsink host={} port={}",
            target_host, target_port
        );

        let pipeline = gst::parse::launch(&pipeline_str)?
            .downcast::<gst::Pipeline>()
            .map_err(|_| "Failed to downcast to Pipeline")?;

        let appsrc = pipeline
            .by_name("rust_src")
            .ok_or("Failed to get appsrc element")?
            .downcast::<gst_app::AppSrc>()
            .map_err(|_| "Failed to downcast to AppSrc")?;

        // Set properties on appsrc
        appsrc.set_format(gst::Format::Time);
        appsrc.set_is_live(true);
        appsrc.set_do_timestamp(true);

        Ok(Self { pipeline, appsrc })
    }

    /// Creates a new receiving pipeline.
    /// This pipeline receives UDP, depacketizes, decodes, and displays.
    pub fn new_recv_pipeline(listen_port: u16) -> Result<(), Box<dyn Error>> {
        gst::init()?;

        let pipeline_str = format!(
            "udpsrc port={} ! \
             application/x-rtp, media=video, clock-rate=90000, encoding-name=H264 ! \
             rtph264depay ! \
             openh264dec ! \
             videoconvert ! \
             autovideosink",
            listen_port
        );

        let pipeline = gst::parse::launch(&pipeline_str)?
            .downcast::<gst::Pipeline>()
            .map_err(|_| "Failed to downcast to Pipeline")?;

        pipeline.set_state(gst::State::Playing)?;

        // We'll run this pipeline in a separate thread to not block
        std::thread::spawn(move || {
            let bus = pipeline.bus().unwrap();
            for msg in bus.iter_timed(gst::ClockTime::NONE) {
                use gst::MessageView;
                match msg.view() {
                    MessageView::Error(err) => {
                        eprintln!(
                            "Error from element {}: {} ({})",
                            msg.src().map_or("None", |s| s.path_string()),
                            err.error(),
                            err.debug().unwrap_or_default()
                        );
                        break;
                    }
                    MessageView::Eos(_) => break,
                    _ => (),
                }
            }
        });

        Ok(())
    }

    /// Starts the sending pipeline.
    pub fn start(&self) -> Result<(), Box<dyn Error>> {
        self.pipeline
            .set_state(gst::State::Playing)
            .map_err(|_| "Failed to set pipeline to Playing")?;
        Ok(())
    }

    /// Pushes a raw frame from OpenCV into the pipeline.
    pub fn push_frame(&self, frame: &Mat) -> Result<(), Box<dyn Error>> {
        let data = frame.data_bytes()?;
        let mut buffer = gst::Buffer::with_size(data.len())?;

        {
            let mut map = buffer.map_writable()?;
            map.copy_from_slice(data);
        }

        // We need to set the video info (caps) on the appsrc
        // This should only be done once, or when the format changes.
        if self.appsrc.caps().is_none() {
            let caps = gst_video::VideoInfo::builder(
                gst_video::VideoFormat::Bgr, // OpenCV's default format
                frame.cols() as u32,
                frame.rows() as u32,
            )
                .build()?
                .to_caps()?;
            self.appsrc.set_caps(Some(&caps));
        }

        self.appsrc
            .push_buffer(buffer)
            .map_err(|_| "Failed to push buffer to appsrc")?;

        Ok(())
    }
}