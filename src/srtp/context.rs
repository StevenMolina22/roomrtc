use crate::rtp::RtpPacket;
use super::error::SrtpError;
use openssl::symm::{encrypt, decrypt, Cipher};
use openssl::hash::MessageDigest;
use openssl::pkey::PKey;
use openssl::sign::Signer;
use std::collections::HashMap;

pub struct SrtpContext {
    client_write_key: Vec<u8>,
    server_write_key: Vec<u8>,
    client_write_salt: Vec<u8>,
    server_write_salt: Vec<u8>,
    roc_map: HashMap<u32, u32>,
}

impl SrtpContext {
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
        })
    }

    fn get_iv(salt: &[u8], ssrc: u32, roc: u32, seq_num: u16) -> [u8; 16] {
        let mut iv = [0u8; 16];
        // Pad salt to 16 bytes (it is 14 bytes)
        iv[0..14].copy_from_slice(salt);

        let mut block = [0u8; 16];
        let ssrc_bytes = ssrc.to_be_bytes();
        let roc_bytes = roc.to_be_bytes();
        let seq_bytes = seq_num.to_be_bytes();

        // (SSRC << 64) | (ROC << 16) | SEQ
        // Bytes 4-7: SSRC
        block[4..8].copy_from_slice(&ssrc_bytes);
        // Bytes 10-13: ROC
        block[10..14].copy_from_slice(&roc_bytes);
        // Bytes 14-15: SEQ
        block[14..16].copy_from_slice(&seq_bytes);

        // XOR with salt
        for i in 0..16 {
            iv[i] ^= block[i];
        }

        iv
    }

    pub fn protect(&mut self, packet: &RtpPacket, is_client: bool) -> Result<Vec<u8>, SrtpError> {
        let (key, salt) = if is_client {
            (&self.client_write_key, &self.client_write_salt)
        } else {
            (&self.server_write_key, &self.server_write_salt)
        };

        let seq_num = packet.chunk_id as u16;
        let roc = packet.frame_id as u32;
        self.roc_map.insert(packet.ssrc, roc);

        let iv = Self::get_iv(salt, packet.ssrc, roc, seq_num);

        let mut packet_bytes = packet.to_bytes();
        let header_len = 28;
        if packet_bytes.len() < header_len {
             return Err(SrtpError::PacketTooShort);
        }

        let payload = &packet_bytes[header_len..];
        let encrypted_payload = encrypt(
            Cipher::aes_128_ctr(),
            key,
            Some(&iv),
            payload
        ).map_err(SrtpError::from)?;

        // Replace payload in packet_bytes with encrypted payload
        packet_bytes.truncate(header_len);
        packet_bytes.extend_from_slice(&encrypted_payload);

        // Authenticate
        // HMAC over (Header + Encrypted Payload + ROC)
        let mut auth_input = packet_bytes.clone();
        auth_input.extend_from_slice(&roc.to_be_bytes());

        let pkey = PKey::hmac(key).map_err(SrtpError::from)?;
        let mut signer = Signer::new(MessageDigest::sha1(), &pkey).map_err(SrtpError::from)?;
        signer.update(&auth_input).map_err(SrtpError::from)?;
        let hmac = signer.sign_to_vec().map_err(SrtpError::from)?;

        // Append first 10 bytes
        packet_bytes.extend_from_slice(&hmac[0..10]);

        Ok(packet_bytes)
    }

    pub fn unprotect(&mut self, packet_bytes: &[u8], is_client: bool) -> Result<RtpPacket, SrtpError> {
        if packet_bytes.len() < 28 + 10 {
            return Err(SrtpError::PacketTooShort);
        }

        // Separate content and tag
        let content_len = packet_bytes.len() - 10;
        let content = &packet_bytes[..content_len];
        let tag = &packet_bytes[content_len..];

        // Parse header to get SSRC, ROC (frame_id), SEQ (chunk_id)
        let mut temp_packet = RtpPacket::from_bytes(content).ok_or(SrtpError::PacketTooShort)?;

        let seq_num = temp_packet.chunk_id as u16;
        let roc = temp_packet.frame_id as u32;

        let (key, salt) = if is_client {
            (&self.server_write_key, &self.server_write_salt)
        } else {
             (&self.client_write_key, &self.client_write_salt)
        };

        // Verify HMAC
        let mut auth_input = content.to_vec();
        auth_input.extend_from_slice(&roc.to_be_bytes());

        let pkey = PKey::hmac(key).map_err(SrtpError::from)?;
        let mut signer = Signer::new(MessageDigest::sha1(), &pkey).map_err(SrtpError::from)?;
        signer.update(&auth_input).map_err(SrtpError::from)?;
        let hmac = signer.sign_to_vec().map_err(SrtpError::from)?;

        if &hmac[0..10] != tag {
            return Err(SrtpError::AuthenticationFailed);
        }

        // Decrypt
        let iv = Self::get_iv(salt, temp_packet.ssrc, roc, seq_num);
        let encrypted_payload = &temp_packet.payload;

        let decrypted_payload = decrypt(
            Cipher::aes_128_ctr(),
            key,
            Some(&iv),
            encrypted_payload
        ).map_err(SrtpError::from)?;

        temp_packet.payload = decrypted_payload;

        Ok(temp_packet)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_srtp_roundtrip() {
        let keying_material = [0u8; 60];
        let mut context = SrtpContext::new(&keying_material).unwrap();

        let packet = RtpPacket::new(
            1, // version
            0, // marker
            96, // payload_type
            vec![1, 2, 3, 4], // payload
            12345, // timestamp
            100, // frame_id
            1, // chunk_id
            999, // ssrc
        );

        // Client protects
        let protected = context.protect(&packet, true).expect("Protection failed");

        // Server unprotects (simulated by using same context but is_client=false)
        // Wait, if I use same context and is_client=false, it will use client_write_key to unprotect.
        // Since client protected with client_write_key, server should unprotect with client_write_key.
        // My logic:
        // protect(is_client=true) -> uses client_write_key.
        // unprotect(is_client=false) -> uses client_write_key.
        // So this is correct for a loopback test with same context.

        let unprotected = context.unprotect(&protected, false).expect("Unprotection failed");

        assert_eq!(packet.payload, unprotected.payload);
        assert_eq!(packet.ssrc, unprotected.ssrc);
    }
}
