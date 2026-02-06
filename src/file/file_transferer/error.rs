use std::fmt::Display;

#[derive(Debug)]
pub enum FileTransfererError {}

impl Display for FileTransfererError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> Result<(), std::fmt::Error> {
        match self {
            _ => todo!(),
        }
    }
}
