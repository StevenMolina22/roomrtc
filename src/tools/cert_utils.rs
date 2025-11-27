use openssl::{
    asn1::Asn1Time, bn::BigNum, error::ErrorStack, hash::MessageDigest, pkcs12::Pkcs12, pkey::PKey,
    rsa::Rsa, x509::X509NameBuilder,
};
use std::fmt;
use udp_dtls::Identity;

pub const PKCS12_PASSWORD: &str = "roomrtc_pass";
const FRIENDLY_NAME: &str = "room-rtc-identity";
const SUBJECT_CN: &str = "RoomRTC-Peer";
const VALIDITY_DAYS: u32 = 365;
const SERIAL_NUMBER: u32 = 1;

/// Holds the local cryptographic identity for this session.
pub struct LocalCert {
    /// Opaque identity consumed by udp_dtls during the DTLS handshake.
    pub identity: Identity,
    /// The SHA-256 fingerprint string (e.g., "AA:BB:CC") advertised in SDP.
    pub fingerprint: String,
    pub pkcs12_der: Vec<u8>,
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
    Openssl(ErrorStack),
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

impl From<ErrorStack> for CertError {
    fn from(value: ErrorStack) -> Self {
        Self::Openssl(value)
    }
}

/// Generate a self-signed X.509 certificate, returning the udp_dtls
/// identity plus the SHA-256 fingerprint string needed by SDP.
pub fn generate_self_signed_cert() -> Result<LocalCert, CertError> {
    let rsa = Rsa::generate(2048)?; // 2048-bit RSA key
    let pkey = PKey::from_rsa(rsa)?; // Wrap in OpenSSL PKey

    let mut builder = openssl::x509::X509::builder()?;
    builder.set_version(2)?; // X.509 v3 format (0-indexed)

    let serial_bn = BigNum::from_u32(SERIAL_NUMBER)?; // Constant serial: 1
    let serial = serial_bn.to_asn1_integer()?;
    builder.set_serial_number(&serial)?;

    let mut name_builder = X509NameBuilder::new()?;
    name_builder.append_entry_by_text("CN", SUBJECT_CN)?; // "RoomRTC-Peer"
    let name = name_builder.build();

    builder.set_subject_name(&name)?;
    builder.set_issuer_name(&name)?; // Self-signed: issuer == subject

    let not_before = Asn1Time::days_from_now(0)?; // Valid immediately
    builder.set_not_before(&not_before)?;
    let not_after = Asn1Time::days_from_now(VALIDITY_DAYS)?; // 365 days
    builder.set_not_after(&not_after)?;

    builder.set_pubkey(&pkey)?; // Attach public key
    builder.sign(&pkey, MessageDigest::sha256())?; // Self-sign with SHA-256

    let cert = builder.build(); // Finalize certificate
    let digest = cert.digest(MessageDigest::sha256())?;
    let fingerprint = fingerprint_from_digest(digest.as_ref());

    // Package into PKCS#12 (.p12) archive with password
    let pkcs12 = Pkcs12::builder()
        .name(FRIENDLY_NAME) // "room-rtc-identity"
        .pkey(&pkey)
        .cert(&cert)
        .build2(PKCS12_PASSWORD)?; // Uses constant password
    let pkcs12_der = pkcs12.to_der()?;

    // Convert to DTLS identity used by udp_dtls crate
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
