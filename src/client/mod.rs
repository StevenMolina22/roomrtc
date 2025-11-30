mod error;

pub use error::ClientError;

use crate::{
    config::{IceConfig, MediaConfig, SdpConfig},
    dtls::key_manager::{LocalCert, generate_self_signed_cert},
    ice::IceAgent,
    sdp::{Attribute, DtlsSetupRole, Fingerprint, MediaDescription, SessionDescriptionProtocol},
};
use std::collections::HashSet;
use std::str::FromStr;

use error::ClientError as Error;

/// High-level client that exposes SDP and ICE operations used by the UI
/// and signaling code.
///
/// `Client` holds a local `SessionDescriptionProtocol` and an `IceAgent`.
/// It can create an SDP offer, process remote offers/answers and drive
/// ICE connectivity checks.
pub struct Client {
    /// Local SDP state representing current media descriptions.
    pub sdp: SessionDescriptionProtocol,

    /// ICE agent responsible for gathering local candidates and
    /// performing connectivity checks with remote candidates.
    pub ice_agent: IceAgent,

    /// Locally generated certificate and DTLS identity for DTLS handshakes.
    pub local_cert: LocalCert,

    /// Remote DTLS fingerprint advertised via SDP.
    pub remote_fingerprint: Option<Fingerprint>,

    /// Remote DTLS setup role advertised via SDP.
    pub remote_setup_role: Option<DtlsSetupRole>,

    /// Local DTLS setup role negotiated from signaling.
    pub local_setup_role: DtlsSetupRole,
}

impl Client {
    /// Create a new `Client` using the provided media port and codec
    /// configuration.
    /// - `ice_config`: ICE configuration for candidate creation.
    /// - `sdp_config`: SDP session-level configuration values.
    ///
    /// Parameters:
    /// - `media_port`: UDP port where ICE candidate gathering is performed
    ///   and where the endpoint expects/receives RTP packets.
    /// - `media_config`: media stream configuration (payload type, codec
    ///   name and clock rate).
    /// - `ice_config`: ICE configuration for candidate creation.
    /// - `sdp_config`: SDP session-level configuration values.
    ///
    /// This method performs the following steps:
    /// 1. Creates an `IceAgent` and calls `gather_candidates` to obtain
    ///    local candidates.
    /// 2. Builds a local `MediaDescription` and adds an `rtpmap` attribute
    ///    (and a `candidate` attribute if a local candidate is available).
    /// 3. Initializes the local `SessionDescriptionProtocol` and, if a
    ///    local candidate exists, sets the connection data (`c=`) to the
    ///    candidate's IP address.
    ///
    /// # Returns
    /// Returns a `Client` containing the local SDP and an `IceAgent`
    /// already configured with (potentially) gathered candidates.
    pub fn new(
        media_port: u16,
        media_config: &MediaConfig,
        ice_config: &IceConfig,
        sdp_config: &SdpConfig,
    ) -> Result<Self, ClientError> {
        let mut ice_agent = IceAgent::new();
        ice_agent
            .gather_candidates(media_port, ice_config)
            .map_err(|e| {
                ClientError::IceConnectionError(format!("Failed to gather ICE candidates: {e}"))
            })?;

        let mut media_description = MediaDescription::new(
            media_config.media_type.clone(),
            media_port,
            media_config.media_protocol.clone(),
            HashSet::from([media_config.rtp_payload_type]),
        );
        media_description
            .add_attribute(Attribute::RTPMap(
                media_config.rtp_payload_type,
                media_config.codec_name.clone(),
                media_config.clock_rate,
                None,
            ))
            .map_err(|e| {
                ClientError::SdpCreationError(format!("Failed to add RTPMap attribute: {e}"))
            })?;

        if let Some(candidate) = ice_agent.get_local_candidate() {
            media_description
                .add_attribute(Attribute::Candidate(candidate.clone()))
                .map_err(|e| {
                    ClientError::SdpCreationError(format!("Failed to add Candidate attribute: {e}"))
                })?;
        }

        let local_cert = generate_self_signed_cert().map_err(|e| {
            ClientError::SecurityInitializationError(format!(
                "Failed to generate local certificate: {e}"
            ))
        })?;

        let fingerprint = Fingerprint::from_hash_string("sha-256", &local_cert.fingerprint)
            .map_err(|e| {
                ClientError::SdpCreationError(format!(
                    "Failed to encode local fingerprint attribute: {e}"
                ))
            })?;
        media_description
            .add_attribute(Attribute::Fingerprint(fingerprint))
            .map_err(|e| {
                ClientError::SdpCreationError(format!("Failed to add fingerprint attribute: {e}"))
            })?;

        let local_setup_role = DtlsSetupRole::ActPass;
        media_description
            .add_attribute(Attribute::Setup(local_setup_role))
            .map_err(|e| {
                ClientError::SdpCreationError(format!("Failed to add setup attribute: {e}"))
            })?;

        let mut sdp = SessionDescriptionProtocol::new(vec![media_description], sdp_config);

        if let Some(local_candidate) = ice_agent.get_local_candidate() {
            sdp.set_connection_data("IN", "IP4", local_candidate.address.clone().as_str());
        }

        Ok(Self {
            sdp,
            ice_agent,
            local_cert,
            remote_fingerprint: None,
            remote_setup_role: None,
            local_setup_role,
        })
    }

    /// Process an SDP offer string and return the generated SDP answer
    /// string. The method parses the incoming SDP, generates a local
    /// answer using the current local SDP state, and adds remote ICE
    /// candidates to the local `IceAgent` so connectivity checks can
    /// start.
    pub fn process_offer(&mut self, offer_str: &str) -> Result<String, Error> {
        let sdp_offer = SessionDescriptionProtocol::from_str(offer_str)
            .map_err(|e| Error::SdpCreationError(e.to_string()))?;

        let desired_role = self.determine_local_role(&sdp_offer, RemoteSdpType::Offer);
        self.set_local_setup_role(desired_role)?;

        let answer = self
            .sdp
            .create_answer(&sdp_offer)
            .map(|answer_sdp| answer_sdp.to_string())
            .map_err(|e| Error::SdpCreationError(e.to_string()))?;

        self.process_remote_sdp(&sdp_offer, RemoteSdpType::Offer)?;

        Ok(answer)
    }

    /// Process an SDP answer string (from the remote peer). This will
    /// parse the SDP and add any remote ICE candidates found to the
    /// local `IceAgent` and start connectivity checks.
    pub fn process_answer(&mut self, answer_str: &str) -> Result<(), Error> {
        let sdp_answer = SessionDescriptionProtocol::from_str(answer_str)
            .map_err(|e| Error::SdpCreationError(e.to_string()))?;

        self.process_remote_sdp(&sdp_answer, RemoteSdpType::Answer)?;

        Ok(())
    }

    /// Return the local SDP offer string generated from the current
    /// `SessionDescriptionProtocol` state.
    #[must_use]
    pub fn get_offer(&self) -> String {
        self.sdp.to_string()
    }

    /// Internal helper that walks a remote `SessionDescriptionProtocol`,
    /// adds remote ICE candidates to the `IceAgent` and starts
    /// connectivity checks.
    fn process_remote_sdp(
        &mut self,
        sdp: &SessionDescriptionProtocol,
        remote_type: RemoteSdpType,
    ) -> Result<(), Error> {
        for md in &sdp.media_descriptions {
            for candidate in md.get_candidates() {
                self.ice_agent
                    .add_remote_candidate(candidate.clone())
                    .map_err(|e| Error::IceConnectionError(e.to_string()))?;
            }

            if let Some(fingerprint) = md.get_fingerprint() {
                self.remote_fingerprint = Some(fingerprint);
            }

            if let Some(remote_role) = md.get_setup_role() {
                self.remote_setup_role = Some(remote_role);
                let desired_role = self.determine_complementary_role(remote_role, remote_type);
                self.set_local_setup_role(desired_role)?;
            }
        }

        self.ice_agent
            .start_connectivity_checks()
            .map_err(|e| Error::IceConnectionError(e.to_string()))
    }

    fn determine_local_role(
        &self,
        remote_sdp: &SessionDescriptionProtocol,
        remote_type: RemoteSdpType,
    ) -> DtlsSetupRole {
        if let Some(remote_role) = remote_sdp
            .media_descriptions
            .first()
            .and_then(MediaDescription::get_setup_role)
        {
            self.determine_complementary_role(remote_role, remote_type)
        } else if matches!(remote_type, RemoteSdpType::Offer) {
            DtlsSetupRole::Active
        } else {
            self.local_setup_role
        }
    }

    fn determine_complementary_role(
        &self,
        remote_role: DtlsSetupRole,
        remote_type: RemoteSdpType,
    ) -> DtlsSetupRole {
        match remote_role {
            DtlsSetupRole::Active => DtlsSetupRole::Passive,
            DtlsSetupRole::Passive => DtlsSetupRole::Active,
            DtlsSetupRole::HoldConn => DtlsSetupRole::Passive,
            DtlsSetupRole::ActPass => {
                if matches!(remote_type, RemoteSdpType::Offer) {
                    DtlsSetupRole::Active
                } else {
                    DtlsSetupRole::Passive
                }
            }
        }
    }

    fn set_local_setup_role(&mut self, role: DtlsSetupRole) -> Result<(), ClientError> {
        if self.local_setup_role == role {
            return Ok(());
        }

        for md in &mut self.sdp.media_descriptions {
            md.attributes
                .retain(|attr| !matches!(attr, Attribute::Setup(_)));
            md.add_attribute(Attribute::Setup(role)).map_err(|e| {
                ClientError::SdpCreationError(format!("Failed to update setup attribute: {e}"))
            })?;
        }

        self.local_setup_role = role;
        Ok(())
    }
}

#[derive(Clone, Copy)]
enum RemoteSdpType {
    Offer,
    Answer,
}
