use std::path::Path;

use ini::Ini;

#[derive(Debug, Clone)]
pub struct Config {
    pub network: NetworkConfig,
    pub media: MediaConfig,
    pub rtcp: RtcpConfig,
}

#[derive(Debug, Clone)]
pub struct NetworkConfig {
    pub bind_address: String,
}

#[derive(Debug, Clone)]
pub struct MediaConfig {
    pub camera_index: usize,
    pub frame_width: f64,
    pub frame_height: f64,
    pub frame_rate: u32,
    pub h264_idr_interval: usize,
    pub rtp_max_chunk_size: usize,
    pub default_ssrc: u32,
    pub rtp_payload_type: u8,
    pub codec_name: String,
    pub clock_rate: u32,
}

#[derive(Debug, Clone)]
pub struct RtcpConfig {
    pub report_period_millis: u64,
    pub receive_limit_millis: u64,
    pub retry_limit: usize,
}

impl Config {
    /// Loads configuration from the specified file path.
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
