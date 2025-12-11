/// Represents responses sent by the server to the client.
///
/// This enum contains the server's responses to client messages and events,
/// primarily used for transmitting user information.
///
/// # Variants
///
/// - `Username`: Transmits a username string to the client.
pub enum ClientResponse {
    /// Username response containing a username string.
    Username(String),
}

impl ClientResponse {
    /// Converts a client response to its byte representation.
    ///
    /// This method serializes the response to a pipe-delimited text format
    /// and appends a newline at the end for network transmission.
    ///
    /// # Protocol Format
    ///
    /// - Username: `USERNAME|value\n`
    ///
    /// # Returns
    ///
    /// A vector of bytes containing the serialized response.
    #[must_use]
    pub fn to_bytes(&self) -> Vec<u8> {
        let s = match self {
            Self::Username(s) => format!("USERNAME|{s}"),
        };

        let mut bytes = s.into_bytes();
        bytes.push(b'\n');
        bytes
    }

    /// Deserializes a client response from bytes.
    ///
    /// This method parses a byte representation of the response (in pipe-delimited text format)
    /// and constructs the corresponding `ClientResponse` variant.
    ///
    /// # Parameters
    ///
    /// * `bytes` - Byte slice containing the serialized response. Expected to be valid UTF-8
    ///   and terminated with a newline character.
    ///
    /// # Returns
    ///
    /// * `Some(ClientResponse)` - If deserialization is successful.
    /// * `None` - If bytes are not valid UTF-8, format is incorrect,
    ///   or there are insufficient fields for the specified variant.
    #[must_use]
    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        let s = str::from_utf8(bytes).ok()?.trim();
        let parts: Vec<&str> = s.split('|').collect();

        match parts[0] {
            "USERNAME" if parts.len() == 2 => Some(Self::Username(parts[1].to_string())),
            _ => None,
        }
    }
}
