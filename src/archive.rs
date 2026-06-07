use std::path::Path;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum ArchiveError {}

pub fn create_archive(inputs: Vec<&Path>, out: &Path) -> Result<(), ArchiveError> {
    dbg!(inputs);
    dbg!(out);
    Ok(())
}
