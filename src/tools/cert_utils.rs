use openssl::{
    asn1::Asn1Time, bn::BigNum, hash::MessageDigest, pkcs12::Pkcs12, pkey::PKey, rsa::Rsa,
    x509::X509NameBuilder,
};
use std::fmt;
use udp_dtls::Identity;

/// Holds the local cryptographic identity for this session.
pub struct LocalCert {
    /// Opaque identity consumed by udp_dtls during the DTLS handshake.
    pub identity: Identity,
    /// The SHA-256 fingerprint string (e.g., "AA:BB:CC") advertised in SDP.
    pub fingerprint: String,
    pkcs12_der: Vec<u8>,
}

impl LocalCert {
    /// Re-create the identity from the stored PKCS#12 material.
    pub fn duplicate_identity(&self) -> Result<Identity, CertError> {
        Identity::from_pkcs12(&self.pkcs12_der, PKCS12_PASSWORD)
            .map_err(|e| CertError::Identity(format!("Failed to clone identity: {e}")))
    }
}

/// Errors that can occur while creating a self-signed certificate.
#[derive(Debug)]
pub enum CertError {
    Openssl(openssl::error::ErrorStack),
    Identity(String),
}

impl fmt::Display for CertError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Openssl(err) => write!(f, "{err}"),
            Self::Identity(err) => write!(f, "{err}"),
        }
    }
}

impl std::error::Error for CertError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Openssl(err) => Some(err),
            Self::Identity(_) => None,
        }
    }
}

impl From<openssl::error::ErrorStack> for CertError {
    fn from(value: openssl::error::ErrorStack) -> Self {
        Self::Openssl(value)
    }
}

const PKCS12_PASSWORD: &str = "roomrtc_pass";
const FRIENDLY_NAME: &str = "room-rtc-identity";
const SUBJECT_CN: &str = "RoomRTC-Peer";
const VALIDITY_DAYS: u32 = 365;
const SERIAL_NUMBER: u32 = 1;

/// Generate a self-signed X.509 certificate, returning the udp_dtls
/// identity plus the SHA-256 fingerprint string needed by SDP.
pub fn generate_self_signed_cert() -> Result<LocalCert, CertError> {
    let rsa = Rsa::generate(2048)?;
    let pkey = PKey::from_rsa(rsa)?;

    let mut builder = openssl::x509::X509::builder()?;
    builder.set_version(2)?;

    let serial_bn = BigNum::from_u32(SERIAL_NUMBER)?;
    let serial = serial_bn.to_asn1_integer()?;
    builder.set_serial_number(&serial)?;

    let mut name_builder = X509NameBuilder::new()?;
    name_builder.append_entry_by_text("CN", SUBJECT_CN)?;
    let name = name_builder.build();

    builder.set_subject_name(&name)?;
    builder.set_issuer_name(&name)?;
    let not_before = Asn1Time::days_from_now(0)?;
    builder.set_not_before(&not_before)?;
    let not_after = Asn1Time::days_from_now(VALIDITY_DAYS)?;
    builder.set_not_after(&not_after)?;
    builder.set_pubkey(&pkey)?;
    builder.sign(&pkey, MessageDigest::sha256())?;

    let cert = builder.build();
    let digest = cert.digest(MessageDigest::sha256())?;
    let fingerprint = fingerprint_from_digest(digest.as_ref());

    let pkcs12 = Pkcs12::builder()
        .name(FRIENDLY_NAME)
        .pkey(&pkey)
        .cert(&cert)
        .build2(PKCS12_PASSWORD)?;
    let pkcs12_der = pkcs12.to_der()?;

    let identity = Identity::from_pkcs12(&pkcs12_der, PKCS12_PASSWORD)
        .map_err(|e| CertError::Identity(format!("DTLS identity creation failed: {e}")))?;

    Ok(LocalCert {
        identity,
        fingerprint,
        pkcs12_der,
    })
}

fn fingerprint_from_digest(digest: &[u8]) -> String {
    digest
        .iter()
        .map(|byte| format!("{byte:02X}"))
        .collect::<Vec<String>>()
        .join(":")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generates_certificate_and_fingerprint() {
        let cert = generate_self_signed_cert().expect("certificate should generate");
        assert!(!cert.fingerprint.is_empty());
        assert!(cert.fingerprint.contains(':'));
        assert_eq!(
            cert.fingerprint.split(':').count(),
            32,
            "SHA-256 fingerprint must have 32 bytes"
        );
        cert.duplicate_identity()
            .expect("cloning identity should succeed");
    }
}
