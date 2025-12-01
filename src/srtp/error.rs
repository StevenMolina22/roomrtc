use std::fmt;

#[derive(Debug)]
pub enum SrtpError {
    KeyDerivationFailed,
    EncryptionFailed,
    DecryptionFailed,
    AuthenticationFailed,
    PacketTooShort,
    OpenSslError(openssl::error::ErrorStack),
}

impl fmt::Display for SrtpError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SrtpError::KeyDerivationFailed => write!(f, "Key derivation failed"),
            SrtpError::EncryptionFailed => write!(f, "Encryption failed"),
            SrtpError::DecryptionFailed => write!(f, "Decryption failed"),
            SrtpError::AuthenticationFailed => write!(f, "Authentication failed"),
            SrtpError::PacketTooShort => write!(f, "Packet too short"),
            SrtpError::OpenSslError(e) => write!(f, "OpenSSL error: {}", e),
        }
    }
}

impl std::error::Error for SrtpError {}

impl From<openssl::error::ErrorStack> for SrtpError {
    fn from(err: openssl::error::ErrorStack) -> Self {
        SrtpError::OpenSslError(err)
    }
}
