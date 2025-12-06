use crate::config::Config;
use crate::dtls::key_manager::{LocalCert, generate_self_signed_cert};
use crate::session::error::CallSessionError as Error;
use crate::session::ice::{CandidatePair, IceAgent};
use crate::session::sdp::{Attribute, MediaDescription, SessionDescriptionProtocol};
use crate::session::sdp::{DtlsSetupRole, Fingerprint};
use std::collections::HashSet;
use std::net::UdpSocket;
use std::sync::Arc;

use crate::logger::Logger;

/// High-level session that exposes SDP and ICE operations used by the UI
/// and signaling code.
pub struct CallSession {
    /// Local SDP state representing current media descriptions.
    pub sdp: SessionDescriptionProtocol,

    /// ICE agent responsible for gathering local candidates and
    /// performing connectivity checks with remote candidates.
    pub ice_agent: IceAgent,

    /// UDP socket used for STUN/ICE and media transport.
    pub socket: UdpSocket, 

    /// Locally generated certificate and DTLS identity for DTLS handshakes.
    pub local_cert: LocalCert,

    /// Remote DTLS fingerprint advertised via SDP.
    pub remote_fingerprint: Option<Fingerprint>,

    /// Remote DTLS setup role advertised via SDP.
    pub remote_setup_role: Option<DtlsSetupRole>,

    /// Local DTLS setup role negotiated from signaling.
    pub local_setup_role: DtlsSetupRole,

    logger: Logger,
}

#[derive(Clone, Copy)]
enum RemoteSdpType {
    Offer,
    Answer,
}

impl CallSession {
    /// Create a new `CallSession` using the provided media port and codec
    /// configuration.
    pub fn new(
        stun_socket: UdpSocket,
        config: &Arc<Config>,
        logger: Logger,
    ) -> Result<Self, Error> {
        let mut ice_agent = IceAgent::new(logger.context("IceAgent"));
        ice_agent
            .gather_candidates(&stun_socket, &config.ice)
            .map_err(|e| {
                Error::IceConnectionError(format!("Failed to gather ICE candidates: {e}"))
            })?;

        let mut media_description = MediaDescription::new(
            config.media.media_type.clone(),
            stun_socket
                .local_addr()
                .map_err(|e| Error::IceConnectionError(e.to_string()))?
                .port(),
            config.media.media_protocol.clone(),
            HashSet::from([config.media.rtp_payload_type]),
        );
        media_description
            .add_attribute(Attribute::RTPMap(
                config.media.rtp_payload_type,
                config.media.codec_name.clone(),
                config.media.clock_rate,
                None,
            ))
            .map_err(|e| Error::SdpCreationError(format!("Failed to add RTPMap attribute: {e}")))?;

        for candidate in ice_agent.get_local_candidates() {
            media_description
                .add_attribute(Attribute::Candidate(candidate.clone()))
                .map_err(|e| {
                    Error::SdpCreationError(format!("Failed to add Candidate attribute: {e}"))
                })?;
        }

        let local_cert = generate_self_signed_cert().map_err(|e| {
            Error::SecurityInitializationError(format!("Failed to generate local certificate: {e}"))
        })?;

        let fingerprint = Fingerprint::from_hash_string("sha-256", &local_cert.fingerprint)
            .map_err(|e| {
                Error::SdpCreationError(format!(
                    "Failed to encode local fingerprint attribute: {e}"
                ))
            })?;

        media_description
            .add_attribute(Attribute::Fingerprint(fingerprint))
            .map_err(|e| {
                Error::SdpCreationError(format!("Failed to add fingerprint attribute: {e}"))
            })?;

        let local_setup_role = DtlsSetupRole::ActPass;
        media_description
            .add_attribute(Attribute::Setup(local_setup_role))
            .map_err(|e| Error::SdpCreationError(format!("Failed to add setup attribute: {e}")))?;

        let mut sdp = SessionDescriptionProtocol::new(vec![media_description], &config.sdp);

        if let Some(local_candidate) = ice_agent.get_local_candidate() {
            sdp.set_connection_data("IN", "IP4", local_candidate.address.clone().as_str());
        }

        Ok(Self {
            sdp,
            ice_agent,
            socket: stun_socket,
            local_cert,
            remote_fingerprint: None,
            remote_setup_role: None,
            local_setup_role,
            logger,
        })
    }

    /// Process an SDP offer string and return the generated SDP answer
    /// string. The method parses the incoming SDP, generates a local
    /// answer using the current local SDP state, and adds remote ICE
    /// candidates to the local `IceAgent` so connectivity checks can
    /// start.
    pub fn process_offer(
        &mut self,
        offer_sdp: &SessionDescriptionProtocol,
    ) -> Result<SessionDescriptionProtocol, Error> {
        let desired_role = self.determine_local_role(offer_sdp, RemoteSdpType::Offer);
        self.set_local_setup_role(desired_role)?;

        let answer_sdp = self
            .sdp
            .create_answer(offer_sdp)
            .map_err(|e| Error::SdpCreationError(e.to_string()))?;

        self.process_remote_sdp(offer_sdp, RemoteSdpType::Offer)?;

        Ok(answer_sdp)
    }

    /// Process an SDP answer string (from the remote peer). This will
    /// parse the SDP and add any remote ICE candidates found to the
    /// local `IceAgent` and start connectivity checks.
    pub fn process_answer(&mut self, answer_sdp: &SessionDescriptionProtocol) -> Result<(), Error> {
        self.process_remote_sdp(answer_sdp, RemoteSdpType::Answer)
    }

    /// Return the local SDP offer, a copy from the current
    /// `SessionDescriptionProtocol` state.
    #[must_use]
    pub fn get_offer(&self) -> SessionDescriptionProtocol {
        self.sdp.clone()
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
            .start_connectivity_checks(&self.socket) 
            .map_err(|e| Error::IceConnectionError(e.to_string()))
    }

    pub fn get_selected_pair(&self) -> Result<&CandidatePair, Error> {
        self.ice_agent
            .get_selected_pair()
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

    const fn determine_complementary_role(
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

    fn set_local_setup_role(&mut self, role: DtlsSetupRole) -> Result<(), Error> {
        if self.local_setup_role == role {
            return Ok(());
        }

        for md in &mut self.sdp.media_descriptions {
            md.attributes
                .retain(|attr| !matches!(attr, Attribute::Setup(_)));
            md.add_attribute(Attribute::Setup(role)).map_err(|e| {
                Error::SdpCreationError(format!("Failed to update setup attribute: {e}"))
            })?;
        }

        self.local_setup_role = role;
        Ok(())
    }
}