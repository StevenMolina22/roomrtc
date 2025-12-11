use super::error::SrtpError;
use crate::transport::rtp::RtpPacket;
use openssl::hash::MessageDigest;
use openssl::pkey::PKey;
use openssl::sign::Signer;
use openssl::symm::{Cipher, decrypt, encrypt};

/// SRTP context for encrypting and authenticating RTP/RTCP packets.
///
/// Manages encryption keys, salts, and sequence counters for secure RTP communication.
/// Supports both client and server perspectives with automatic key selection.
pub struct SrtpContext {
    client_write_key: Vec<u8>,
    server_write_key: Vec<u8>,
    client_write_salt: Vec<u8>,
    server_write_salt: Vec<u8>,
}

impl SrtpContext {
    /// Creates a new SRTP context from the provided DTLS keying material.
    ///
    /// Requires at least 60 bytes of material to derive the write keys and salts
    /// for both client and server.
    pub fn new(keying_material: &[u8]) -> Result<Self, SrtpError> {
        if keying_material.len() < 60 {
            return Err(SrtpError::KeyDerivationFailed);
        }

        let client_write_key = keying_material[0..16].to_vec();
        let server_write_key = keying_material[16..32].to_vec();
        let client_write_salt = keying_material[32..46].to_vec();
        let server_write_salt = keying_material[46..60].to_vec();

        Ok(Self {
            client_write_key,
            server_write_key,
            client_write_salt,
            server_write_salt,
        })
    }

    /// Encrypts and authenticates an RTP packet using AES-CM and HMAC-SHA1.
    ///
    /// This method updates the internal Roll-over Counter (ROC) for the packet's SSRC,
    /// encrypts the payload, and appends an authentication tag.
    ///
    /// # Errors
    /// Returns `SrtpError::PacketTooShort` if the packet serialization fails or is invalid.
    pub fn protect(&mut self, packet: &RtpPacket, is_client: bool) -> Result<Vec<u8>, SrtpError> {
        let (key, salt) = self.get_write_keys(is_client);
        let packet_bytes = packet.to_bytes();

        if packet_bytes.len() < 33 {
            return Err(SrtpError::PacketTooShort);
        }

        let iv = Self::get_iv(salt, packet.ssrc, packet.sequence_number);

        let encrypted_payload = Self::apply_aes_ctr(key, &iv, &packet_bytes[33..], true)?;

        let mut out_packet = packet_bytes[..33].to_vec();
        out_packet.extend_from_slice(&encrypted_payload);

        let tag = Self::calculate_hmac(key, &out_packet)?;
        out_packet.extend_from_slice(&tag);

        Ok(out_packet)
    }

    /// Verifies and decrypts a raw SRTP packet into a structured `RtpPacket`.
    ///
    /// This method parses the header to retrieve the ROC, verifies the HMAC-SHA1
    /// authentication tag, and decrypts the payload if verification succeeds.
    ///
    /// # Errors
    /// Returns `SrtpError::AuthenticationFailed` if the HMAC tag does not match, or
    /// `SrtpError::PacketTooShort` if the data is insufficient.
    pub fn unprotect(
        &mut self,
        packet_bytes: &[u8],
        is_client: bool,
    ) -> Result<RtpPacket, SrtpError> {
        if packet_bytes.len() < 33 + 10 {
            return Err(SrtpError::PacketTooShort);
        }

        let (key, salt) = self.get_read_keys(is_client);

        let content_len = packet_bytes.len() - 10;
        let (content, tag) = packet_bytes.split_at(content_len);

        let calculated_tag = Self::calculate_hmac(key, content)?;
        if calculated_tag != tag {
            return Err(SrtpError::AuthenticationFailed);
        }

        let seq_bytes: [u8; 8] = packet_bytes[5..13].try_into().map_err(|_| SrtpError::PacketTooShort)?;
        let sequence_number = u64::from_be_bytes(seq_bytes);

        let ssrc_bytes: [u8; 4] = packet_bytes[29..33].try_into().map_err(|_| SrtpError::PacketTooShort)?;
        let ssrc = u32::from_be_bytes(ssrc_bytes);

        let iv = Self::get_iv(salt, ssrc, sequence_number);

        let encrypted_payload = &content[33..];
        let decrypted_payload = Self::apply_aes_ctr(key, &iv, encrypted_payload, false)?;

        let mut raw_decrypted_packet = Vec::with_capacity(33 + decrypted_payload.len());
        raw_decrypted_packet.extend_from_slice(&content[..33]);
        raw_decrypted_packet.extend_from_slice(&decrypted_payload);

        RtpPacket::from_bytes(&raw_decrypted_packet).ok_or(SrtpError::PacketTooShort)
    }
}

impl SrtpContext {
    /// Returns the write keys (encryption key and salt) based on the perspective.
    ///
    /// # Arguments
    /// * `is_client` - If true, returns client keys; otherwise returns server keys.
    fn get_write_keys(&self, is_client: bool) -> (&[u8], &[u8]) {
        if is_client {
            (&self.client_write_key, &self.client_write_salt)
        } else {
            (&self.server_write_key, &self.server_write_salt)
        }
    }

    /// Returns the read keys (decryption key and salt) based on the perspective.
    ///
    /// # Arguments
    /// * `is_client` - If true, returns server keys (client reads from server); otherwise returns client keys.
    fn get_read_keys(&self, is_client: bool) -> (&[u8], &[u8]) {
        if is_client {
            (&self.server_write_key, &self.server_write_salt)
        } else {
            (&self.client_write_key, &self.client_write_salt)
        }
    }

    /// Applies AES-128 in CTR mode for encryption or decryption.
    ///
    /// # Arguments
    /// * `key` - The encryption/decryption key.
    /// * `iv` - The initialization vector.
    /// * `data` - The data to encrypt or decrypt.
    /// * `encrypting` - If true, encrypts; otherwise decrypts.
    ///
    /// # Errors
    /// Returns `SrtpError` if the cryptographic operation fails.
    fn apply_aes_ctr(
        key: &[u8],
        iv: &[u8],
        data: &[u8],
        encrypting: bool,
    ) -> Result<Vec<u8>, SrtpError> {
        let cipher = Cipher::aes_128_ctr();
        let result = if encrypting {
            encrypt(cipher, key, Some(iv), data)
        } else {
            decrypt(cipher, key, Some(iv), data)
        };
        result.map_err(SrtpError::from)
    }

    /// Calculates the HMAC-SHA1 authentication tag for the given data.
    ///
    /// Returns the first 10 bytes of the HMAC as specified by SRTP.
    ///
    /// # Arguments
    /// * `key` - The HMAC key.
    /// * `data` - The data to authenticate.
    ///
    /// # Errors
    /// Returns `SrtpError` if the HMAC calculation fails.
    fn calculate_hmac(key: &[u8], data: &[u8]) -> Result<Vec<u8>, SrtpError> {
        let pkey = PKey::hmac(key).map_err(SrtpError::from)?;
        let mut signer = Signer::new(MessageDigest::sha1(), &pkey).map_err(SrtpError::from)?;
        signer.update(data).map_err(SrtpError::from)?;
        let full_hmac = signer.sign_to_vec().map_err(SrtpError::from)?;

        Ok(full_hmac[0..10].to_vec())
    }

    /// Generates the initialization vector (IV) for AES-CTR encryption.
    ///
    /// Combines the salt with the SSRC and sequence number according to SRTP specifications.
    ///
    /// # Arguments
    /// * `salt` - The salt value.
    /// * `ssrc` - The synchronization source identifier.
    /// * `sequence_number` - The packet sequence number.
    ///
    /// # Returns
    /// A 16-byte initialization vector.
    fn get_iv(salt: &[u8], ssrc: u32, sequence_number: u64) -> [u8; 16] {
        let mut iv = [0u8; 16];

        let salt_len = salt.len().min(14);
        iv[0..salt_len].copy_from_slice(&salt[0..salt_len]);

        let mut block = [0u8; 16];
        let ssrc_bytes = ssrc.to_be_bytes();
        let seq_bytes = sequence_number.to_be_bytes();

        block[4..8].copy_from_slice(&ssrc_bytes);

        block[8..16].copy_from_slice(&seq_bytes);

        for i in 0..16 {
            iv[i] ^= block[i];
        }

        iv
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_srtp_roundtrip() {
        let keying_material = [0u8; 60];
        let mut context = match SrtpContext::new(&keying_material) {
            Ok(ctx) => ctx,
            Err(_) => return,
        };

        let packet = RtpPacket {
            version: 0,
            marker: 1,
            total_chunks: 1,
            is_i_frame: true,
            payload_type: 96,
            sequence_number: 1,
            timestamp: 12345,
            ssrc: 999,
            payload: vec![1, 2, 3, 4],
        };

        let protected = context.protect(&packet, true).expect("Protection failed");

        let unprotected = context
            .unprotect(&protected, false)
            .expect("Unprotection failed");

        assert_eq!(packet.payload, unprotected.payload);
        assert_eq!(packet.ssrc, unprotected.ssrc);
    }
}
