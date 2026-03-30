use std::fmt::Display;

#[derive(Debug)]
pub enum FileMetadataError {
    MetadataError,
    NameError,
}

impl Display for FileMetadataError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> Result<(), std::fmt::Error> {
        match self {
            Self::MetadataError => write!(f, "Error: \"Failed to obtain metadata from file\""),
            Self::NameError => write!(f, "Error: \"Failed to obtain file name or invalid UTF-8\""),
        }
    }
}
