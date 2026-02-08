use crate::file::file_metadata::FileMetadataError as Error;
use std::fs::File;
use std::path::Path;

#[derive(Clone)]
pub struct FileMetadata {
    pub size: u64,
    pub name: String,
}

impl FileMetadata {
    pub fn from(file_path: &Path, file: &File) -> Result<Self, Error> {
        let name = file_path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap()
            .to_string();
        let size = file.metadata().map_err(|_| Error::MetadataError)?.len();

        Ok(Self { name, size })
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = vec![];
        bytes.extend_from_slice(&self.size.to_be_bytes());
        bytes.push(self.name.len() as u8);
        bytes.extend_from_slice(self.name.as_bytes());
        bytes
    }

    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        if bytes.len() >= 9 {
            let file_name_len = bytes[8] as usize;
            let start = 9;
            let end_name = start + file_name_len;
            if bytes.len() < end_name {
                return None;
            }
            Some(Self {
                size: u64::from_be_bytes(bytes[0..8].try_into().ok()?),
                name: String::from_utf8(bytes[start..end_name].to_vec()).ok()?,
            })
        } else {
            None
        }
    }
}
