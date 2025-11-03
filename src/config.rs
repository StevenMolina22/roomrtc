use std::path::Path;

use ini::Ini;

#[derive(Debug, Clone)]
/// Contains network, media and RTCP-related configuration used by
/// the application to configure sockets, camera capture and reporting
/// behavior.
pub struct Config {
    /// Network-related settings.
    pub network: NetworkConfig,

    /// Media capture and RTP-related settings.
    pub media: MediaConfig,

    /// RTCP reporting configuration.
    pub rtcp: RtcpConfig,
}

#[derive(Debug, Clone)]
/// Network configuration for the application.
pub struct NetworkConfig {
    /// Address and port to bind sockets to (e.g. "0.0.0.0:8000").
    pub bind_address: String,
}

#[derive(Debug, Clone)]
/// Media capture and RTP parameters.
///
/// These values control how the camera is configured, how frames are
/// chunked into RTP packets and the default codec/SSRC used for RTP
/// streams.
pub struct MediaConfig {
    /// Index of the camera device to open.
    pub camera_index: usize,

    /// Frame width in pixels.
    pub frame_width: f64,

    /// Frame height in pixels.
    pub frame_height: f64,

    /// Capture frame rate.
    pub frame_rate: u32,

    /// H.264 IDR interval (keyframe frequency) in frames.
    pub h264_idr_interval: usize,

    /// Maximum size (bytes) of RTP payload chunks.
    pub rtp_max_chunk_size: usize,

    /// Default SSRC to use for outgoing RTP streams.
    pub default_ssrc: u32,

    /// RTP payload type number for the chosen codec.
    pub rtp_payload_type: u8,

    /// Codec name (e.g. "H264"). Used in SDP generation.
    pub codec_name: String,

    /// Codec clock rate used in RTP timestamping and SDP.
    pub clock_rate: u32,
}

#[derive(Debug, Clone)]
/// Configuration for RTCP-style reporting used by the report handler.
pub struct RtcpConfig {
    /// Period between outgoing reports in milliseconds.
    pub report_period_millis: u64,

    /// Maximum allowed time (milliseconds) without receiving a report
    /// before considering the peer inactive.
    pub receive_limit_millis: u64,

    /// Number of consecutive receive timeouts before closing the
    /// connection.
    pub retry_limit: usize,
}

impl Config {
    /// Load configuration from the given INI file path.
    ///
    /// The INI file is expected to contain the following sections:
    /// - `[network]` with `bind_address`
    /// - `[media]` with camera and RTP-related keys
    /// - `[rtcp]` with reporting parameters
    ///
    /// # Errors
    /// Returns an error when the file cannot be read, a required
    /// section/key is missing, or when values cannot be parsed into
    /// the expected numeric types.
    pub fn load(path: &Path) -> Result<Self, Box<dyn std::error::Error>> {
        let conf = Ini::load_from_file(path)?;

        let network_section = conf
            .section(Some("network"))
            .ok_or("Missing [network] section")?;
        let media_section = conf
            .section(Some("media"))
            .ok_or("Missing [media] section")?;
        let rtcp_section = conf.section(Some("rtcp")).ok_or("Missing [rtcp] section")?;

        Ok(Self {
            network: NetworkConfig {
                bind_address: network_section
                    .get("bind_address")
                    .ok_or("Missing bind_address")?
                    .to_string(),
            },
            media: MediaConfig {
                camera_index: media_section
                    .get("camera_index")
                    .ok_or("Missing camera_index")?
                    .parse()?,
                frame_width: media_section
                    .get("frame_width")
                    .ok_or("Missing frame_width")?
                    .parse()?,
                frame_height: media_section
                    .get("frame_height")
                    .ok_or("Missing frame_height")?
                    .parse()?,
                frame_rate: media_section
                    .get("frame_rate")
                    .ok_or("Missing frame_rate")?
                    .parse()?,
                h264_idr_interval: media_section
                    .get("h264_idr_interval")
                    .ok_or("Missing h264_idr_interval")?
                    .parse()?,
                rtp_max_chunk_size: media_section
                    .get("rtp_max_chunk_size")
                    .ok_or("Missing rtp_max_chunk_size")?
                    .parse()?,
                default_ssrc: media_section
                    .get("default_ssrc")
                    .ok_or("Missing default_ssrc")?
                    .parse()?,
                rtp_payload_type: media_section
                    .get("rtp_payload_type")
                    .ok_or("Missing rtp_payload_type")?
                    .parse()?,
                codec_name: media_section
                    .get("codec_name")
                    .ok_or("Missing codec_name")?
                    .to_string(),
                clock_rate: media_section
                    .get("clock_rate")
                    .ok_or("Missing clock_rate")?
                    .parse()?,
            },
            rtcp: RtcpConfig {
                report_period_millis: rtcp_section
                    .get("report_period_millis")
                    .ok_or("Missing report_period_millis")?
                    .parse()?,
                receive_limit_millis: rtcp_section
                    .get("receive_limit_millis")
                    .ok_or("Missing receive_limit_millis")?
                    .parse()?,
                retry_limit: rtcp_section
                    .get("retry_limit")
                    .ok_or("Missing retry_limit")?
                    .parse()?,
            },
        })
    }
}
