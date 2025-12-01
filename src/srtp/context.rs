use super::error::SrtpError;
use crate::transport::rtp::RtpPacket;
use openssl::hash::MessageDigest;
use openssl::pkey::PKey;
use openssl::sign::Signer;
use openssl::symm::{Cipher, decrypt, encrypt};
use std::collections::HashMap;

/// SRTP context for encrypting and authenticating RTP/RTCP packets.
///
/// Manages encryption keys, salts, and sequence counters for secure RTP communication.
/// Supports both client and server perspectives with automatic key selection.
pub struct SrtpContext {
    client_write_key: Vec<u8>,
    server_write_key: Vec<u8>,
    client_write_salt: Vec<u8>,
    server_write_salt: Vec<u8>,
    /// Roll-over counter tracking per SSRC for RTP packets
    roc_map: HashMap<u32, u32>,
    /// Index counter tracking per SSRC for RTCP packets
    srtcp_idx_map: HashMap<u32, u32>,
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
            roc_map: HashMap::new(),
            srtcp_idx_map: HashMap::new(),
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
        let seq_num = packet.chunk_id as u16;
        let roc = packet.frame_id as u32;

        self.roc_map.insert(packet.ssrc, roc);

        let (key, salt) = self.get_write_keys(is_client);
        let packet_bytes = packet.to_bytes();

        if packet_bytes.len() < 28 {
            return Err(SrtpError::PacketTooShort);
        }

        let iv = Self::get_iv(salt, packet.ssrc, roc, seq_num);

        let encrypted_payload = Self::apply_aes_ctr(key, &iv, &packet_bytes[28..], true)?;

        let mut out_packet = packet_bytes[..28].to_vec();
        out_packet.extend_from_slice(&encrypted_payload);

        let tag = self.calculate_srtp_tag(key, &out_packet, roc)?;
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
        if packet_bytes.len() < 38 {
            return Err(SrtpError::PacketTooShort);
        }
        let content_len = packet_bytes.len() - 10;
        let (content, tag) = packet_bytes.split_at(content_len);

        let mut temp_packet = RtpPacket::from_bytes(content).ok_or(SrtpError::PacketTooShort)?;
        let (key, salt) = self.get_read_keys(is_client);
        let roc = temp_packet.frame_id as u32;

        self.verify_srtp_tag(key, content, roc, tag)?;

        let iv = Self::get_iv(salt, temp_packet.ssrc, roc, temp_packet.chunk_id as u16);
        let decrypted = Self::apply_aes_ctr(key, &iv, &temp_packet.payload, false)?;

        temp_packet.payload = decrypted;
        Ok(temp_packet)
    }

    /// Encrypts and authenticates a raw RTCP packet (SRTCP).
    ///
    /// This method automatically manages the SRTCP index, appends it with the
    /// Encryption flag (E-bit) set, and adds the authentication tag.
    ///
    /// # Errors
    /// Returns `SrtpError::KeyDerivationFailed` if the SRTCP index is exhausted,
    /// or `SrtpError::PacketTooShort` if the input is not a valid RTCP packet.
    pub fn protect_rtcp(
        &mut self,
        packet_data: &[u8],
        ssrc: u32,
        is_client: bool,
    ) -> Result<Vec<u8>, SrtpError> {
        let index = self.next_rtcp_index(ssrc)?;

        let (key, salt) = self.get_write_keys(is_client);

        if packet_data.len() < 8 {
            return Err(SrtpError::PacketTooShort);
        }

        let iv = Self::get_srtcp_iv(salt, ssrc, index);
        let encrypted_payload = Self::apply_aes_ctr(key, &iv, &packet_data[8..], true)?;

        let mut out_packet = packet_data[..8].to_vec();
        out_packet.extend_from_slice(&encrypted_payload);
        out_packet.extend_from_slice(&(index | 0x8000_0000).to_be_bytes());

        let tag = Self::calculate_hmac(key, &out_packet)?;
        out_packet.extend_from_slice(&tag);

        Ok(out_packet)
    }

    /// Verifies and decrypts a raw SRTCP packet.
    ///
    /// This method extracts the SRTCP index and authentication tag from the footer,
    /// verifies the packet integrity, and decrypts the payload.
    ///
    /// # Errors
    /// Returns `SrtpError::AuthenticationFailed` if the integrity check fails,
    /// or `SrtpError::PacketTooShort` if the packet is malformed.
    pub fn unprotect_rtcp(
        &mut self,
        packet_bytes: &[u8],
        is_client: bool,
    ) -> Result<Vec<u8>, SrtpError> {
        if packet_bytes.len() < 22 {
            return Err(SrtpError::PacketTooShort);
        }

        let (key, salt) = self.get_read_keys(is_client);

        let split_at_tag = packet_bytes.len() - 10;
        let (auth_input, tag) = packet_bytes.split_at(split_at_tag);

        if Self::calculate_hmac(key, auth_input)? != tag {
            return Err(SrtpError::AuthenticationFailed);
        }

        let split_at_index = auth_input.len() - 4;
        let (content, index_bytes) = auth_input.split_at(split_at_index);

        let index_val = u32::from_be_bytes(
            index_bytes
                .try_into()
                .map_err(|_| SrtpError::PacketTooShort)?,
        );
        let srtcp_index = index_val & 0x7FFF_FFFF;

        let ssrc_bytes = packet_bytes.get(4..8).ok_or(SrtpError::PacketTooShort)?;
        let ssrc = u32::from_be_bytes(ssrc_bytes.try_into().unwrap());

        let iv = Self::get_srtcp_iv(salt, ssrc, srtcp_index);
        let decrypted_payload = Self::apply_aes_ctr(key, &iv, &content[8..], false)?;

        let mut out_packet = content[..8].to_vec();
        out_packet.extend_from_slice(&decrypted_payload);

        Ok(out_packet)
    }
}

/// Private helpers for SRTP / SRTCP context
impl SrtpContext {
    /// Helper to select keys for writing (Sender logic)
    fn get_write_keys(&self, is_client: bool) -> (&[u8], &[u8]) {
        if is_client {
            (&self.client_write_key, &self.client_write_salt)
        } else {
            (&self.server_write_key, &self.server_write_salt)
        }
    }

    /// Helper to select keys for reading (Receiver logic - swaps roles)
    fn get_read_keys(&self, is_client: bool) -> (&[u8], &[u8]) {
        if is_client {
            (&self.server_write_key, &self.server_write_salt)
        } else {
            (&self.client_write_key, &self.client_write_salt)
        }
    }

    /// Wrapper for OpenSSL AES-128-CTR to reduce verbosity
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

    /// Generates the HMAC-SHA1 signature and truncates to 10 bytes
    fn calculate_hmac(key: &[u8], data: &[u8]) -> Result<Vec<u8>, SrtpError> {
        let pkey = PKey::hmac(key).map_err(SrtpError::from)?;
        let mut signer = Signer::new(MessageDigest::sha1(), &pkey).map_err(SrtpError::from)?;
        signer.update(data).map_err(SrtpError::from)?;
        let full_hmac = signer.sign_to_vec().map_err(SrtpError::from)?;

        Ok(full_hmac[0..10].to_vec())
    }

    /// Generates the 128-bit Initialization Vector (IV) for AES-CTR.
    ///
    /// Computed by `XORing` the session salt with the SSRC, ROC, and sequence number
    /// as specified in RFC 3711.
    fn get_iv(salt: &[u8], ssrc: u32, roc: u32, seq_num: u16) -> [u8; 16] {
        let mut iv = [0u8; 16];
        // Pad salt to 16 bytes (it is 14 bytes)
        iv[0..14].copy_from_slice(salt);

        let mut block = [0u8; 16];
        let ssrc_bytes = ssrc.to_be_bytes();
        let roc_bytes = roc.to_be_bytes();
        let seq_bytes = seq_num.to_be_bytes();

        block[4..8].copy_from_slice(&ssrc_bytes);
        block[10..14].copy_from_slice(&roc_bytes);
        block[14..16].copy_from_slice(&seq_bytes);

        for i in 0..16 {
            iv[i] ^= block[i];
        }

        iv
    }

    /// Specific logic for SRTP Tag calculation: Content + ROC
    fn calculate_srtp_tag(
        &self,
        key: &[u8],
        content: &[u8],
        roc: u32,
    ) -> Result<Vec<u8>, SrtpError> {
        let mut auth_input = content.to_vec();
        auth_input.extend_from_slice(&roc.to_be_bytes());
        Self::calculate_hmac(key, &auth_input)
    }

    /// Specific logic for SRTP Tag verification
    fn verify_srtp_tag(
        &self,
        key: &[u8],
        content: &[u8],
        roc: u32,
        expected_tag: &[u8],
    ) -> Result<(), SrtpError> {
        let calculated = self.calculate_srtp_tag(key, content, roc)?;
        if calculated != expected_tag {
            return Err(SrtpError::AuthenticationFailed);
        }
        Ok(())
    }

    /// Retrieves and increments the 31-bit SRTCP index for the given SSRC.
    /// Returns an error if the index exceeds `0x7FFF_FFFF`, indicating key exhaustion.
    fn next_rtcp_index(&mut self, ssrc: u32) -> Result<u32, SrtpError> {
        let index = self.srtcp_idx_map.entry(ssrc).or_insert(0);
        let current = *index;

        if current > 0x7FFF_FFFF {
            return Err(SrtpError::KeyDerivationFailed);
        }

        *index += 1;
        Ok(current)
    }

    /// Generates the Initialization Vector (IV) for SRTCP.
    /// Formula: (Salt) XOR ((SSRC << 64) | (`SRTCP_INDEX` << 16))
    fn get_srtcp_iv(salt: &[u8], ssrc: u32, index: u32) -> [u8; 16] {
        let mut iv = [0u8; 16];

        let salt_len = salt.len().min(14);
        iv[0..salt_len].copy_from_slice(&salt[0..salt_len]);

        let mut block = [0u8; 16];
        let ssrc_bytes = ssrc.to_be_bytes();
        let index_bytes = index.to_be_bytes();

        block[4..8].copy_from_slice(&ssrc_bytes);

        block[10..14].copy_from_slice(&index_bytes);

        // 3. XOR the salt with the block
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
        let mut context = SrtpContext::new(&keying_material).unwrap();

        let packet = RtpPacket::new(1, 0, 96, vec![1, 2, 3, 4], 12345, 100, 1, 999);

        let protected = context.protect(&packet, true).expect("Protection failed");

        let unprotected = context
            .unprotect(&protected, false)
            .expect("Unprotection failed");

        assert_eq!(packet.payload, unprotected.payload);
        assert_eq!(packet.ssrc, unprotected.ssrc);
    }

    #[test]
    fn test_srtcp_protection_roundtrip() {
        let key_material = [0u8; 60];
        let mut client_ctx = SrtpContext::new(&key_material).unwrap();
        let mut server_ctx = SrtpContext::new(&key_material).unwrap();

        let ssrc = 0x12345678;
        let packet = crate::transport::rtcp::RtcpPacket::ConnectivityReport(ssrc);
        let raw_bytes = packet.to_bytes();

        let protected = client_ctx
            .protect_rtcp(&raw_bytes, ssrc, true)
            .expect("SRTCP protection failed");

        assert_eq!(protected.len(), raw_bytes.len() + 4 + 10);

        let unprotected = server_ctx
            .unprotect_rtcp(&protected, false)
            .expect("SRTCP unprotection failed");

        assert_eq!(
            unprotected, raw_bytes,
            "Decrypted data does not match original"
        );
    }
}
