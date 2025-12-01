use super::error::SrtpError;
use crate::rtp::RtpPacket;
use openssl::hash::MessageDigest;
use openssl::pkey::PKey;
use openssl::sign::Signer;
use openssl::symm::{Cipher, decrypt, encrypt};
use std::collections::HashMap;

pub struct SrtpContext {
    client_write_key: Vec<u8>,
    server_write_key: Vec<u8>,
    client_write_salt: Vec<u8>,
    server_write_salt: Vec<u8>,
    roc_map: HashMap<u32, u32>,
    srtcp_idx_map: HashMap<u32, u32>,
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
            srtcp_idx_map: HashMap::new(),
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

        // XOR with salt - IV Calculation (Salt + Counter) for Integer Counter Mode
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
        let encrypted_payload =
            encrypt(Cipher::aes_128_ctr(), key, Some(&iv), payload).map_err(SrtpError::from)?;

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

    pub fn unprotect(
        &mut self,
        packet_bytes: &[u8],
        is_client: bool,
    ) -> Result<RtpPacket, SrtpError> {
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

        let decrypted_payload = decrypt(Cipher::aes_128_ctr(), key, Some(&iv), encrypted_payload)
            .map_err(SrtpError::from)?;

        temp_packet.payload = decrypted_payload;

        Ok(temp_packet)
    }

    /// Generates the Initialization Vector (IV) for SRTCP.
    /// Formula: (Salt) XOR ((SSRC << 64) | (SRTCP_INDEX << 16))
    fn get_srtcp_iv(salt: &[u8], ssrc: u32, index: u32) -> [u8; 16] {
        let mut iv = [0u8; 16];

        // 1. Copy the 14-byte salt into the IV container
        // Note: Check your salt length. Standard is 14 bytes.
        let salt_len = salt.len().min(14);
        iv[0..salt_len].copy_from_slice(&salt[0..salt_len]);

        let mut block = [0u8; 16];
        let ssrc_bytes = ssrc.to_be_bytes();
        let index_bytes = index.to_be_bytes();

        // 2. Construct the XOR block based on RFC 3711:
        // Position SSRC at bytes 4..8 (bits 32-63)
        block[4..8].copy_from_slice(&ssrc_bytes);

        // Position SRTCP Index at bytes 10..14 (bits 80-111)
        // This corresponds to shifting the index left by 16 bits.
        block[10..14].copy_from_slice(&index_bytes);

        // 3. XOR the salt with the block
        for i in 0..16 {
            iv[i] ^= block[i];
        }

        iv
    }

    pub fn protect_rtcp(
        &mut self,
        packet_data: &[u8],
        ssrc: u32,
        is_client: bool,
    ) -> Result<Vec<u8>, SrtpError> {
        // 1. Get Keys (same as SRTP)
        let (key, salt) = if is_client {
            (&self.client_write_key, &self.client_write_salt)
        } else {
            (&self.server_write_key, &self.server_write_salt)
        };

        // 2. Get and Increment SRTCP Index
        let index = self.srtcp_idx_map.entry(ssrc).or_insert(0);
        let current_index = *index;
        *index += 1;

        // Check for wrapping (SRTCP index is 31 bits)
        if current_index > 0x7FFFFFFF {
            return Err(SrtpError::KeyDerivationFailed);
        }

        // 3. Encrypt Payload
        // RTCP Header is first 8 bytes. Everything after is payload.
        // We encrypt the whole payload, not just video data.
        let header_len = 8;
        if packet_data.len() < header_len {
            return Err(SrtpError::PacketTooShort);
        }

        let payload = &packet_data[header_len..];

        // IV Generation for SRTCP: (SSRC << 64) | (Index << 16)
        let iv = Self::get_srtcp_iv(salt, ssrc, current_index);

        let encrypted_payload =
            encrypt(Cipher::aes_128_ctr(), key, Some(&iv), payload).map_err(SrtpError::from)?;

        // 4. Construct New Packet
        let mut out_packet = packet_data[..header_len].to_vec();
        out_packet.extend_from_slice(&encrypted_payload);

        // 5. Append SRTCP Index (with E-bit set to 1 usually)
        // Bit 31 is E-bit. 1 = Encrypted.
        let index_with_e_bit = current_index | 0x80000000;
        out_packet.extend_from_slice(&index_with_e_bit.to_be_bytes());

        // 6. Authenticate (HMAC)
        // Input is the whole current packet (Header + Encrypted Payload + Index)
        let pkey = PKey::hmac(key).map_err(SrtpError::from)?;
        let mut signer = Signer::new(MessageDigest::sha1(), &pkey).map_err(SrtpError::from)?;
        signer.update(&out_packet).map_err(SrtpError::from)?;
        let hmac = signer.sign_to_vec().map_err(SrtpError::from)?;

        // 7. Append Tag
        out_packet.extend_from_slice(&hmac[0..10]);

        Ok(out_packet)
    }

    pub fn unprotect_rtcp(
        &mut self,
        packet_bytes: &[u8],
        is_client: bool,
    ) -> Result<Vec<u8>, SrtpError> {
        // 8 (Header) + 4 (Index) + 10 (Tag) = 22 bytes min
        if packet_bytes.len() < 22 {
            return Err(SrtpError::PacketTooShort);
        }

        // 1. Separate Tag, Index, and Content
        let total_len = packet_bytes.len();
        let tag_len = 10;
        let index_len = 4;

        let content_len = total_len - tag_len - index_len;
        let content_and_index_len = total_len - tag_len;

        let content_and_index = &packet_bytes[..content_and_index_len];
        let tag = &packet_bytes[content_and_index_len..];

        // Extract Index bytes (last 4 bytes before tag)
        let index_bytes = &packet_bytes[content_len..content_and_index_len];
        let index_val = u32::from_be_bytes(
            index_bytes
                .try_into()
                .map_err(|_| SrtpError::PacketTooShort)?,
        );
        // Check E-bit (MSB)
        let is_encrypted = (index_val & 0x80000000) != 0;
        let srtcp_index = index_val & 0x7FFFFFFF;

        // 2. Get Keys (Swap client/server logic compared to protect)
        let (key, salt) = if is_client {
            (&self.server_write_key, &self.server_write_salt)
        } else {
            (&self.client_write_key, &self.client_write_salt)
        };

        // 3. Verify HMAC
        // HMAC covers everything up to the tag
        let pkey = PKey::hmac(key).map_err(SrtpError::from)?;
        let mut signer = Signer::new(MessageDigest::sha1(), &pkey).map_err(SrtpError::from)?;
        signer.update(content_and_index).map_err(SrtpError::from)?;
        let hmac = signer.sign_to_vec().map_err(SrtpError::from)?;

        if &hmac[0..10] != tag {
            return Err(SrtpError::AuthenticationFailed);
        }

        // 4. Decrypt (if E-bit was set)
        let mut out_packet = packet_bytes[..content_len].to_vec();

        if is_encrypted {
            // Parse Header to get SSRC (Bytes 4-7)
            // Note: You need to parse the SSRC from the packet bytes manually here
            if out_packet.len() < 8 {
                return Err(SrtpError::PacketTooShort);
            }
            let ssrc =
                u32::from_be_bytes([out_packet[4], out_packet[5], out_packet[6], out_packet[7]]);

            let iv = Self::get_srtcp_iv(salt, ssrc, srtcp_index);
            let header_len = 8;
            let encrypted_payload = &out_packet[header_len..];

            let decrypted_payload =
                decrypt(Cipher::aes_128_ctr(), key, Some(&iv), encrypted_payload)
                    .map_err(SrtpError::from)?;

            // Reconstruct: Header + Decrypted Payload
            out_packet.truncate(header_len);
            out_packet.extend_from_slice(&decrypted_payload);
        }

        // 5. Update Replay Protection (Optional but recommended)
        // You should track the highest received index per SSRC to prevent replay attacks.

        Ok(out_packet)
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
            1,                // version
            0,                // marker
            96,               // payload_type
            vec![1, 2, 3, 4], // payload
            12345,            // timestamp
            100,              // frame_id
            1,                // chunk_id
            999,              // ssrc
        );

        // Client protects
        let protected = context.protect(&packet, true).expect("Protection failed");

        // Verify key symmetry (Loopback Test):
        // The unprotect function, when called by the Receiver (is_client=false),
        // must automatically select the key associated with the Sender (client_write_key).
        // This confirms the key assignment logic is correctly swapped.
        let unprotected = context
            .unprotect(&protected, false)
            .expect("Unprotection failed");

        assert_eq!(packet.payload, unprotected.payload);
        assert_eq!(packet.ssrc, unprotected.ssrc);
    }

    #[test]
    fn test_srtcp_protection_roundtrip() {
        // Initialize shared cryptographic context.
        // Both client and server derive keys from the same material.
        let key_material = [0u8; 60];
        let mut client_ctx = SrtpContext::new(&key_material).unwrap();
        let mut server_ctx = SrtpContext::new(&key_material).unwrap();

        // Construct a valid RTCP packet with a specific SSRC.
        let ssrc = 0x12345678;
        let packet = crate::rtcp::RtcpPacket::ConnectivityReport(ssrc);
        let raw_bytes = packet.as_bytes();

        // Client protects (encrypts and authenticates) the packet.
        let protected = client_ctx
            .protect_rtcp(&raw_bytes, ssrc, true)
            .expect("SRTCP protection failed");

        // Verify SRTCP packet expansion.
        // Expected size: Original Length + SRTCP Index (4 bytes) + HMAC Tag (10 bytes).
        assert_eq!(protected.len(), raw_bytes.len() + 4 + 10);

        // Server unprotects (authenticates and decrypts) the packet.
        // Note: `is_client` is set to false to ensure the server uses the correct key set.
        let unprotected = server_ctx
            .unprotect_rtcp(&protected, false)
            .expect("SRTCP unprotection failed");

        // Validate that the decrypted data matches the original packet exactly.
        assert_eq!(
            unprotected, raw_bytes,
            "Decrypted data does not match original"
        );
    }
}
