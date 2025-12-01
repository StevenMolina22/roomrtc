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
            Self::KeyDerivationFailed => write!(f, "Key derivation failed"),
            Self::EncryptionFailed => write!(f, "Encryption failed"),
            Self::DecryptionFailed => write!(f, "Decryption failed"),
            Self::AuthenticationFailed => write!(f, "Authentication failed"),
            Self::PacketTooShort => write!(f, "Packet too short"),
            Self::OpenSslError(e) => write!(f, "OpenSSL error: {e}"),
        }
    }
}

impl std::error::Error for SrtpError {}

impl From<openssl::error::ErrorStack> for SrtpError {
    fn from(err: openssl::error::ErrorStack) -> Self {
        Self::OpenSslError(err)
    }
}
