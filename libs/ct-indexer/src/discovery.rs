use crate::{WorkspaceMember, Result, IndexError};
use std::path::Path;
use std::process::Command;
use serde::Deserialize;
use tracing::{debug, info};

#[derive(Debug, Deserialize)]
struct CargoMetadata {
    #[allow(dead_code)]
    workspace_root: String,
    workspace_members: Vec<String>,
    packages: Vec<Package>,
}

#[derive(Debug, Deserialize)]
struct Package {
    id: String,
    name: String,
    version: String,
    manifest_path: String,
}

pub async fn discover_workspace_members(workspace_root: &Path) -> Result<Vec<WorkspaceMember>> {
    info!("Discovering workspace members at {:?}", workspace_root);
    
    let output = Command::new("cargo")
        .arg("metadata")
        .arg("--no-deps")
        .arg("--format-version")
        .arg("1")
        .current_dir(workspace_root)
        .output()
        .map_err(|e| IndexError::Io(e))?;
    
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(IndexError::IndexingFailed(
            format!("cargo metadata failed: {}", stderr)
        ));
    }
    
    let metadata: CargoMetadata = serde_json::from_slice(&output.stdout)?;
    
    let mut members = Vec::new();
    
    for package in metadata.packages {
        if metadata.workspace_members.contains(&package.id) {
            let path = Path::new(&package.manifest_path)
                .parent()
                .ok_or_else(|| IndexError::IndexingFailed(
                    format!("Invalid manifest path: {}", package.manifest_path)
                ))?
                .to_path_buf();
            
            debug!("Found workspace member: {} at {:?}", package.name, path);
            
            members.push(WorkspaceMember {
                name: package.name,
                version: package.version,
                path,
                package_id: package.id,
            });
        }
    }
    
    Ok(members)
}

pub fn get_rustc_version() -> Result<String> {
    let output = Command::new("rustc")
        .arg("--version")
        .arg("--verbose")
        .output()
        .map_err(|e| IndexError::Io(e))?;
    
    if !output.status.success() {
        return Err(IndexError::IndexingFailed(
            "Failed to get rustc version".to_string()
        ));
    }
    
    let version_info = String::from_utf8_lossy(&output.stdout);
    let commit_hash = version_info
        .lines()
        .find(|line| line.starts_with("commit-hash:"))
        .and_then(|line| line.split_whitespace().nth(1))
        .unwrap_or("unknown");
    
    Ok(format!("sha256:{}", commit_hash))
}

pub fn get_cfg_snapshot() -> Result<String> {
    let output = Command::new("rustc")
        .arg("--print")
        .arg("cfg")
        .output()
        .map_err(|e| IndexError::Io(e))?;
    
    if !output.status.success() {
        return Err(IndexError::IndexingFailed(
            "Failed to get cfg snapshot".to_string()
        ));
    }
    
    let cfg = String::from_utf8_lossy(&output.stdout);
    let mut hasher = blake3::Hasher::new();
    hasher.update(cfg.as_bytes());
    
    Ok(format!("blake3:{}", hasher.finalize().to_hex()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rustc_version() {
        let version = get_rustc_version();
        assert!(version.is_ok());
        assert!(version.unwrap().starts_with("sha256:"));
    }

    #[test]
    fn test_cfg_snapshot() {
        let snapshot = get_cfg_snapshot();
        assert!(snapshot.is_ok());
        assert!(snapshot.unwrap().starts_with("blake3:"));
    }
}