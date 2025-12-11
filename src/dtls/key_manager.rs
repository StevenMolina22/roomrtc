use openssl::{
    asn1::Asn1Time,
    bn::BigNum,
    ec::{EcGroup, EcKey},
    error::ErrorStack,
    hash::MessageDigest,
    nid::Nid,
    pkcs12::Pkcs12,
    pkey::PKey,
    x509::X509NameBuilder,
};
use std::fmt;
use udp_dtls::Identity;

/// Default password used to protect the PKCS#12 archive.
pub const PKCS12_PASSWORD: &str = "roomrtc_pass";

/// Friendly name assigned to the identity bundle within PKCS#12.
const FRIENDLY_NAME: &str = "room-rtc-identity";

/// Common Name (CN) field used in the self-signed X.509 certificate.
const SUBJECT_CN: &str = "RoomRTC-Peer";

/// Validity period for the self-signed certificate, in days.
const VALIDITY_DAYS: u32 = 365;

/// Constant serial number assigned to the generated certificate.
const SERIAL_NUMBER: u32 = 1;

/// Holds the local cryptographic identity for this session.
///
/// This structure encapsulates all cryptographic material required for the DTLS handshake
/// and subsequent secure communication. The identity is derived from an EC (Elliptic Curve)
/// key pair and a self-signed X.509 certificate using the NIST P-256 curve, which is the
/// standard for WebRTC applications.
///
/// # Fields
///
/// * `identity` - Opaque identity consumed by `udp_dtls` during the DTLS handshake.
/// * `fingerprint` - The SHA-256 fingerprint string (e.g., "AA:BB:CC") advertised in SDP.
/// * `pkcs12_der` - The entire local cryptographic identity (private key and X.509 certificate)
///   bundled into a PKCS#12 archive and serialized using DER encoding.
pub struct LocalCert {
    /// Opaque identity consumed by `udp_dtls` during the DTLS handshake.
    pub identity: Identity,
    /// The SHA-256 fingerprint string (e.g., "AA:BB:CC") advertised in SDP.
    pub fingerprint: String,
    /// The entire local cryptographic identity (private key and X.509 certificate)
    /// bundled into a PKCS#12 archive and serialized using DER encoding.
    pub pkcs12_der: Vec<u8>,
}

impl LocalCert {
    /// Recreates the DTLS identity from the stored PKCS#12 material.
    ///
    /// This method allows cloning the identity for use in multiple DTLS connections
    /// without regenerating the cryptographic keys.
    ///
    /// # Returns
    ///
    /// * `Ok(Identity)` - A new DTLS identity derived from the stored PKCS#12 data.
    /// * `Err(CertError)` - If the identity cannot be created from the stored material.
    pub fn duplicate_identity(&self) -> Result<Identity, CertError> {
        Identity::from_pkcs12(&self.pkcs12_der, PKCS12_PASSWORD)
            .map_err(|e| CertError::Identity(format!("Failed to clone identity: {e}")))
    }
}

/// Errors that can occur while creating a self-signed certificate.
///
/// This enum represents failures in certificate generation, OpenSSL operations,
/// or DTLS identity creation.
///
/// # Variants
///
/// - `Openssl`: An error from the OpenSSL library.
/// - `Identity`: An error creating the DTLS identity.
#[derive(Debug)]
pub enum CertError {
    /// OpenSSL library error.
    Openssl(ErrorStack),
    /// DTLS identity creation error with details.
    Identity(String),
}

impl fmt::Display for CertError {
    /// Formats the certificate error for display.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Openssl(err) => write!(f, "{err}"),
            Self::Identity(err) => write!(f, "{err}"),
        }
    }
}

/// Returns the underlying error source if available.
impl std::error::Error for CertError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Openssl(err) => Some(err),
            Self::Identity(_) => None,
        }
    }
}

/// Converts an OpenSSL ErrorStack into a CertError.
impl From<ErrorStack> for CertError {
    fn from(value: ErrorStack) -> Self {
        Self::Openssl(value)
    }
}

/// Generates a self-signed X.509 certificate using Elliptic Curve Cryptography (ECDSA).
///
/// This function creates a complete cryptographic identity suitable for WebRTC DTLS handshakes:
/// - Generates an EC key pair using the NIST P-256 curve (X9.62_PRIME256V1), the standard for WebRTC
/// - Creates a self-signed X.509 certificate with the generated key
/// - Computes the SHA-256 fingerprint for SDP advertisement
/// - Packages everything into a PKCS#12 archive for portability
/// - Creates a DTLS identity for use in secure communication
///
/// # Returns
///
/// * `Ok(LocalCert)` - Successfully generated certificate with identity and fingerprint.
/// * `Err(CertError)` - Failed during certificate generation, OpenSSL operation, or identity creation.
///
/// # Examples
///
/// ```ignore
/// let local_cert = generate_self_signed_cert()?;
/// println!("Fingerprint: {}", local_cert.fingerprint);
/// ```
pub fn generate_self_signed_cert() -> Result<LocalCert, CertError> {
    // Generate EC Key using NIST P-256 (standard for WebRTC)
    let group = EcGroup::from_curve_name(Nid::X9_62_PRIME256V1)?;
    let ec_key = EcKey::generate(&group)?;
    let pkey = PKey::from_ec_key(ec_key)?;

    // Build the X.509 Certificate
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
    // OpenSSL automatically detects the key type (EC) and uses ECDSA-SHA256
    builder.sign(&pkey, MessageDigest::sha256())?;

    let cert = builder.build();
    let digest = cert.digest(MessageDigest::sha256())?;
    let fingerprint = fingerprint_from_digest(digest.as_ref());

    // Package into PKCS#12
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

// Converts a SHA-256 digest to colon-separated hexadecimal representation
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
    // Tests that certificate generation produces valid fingerprint and identity
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
