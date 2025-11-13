/// SDP keys used when parsing session description lines.
///
/// These constants represent the single-letter SDP line prefixes used
/// when processing a textual SDP. They are kept here to avoid magic
/// strings scattered through the parser.
///
/// - `m` indicates the start of a media description line ("m=").
/// - `a` indicates an attribute line ("a=") that should be attached
///   to the most-recently-parsed media description.
pub const MEDIA_DESCRIPTION_KEY: &str = "m";
pub const ATTRIBUTE_KEY: &str = "a";
