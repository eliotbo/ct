pub mod config;
pub mod models;
pub mod transport;
pub mod utils;

use blake3::Hasher;
use std::path::Path;
use thiserror::Error;

pub const TOOL_FINGERPRINT: &str = "ct-v0.1.0";

#[derive(Error, Debug)]
pub enum CoreError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    
    #[error("Configuration error: {0}")]
    Config(String),
    
    #[error("Invalid path: {0}")]
    InvalidPath(String),
    
    #[error("Workspace not found")]
    WorkspaceNotFound,
}

pub type Result<T> = std::result::Result<T, CoreError>;

pub fn compute_symbol_id(
    def_path: &str,
    kind: &str,
    file_digest: &str,
    span_start: u32,
    span_end: u32,
) -> String {
    let mut hasher = Hasher::new();
    hasher.update(TOOL_FINGERPRINT.as_bytes());
    hasher.update(def_path.as_bytes());
    hasher.update(kind.as_bytes());
    hasher.update(file_digest.as_bytes());
    hasher.update(&span_start.to_le_bytes());
    hasher.update(&span_end.to_le_bytes());
    
    let hash = hasher.finalize();
    let bytes = hash.as_bytes();
    hex::encode(&bytes[..16])
}

pub fn compute_file_digest(content: &[u8]) -> String {
    let hash = blake3::hash(content);
    format!("blake3:{}", hash.to_hex())
}

pub fn compute_workspace_fingerprint(workspace_path: &Path) -> String {
    let mut hasher = Hasher::new();
    hasher.update(workspace_path.to_string_lossy().as_bytes());
    let hash = hasher.finalize();
    format!("blake3:{}", &hash.to_hex()[..16])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_symbol_id_generation() {
        let id1 = compute_symbol_id(
            "crate::util::State",
            "struct",
            "blake3:abc123",
            100,
            200,
        );
        
        let id2 = compute_symbol_id(
            "crate::util::State",
            "struct",
            "blake3:abc123",
            100,
            200,
        );
        
        assert_eq!(id1, id2);
        assert_eq!(id1.len(), 32); // 16 bytes as hex
    }

    #[test]
    fn test_file_digest() {
        let content = b"hello world";
        let digest = compute_file_digest(content);
        assert!(digest.starts_with("blake3:"));
    }
}