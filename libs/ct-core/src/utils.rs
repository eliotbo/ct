use std::path::{Path, PathBuf};
use crate::{CoreError, Result};
use std::process::Command;

pub fn find_workspace_root(start_path: &Path) -> Result<PathBuf> {
    let mut current = start_path;
    
    loop {
        if current.join("Cargo.toml").exists() {
            let output = Command::new("cargo")
                .arg("metadata")
                .arg("--no-deps")
                .arg("--format-version")
                .arg("1")
                .current_dir(current)
                .output()
                .map_err(|e| CoreError::Io(e))?;
                
            if output.status.success() {
                let metadata: serde_json::Value = serde_json::from_slice(&output.stdout)
                    .map_err(|e| CoreError::Config(format!("Invalid cargo metadata: {}", e)))?;
                    
                if let Some(workspace_root) = metadata["workspace_root"].as_str() {
                    return Ok(PathBuf::from(workspace_root));
                }
            }
        }
        
        if let Some(parent) = current.parent() {
            current = parent;
        } else {
            break;
        }
    }
    
    Err(CoreError::WorkspaceNotFound)
}

pub fn normalize_path(path: &str, current_crate: Option<&str>) -> String {
    if path.starts_with("crate::") && current_crate.is_some() {
        path.replace("crate::", &format!("{}::", current_crate.unwrap()))
    } else {
        path.to_string()
    }
}

pub fn parse_expansion_operators(expansion: &str) -> (usize, usize) {
    let children = expansion.chars().filter(|&c| c == '>').count();
    let parents = expansion.chars().filter(|&c| c == '<').count();
    (children, parents)
}

pub fn validate_visibility_filter(vis: Option<&str>) -> Result<Option<&str>> {
    match vis {
        Some("public") | Some("private") | Some("all") | None => Ok(vis),
        Some(v) => Err(CoreError::Config(format!("Invalid visibility filter: {}", v))),
    }
}

pub fn format_exit_code(code: u8) -> String {
    match code {
        0 => "ok".to_string(),
        2 => "invalid args".to_string(),
        3 => "over-max decision required".to_string(),
        4 => "daemon unavailable".to_string(),
        5 => "index mismatch".to_string(),
        6 => "internal error".to_string(),
        _ => format!("unknown ({})", code),
    }
}

pub const EXIT_OK: u8 = 0;
pub const EXIT_INVALID_ARGS: u8 = 2;
pub const EXIT_OVER_MAX: u8 = 3;
pub const EXIT_DAEMON_UNAVAILABLE: u8 = 4;
pub const EXIT_INDEX_MISMATCH: u8 = 5;
pub const EXIT_INTERNAL_ERROR: u8 = 6;
pub const EXIT_DAEMON_ALREADY_RUNNING: u8 = 7;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_path() {
        assert_eq!(
            normalize_path("crate::util::State", Some("my_crate")),
            "my_crate::util::State"
        );
        assert_eq!(
            normalize_path("other_crate::util::State", Some("my_crate")),
            "other_crate::util::State"
        );
    }

    #[test]
    fn test_parse_expansion_operators() {
        assert_eq!(parse_expansion_operators(">>"), (2, 0));
        assert_eq!(parse_expansion_operators("<<"), (0, 2));
        assert_eq!(parse_expansion_operators("><"), (1, 1));
        assert_eq!(parse_expansion_operators(""), (0, 0));
    }

    #[test]
    fn test_validate_visibility_filter() {
        assert!(validate_visibility_filter(Some("public")).is_ok());
        assert!(validate_visibility_filter(Some("private")).is_ok());
        assert!(validate_visibility_filter(Some("all")).is_ok());
        assert!(validate_visibility_filter(None).is_ok());
        assert!(validate_visibility_filter(Some("invalid")).is_err());
    }
}