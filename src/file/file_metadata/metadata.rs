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
            .ok_or(Error::NameError)?
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

#[cfg(test)]
mod tests {
    use super::FileMetadata;
    use std::fs::{File, remove_file};
    use std::io::Write;
    use std::path::Path;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn create_temp_file_with_size(size: usize) -> (std::path::PathBuf, File) {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let mut path = std::env::temp_dir();
        path.push(format!("metadata_test_{}.tmp", nanos));

        let mut f = File::create(&path).expect("create temp file");
        let data = vec![0u8; size];
        f.write_all(&data).expect("write data");
        drop(f);
        let f = File::open(&path).expect("open temp file");
        (path, f)
    }

    #[test]
    fn from_and_to_bytes_roundtrip() {
        let (path, file) = create_temp_file_with_size(10);
        let name = path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap()
            .to_string();

        let meta = FileMetadata::from(Path::new(&path), &file).expect("from should succeed");
        assert_eq!(meta.size, 10);
        assert_eq!(meta.name, name);

        let bytes = meta.to_bytes();
        assert_eq!(bytes.len(), 8 + 1 + meta.name.len());

        let parsed = FileMetadata::from_bytes(&bytes).expect("from_bytes should parse");
        assert_eq!(parsed.size, meta.size);
        assert_eq!(parsed.name, meta.name);

        let _ = remove_file(path);
    }

    #[test]
    fn from_bytes_invalid_cases() {
        assert!(FileMetadata::from_bytes(&[]).is_none());

        let short = vec![0u8; 5];
        assert!(FileMetadata::from_bytes(&short).is_none());

        let mut v = Vec::new();
        v.extend_from_slice(&100u64.to_be_bytes());
        v.push(5u8);
        assert!(FileMetadata::from_bytes(&v).is_none());
    }

    #[test]
    fn from_name_error_returns_err() {
        let (path, file) = create_temp_file_with_size(1);
        let p = Path::new("");
        let res = FileMetadata::from(p, &file);
        assert!(res.is_err());
        let _ = remove_file(path);
    }
}
