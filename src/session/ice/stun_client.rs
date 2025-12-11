use std::net::{ToSocketAddrs, UdpSocket};
use std::time::Duration;

const STUN_SERVER: &str = "stun.l.google.com:19302";

/// Sends a STUN Binding Request using the provided UDP socket and returns the
/// public address (IP:PORT) observed by the STUN server.
///
/// This function sets a short read timeout on the socket, sends up to three
/// requests to the STUN server defined by `STUN_SERVER`, and waits for a
/// response containing the `XOR-MAPPED-ADDRESS` attribute. On success it parses
/// the response and returns the public address as a `String` formatted as
/// `"<ip>:<port>"`.
///
/// # Arguments
///
/// - `socket`: a bound local UDP socket used to send the STUN request.
///
/// # Returns
///
/// - `Ok(String)`: the public address `"ip:port"` on success.
/// - `Err(String)`: an error string (e.g. timeouts, DNS errors, or send/recv
///   errors).
pub fn get_public_ip_and_port(
    socket: &UdpSocket,
    logger: &crate::logger::Logger,
) -> Result<String, String> {
    socket
        .set_read_timeout(Some(Duration::from_millis(1500)))
        .map_err(|e| e.to_string())?;

    let mut packet = vec![0x00, 0x01, 0x00, 0x00, 0x21, 0x12, 0xA4, 0x42];
    for _ in 0..12 {
        packet.push(rand::random::<u8>());
    }

    let remote_addr = STUN_SERVER
        .to_socket_addrs()
        .map_err(|e| e.to_string())?
        .find(std::net::SocketAddr::is_ipv4)
        .ok_or("DNS Error: No IPv4 address found for STUN server")?;

    logger.debug(&format!(
        "[STUN] Sending request to {} from local socket {:?}",
        remote_addr,
        socket.local_addr()
    ));

    for i in 1..=3 {
        if let Err(e) = socket.send_to(&packet, remote_addr) {
            logger.warn(&format!("[STUN] Attempt {i}: Error sending: {e}"));
            continue;
        }

        let mut buf = [0u8; 1024];
        match socket.recv_from(&mut buf) {
            Ok((amt, _src)) => {
                let _ = socket.set_read_timeout(None);
                return parse_stun_response(&buf[..amt]);
            }
            Err(e) => {
                logger.warn(&format!(
                    "[STUN] Attempt {i}: Timeout or error receiving: {e}"
                ));
            }
        }
    }

    let _ = socket.set_read_timeout(None);
    Err("STUN Timeout: No response received from server".to_string())
}

/// Parses a STUN response searching for the `XOR-MAPPED-ADDRESS` attribute and
/// decodes the public address.
///
/// Expects `data` to follow the STUN packet layout: a 20-byte header containing
/// the magic cookie `0x2112A442`, followed by TLV attributes. If it finds an
/// attribute with type `0x0020` (XOR-MAPPED-ADDRESS) and IPv4 family, it will
/// decode the port and IP applying the XOR mask with the magic cookie as
/// specified in RFC 5389.
///
/// # Arguments
///
/// - `data`: raw bytes of the received STUN response.
///
/// # Returns
///
/// - `Ok(String)`: public address in the format `"ip:port"` when the attribute
///   is found and decoded successfully.
/// - `Err(String)`: error message when the packet is invalid or the expected
///   attribute is not found.
fn parse_stun_response(data: &[u8]) -> Result<String, String> {
    if data.len() < 20 {
        return Err("Invalid STUN response (too short)".into());
    }

    if data[4..8] != [0x21, 0x12, 0xA4, 0x42] {
        return Err("Invalid STUN response: Magic Cookie mismatch".into());
    }

    let msg_len = u16::from_be_bytes([data[2], data[3]]) as usize;
    let mut pos = 20;

    while pos < 20 + msg_len && pos + 4 <= data.len() {
        let attr_type = u16::from_be_bytes([data[pos], data[pos + 1]]);
        let attr_len = u16::from_be_bytes([data[pos + 2], data[pos + 3]]) as usize;
        pos += 4;

        if attr_type == 0x0020 && pos + attr_len <= data.len() {
            let val = &data[pos..pos + attr_len];
            if val.len() >= 8 && val[1] == 0x01 {
                let port = u16::from_be_bytes([val[2] ^ 0x21, val[3] ^ 0x12]);

                let ip = format!(
                    "{}.{}.{}.{}",
                    val[4] ^ 0x21,
                    val[5] ^ 0x12,
                    val[6] ^ 0xA4,
                    val[7] ^ 0x42
                );

                return Ok(format!("{ip}:{port}"));
            }
        }

        let padding = (4 - (attr_len % 4)) % 4;
        pos += attr_len + padding;
    }

    Err("XOR-MAPPED-ADDRESS (public IP) attribute not found in response".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    const MAGIC_COOKIE: u32 = 0x2112A442;

    struct StunBuilder {
        attributes: Vec<u8>,
    }

    impl StunBuilder {
        fn new() -> Self {
            Self {
                attributes: Vec::new(),
            }
        }

        fn add_attribute(mut self, attr_type: u16, value: &[u8]) -> Self {
            self.attributes.extend_from_slice(&attr_type.to_be_bytes());
            self.attributes
                .extend_from_slice(&(value.len() as u16).to_be_bytes());
            self.attributes.extend_from_slice(value);

            let padding = (4 - (value.len() % 4)) % 4;
            for _ in 0..padding {
                self.attributes.push(0x00);
            }
            self
        }

        fn add_xor_address(self, ip: &str, port: u16) -> Self {
            let mut value = vec![0x00, 0x01]; // Reserved + Family (IPv4)

            let magic_high_16 = (MAGIC_COOKIE >> 16) as u16;
            let x_port = port ^ magic_high_16;
            value.extend_from_slice(&x_port.to_be_bytes());

            let ip_parts: Vec<u8> = ip.split('.').filter_map(|s| s.parse::<u8>().ok()).collect();

            // Pad with zeros if parsing failed or incomplete
            let mut ip_bytes = [0u8; 4];
            for (i, byte) in ip_parts.iter().take(4).enumerate() {
                ip_bytes[i] = *byte;
            }

            let ip_u32 = u32::from_be_bytes([ip_bytes[0], ip_bytes[1], ip_bytes[2], ip_bytes[3]]);
            let x_ip = ip_u32 ^ MAGIC_COOKIE;
            value.extend_from_slice(&x_ip.to_be_bytes());

            self.add_attribute(0x0020, &value)
        }

        fn build(self) -> Vec<u8> {
            let mut packet = vec![
                0x01, 0x01, // Type: Binding Response
                0x00, 0x00, // Length (placeholder)
                0x21, 0x12, 0xA4, 0x42, // Magic Cookie
            ];
            packet.extend_from_slice(&[0xAA; 12]);

            packet.extend_from_slice(&self.attributes);

            let len = (packet.len() - 20) as u16;
            packet[2] = (len >> 8) as u8;
            packet[3] = (len & 0xFF) as u8;

            packet
        }
    }

    fn build_fake_stun_response(public_ip: &str, public_port: u16) -> Vec<u8> {
        let magic_cookie: u32 = 0x2112A442;

        let mut packet = vec![
            0x01, 0x01, // Type: Binding Success Response (0x0101)
            0x00, 0x0C, // Length: 12 bytes de payload (Attribute header 4 + body 8)
            0x21, 0x12, 0xA4, 0x42, // Magic Cookie
        ];
        packet.extend_from_slice(&[0u8; 12]);

        packet.extend_from_slice(&0x0020u16.to_be_bytes()); // Attribute Type
        packet.extend_from_slice(&0x0008u16.to_be_bytes()); // Attribute Length (8 bytes)

        packet.push(0x00); // Reserved (1 byte)
        packet.push(0x01); // Family: IPv4 (0x01)

        let magic_high_16 = (magic_cookie >> 16) as u16;
        let x_port = public_port ^ magic_high_16;
        packet.extend_from_slice(&x_port.to_be_bytes());

        let ip_parts: Vec<u8> = public_ip
            .split('.')
            .filter_map(|s| s.parse::<u8>().ok())
            .collect();

        let mut ip_bytes = [0u8; 4];
        for (i, byte) in ip_parts.iter().take(4).enumerate() {
            ip_bytes[i] = *byte;
        }

        let ip_u32 = u32::from_be_bytes([ip_bytes[0], ip_bytes[1], ip_bytes[2], ip_bytes[3]]);
        let x_ip = ip_u32 ^ magic_cookie;
        packet.extend_from_slice(&x_ip.to_be_bytes());

        packet
    }

    #[test]
    fn test_parse_stun_response_correctly_decodes_xor_mapped_address() {
        let expected_ip = "200.100.50.25";
        let expected_port = 8888;

        let packet = build_fake_stun_response(expected_ip, expected_port);

        let result = parse_stun_response(&packet);

        assert!(result.is_ok(), "El parser falló al leer un paquete válido");
        assert_eq!(
            result.expect("failed to result"),
            format!("{expected_ip}:{expected_port}")
        );
    }

    #[test]
    fn test_parse_stun_response_invalid_cookie() {
        let mut packet = build_fake_stun_response("1.2.3.4", 5555);
        packet[4] = 0x00;

        let result = parse_stun_response(&packet);
        assert!(result.is_err());
        assert_eq!(
            result.expect_err("expected error"),
            "Invalid STUN response: Magic Cookie mismatch"
        );
    }

    #[test]
    fn test_parse_stun_response_too_short() {
        let packet = vec![0x00, 0x01]; // Incomplet packet
        let result = parse_stun_response(&packet);
        assert!(result.is_err());
    }

    #[test]
    fn test_robustness_attributes_before_target() {
        let packet = StunBuilder::new()
            .add_attribute(0x8022, b"STUN Server v1.0")
            .add_xor_address("192.168.1.50", 12345)
            .build();

        let result = parse_stun_response(&packet);
        assert_eq!(
            result.expect("failed to parse STUN response"),
            "192.168.1.50:12345"
        );
    }

    #[test]
    fn test_robustness_padding_calculation() {
        let packet = StunBuilder::new()
            .add_attribute(0x0001, &[0x01, 0x02, 0x03, 0x04, 0x05]) // 5 bytes
            .add_xor_address("10.0.0.1", 8080)
            .build();

        let result = parse_stun_response(&packet);
        assert_eq!(
            result.expect("failed to parse STUN response"),
            "10.0.0.1:8080"
        );
    }

    #[test]
    fn test_robustness_boundary_values() {
        let packet = StunBuilder::new()
            .add_xor_address("255.255.255.255", 65535)
            .build();

        let result = parse_stun_response(&packet);
        assert_eq!(
            result.expect("failed to parse STUN response"),
            "255.255.255.255:65535"
        );
    }

    #[test]
    fn test_robustness_ignores_ipv6() {
        let mut value = vec![0x00, 0x02]; // Family IPv6
        value.extend_from_slice(&[0xFF; 18]); // Datos IPv6 fake

        let packet = StunBuilder::new()
            .add_attribute(0x0020, &value) // Attribute XOR-MAPPED-ADDRESS but with IPv6
            .build();

        let result = parse_stun_response(&packet);
        assert!(result.is_err());
        assert!(result.expect_err("expected error").contains("not found"));
    }

    #[test]
    fn test_robustness_malformed_length() {
        let mut packet = StunBuilder::new().add_xor_address("1.2.3.4", 5000).build();

        packet[3] = 0xFF; // Big length

        let _ = parse_stun_response(&packet);
    }
}
