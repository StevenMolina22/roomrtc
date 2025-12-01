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

    /// RTP configuration
    pub rtp: RtpConfig,

    /// SDP session-level configuration.
    pub sdp: SdpConfig,

    /// ICE candidate configuration.
    pub ice: IceConfig,

    pub server: ServerConfig,
}

#[derive(Debug, Clone)]
/// Network configuration for the application.
pub struct NetworkConfig {
    /// Address and port to bind sockets to (e.g. "0.0.0.0:8000").
    pub bind_address: String,

    /// Maximum UDP packet size for receiver buffer.
    pub max_udp_packet_size: usize,
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

    /// RTP packet version.
    pub rtp_version: u8,

    /// SDP media type (e.g. "video").
    pub media_type: String,

    /// SDP media protocol (e.g. "RTP/AVP").
    pub media_protocol: String,
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

#[derive(Debug, Clone)]
pub struct RtpConfig {
    /// `RtpPacket` max size supported
    pub max_packet_size: usize,

    /// Socket read timeout in milliseconds.
    pub read_timeout_millis: u64,
}

#[derive(Debug, Clone)]
/// SDP session-level configuration values.
pub struct SdpConfig {
    /// SDP version number.
    pub version: u8,

    /// Origin session identifier.
    pub origin_id: usize,

    /// Session name.
    pub session_name: String,

    /// Timing information.
    pub timing: String,

    /// Connection data network type.
    pub connection_data_net_type: String,

    /// Connection data address type.
    pub connection_data_addr_type: String,

    /// Connection data address.
    pub connection_data_address: String,
}

#[derive(Debug, Clone)]
/// ICE candidate configuration values.
pub struct IceConfig {
    /// Foundation identifier.
    pub foundation: String,

    /// Transport protocol.
    pub transport: String,

    /// Component identifier.
    pub component_id: u8,

    /// Host candidate type preference (RFC 8445).
    pub host_priority_preference: u32,

    /// Server reflexive candidate type preference (RFC 8445).
    pub srflx_priority_preference: u32,

    /// Host candidate local preference.
    pub host_local_preference: u16,
}

impl Default for SdpConfig {
    fn default() -> Self {
        Self {
            version: 0,
            origin_id: 0,
            session_name: "-".to_string(),
            timing: "0 0".to_string(),
            connection_data_net_type: "IN".to_string(),
            connection_data_addr_type: "IP4".to_string(),
            connection_data_address: "0.0.0.0".to_string(),
        }
    }
}

#[derive(Debug, Clone)]
///Configuration for Central Server
pub struct ServerConfig {
    ///Path to clients data file
    pub users_file: String,
    pub client_server_addr: String,
    pub server_client_addr: String,
    pub server_private_key_file: String,
    pub server_certification_file: String,
    pub server_name: String,
    pub max_amount_of_users_connected: usize,
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
        let rtp_section = conf.section(Some("rtp")).ok_or("Missing [rtp] section")?;
        let rtcp_section = conf.section(Some("rtcp")).ok_or("Missing [rtcp] section")?;
        let sdp_section = conf.section(Some("sdp")).ok_or("Missing [sdp] section")?;
        let ice_section = conf.section(Some("ice")).ok_or("Missing [ice] section")?;
        let server_section = conf
            .section(Some("server"))
            .ok_or("Missing [server] section")?;

        Ok(Self {
            network: NetworkConfig {
                bind_address: network_section
                    .get("bind_address")
                    .ok_or("Missing bind_address")?
                    .to_string(),
                max_udp_packet_size: network_section
                    .get("max_udp_packet_size")
                    .ok_or("Missing max_udp_packet_size")?
                    .parse()?,
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
                rtp_version: media_section
                    .get("rtp_version")
                    .ok_or("Missing rtp_version")?
                    .parse()?,
                media_type: media_section
                    .get("media_type")
                    .ok_or("Missing media_type")?
                    .to_string(),
                media_protocol: media_section
                    .get("media_protocol")
                    .ok_or("Missing media_protocol")?
                    .to_string(),
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
            rtp: RtpConfig {
                max_packet_size: rtp_section
                    .get("max_packet_size")
                    .ok_or("Missing max_packet_size")?
                    .parse()?,
                read_timeout_millis: rtp_section
                    .get("read_timeout_millis")
                    .ok_or("Missing read_timeout_millis")?
                    .parse()?,
            },
            sdp: SdpConfig {
                version: sdp_section
                    .get("version")
                    .ok_or("Missing version")?
                    .parse()?,
                origin_id: sdp_section
                    .get("origin_id")
                    .ok_or("Missing origin_id")?
                    .parse()?,
                session_name: sdp_section
                    .get("session_name")
                    .ok_or("Missing session_name")?
                    .to_string(),
                timing: sdp_section
                    .get("timing")
                    .ok_or("Missing timing")?
                    .to_string(),
                connection_data_net_type: sdp_section
                    .get("connection_data_net_type")
                    .ok_or("Missing connection_data_net_type")?
                    .to_string(),
                connection_data_addr_type: sdp_section
                    .get("connection_data_addr_type")
                    .ok_or("Missing connection_data_addr_type")?
                    .to_string(),
                connection_data_address: sdp_section
                    .get("connection_data_address")
                    .ok_or("Missing connection_data_address")?
                    .to_string(),
            },
            ice: IceConfig {
                foundation: ice_section
                    .get("foundation")
                    .ok_or("Missing foundation")?
                    .to_string(),
                transport: ice_section
                    .get("transport")
                    .ok_or("Missing transport")?
                    .to_string(),
                component_id: ice_section
                    .get("component_id")
                    .ok_or("Missing component_id")?
                    .parse()?,
                host_priority_preference: ice_section
                    .get("host_priority_preference")
                    .ok_or("Missing host_priority_preference")?
                    .parse()?,
                srflx_priority_preference: ice_section
                    .get("srflx_priority_preference")
                    .ok_or("Missing srflx_priority_preference")?
                    .parse()?,
                host_local_preference: ice_section
                    .get("host_local_preference")
                    .ok_or("Missing host_local_preference")?
                    .parse()?,
            },
            server: ServerConfig {
                users_file: server_section
                    .get("users_file")
                    .ok_or("Missing path to users file")?
                    .parse()?,
                client_server_addr: server_section
                    .get("client_server_addr")
                    .ok_or("Missing client-server address")?
                    .parse()?,
                server_client_addr: server_section
                    .get("server_client_addr")
                    .ok_or("Missing server-client address")?
                    .parse()?,
                server_private_key_file: server_section
                    .get("server_private_key_file")
                    .ok_or("Missing key file address")?
                    .parse()?,
                server_certification_file: server_section
                    .get("server_certification_file")
                    .ok_or("Missing certification file address")?
                    .parse()?,
                server_name: server_section
                    .get("server_name")
                    .ok_or("Missing server_name")?
                    .parse()?,
                max_amount_of_users_connected: server_section
                    .get("max_amount_of_users_connected")
                    .ok_or("Missing max amount of users connected")?
                    .parse()?,
            },
        })
    }

    #[must_use]
    pub fn network_only(&self) -> NetworkConfig {
        self.network.clone()
    }

    #[must_use]
    pub fn media_only(&self) -> MediaConfig {
        self.media.clone()
    }

    #[must_use]
    pub fn rtp_only(&self) -> RtpConfig {
        self.rtp.clone()
    }

    #[must_use]
    pub fn rtcp_only(&self) -> RtcpConfig {
        self.rtcp.clone()
    }

    #[must_use]
    pub fn sdp_only(&self) -> SdpConfig {
        self.sdp.clone()
    }

    #[must_use]
    pub fn ice_only(&self) -> IceConfig {
        self.ice.clone()
    }

    #[must_use]
    pub fn server_only(&self) -> ServerConfig {
        self.server.clone()
    }
}
