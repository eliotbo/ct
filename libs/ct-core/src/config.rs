use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use directories::ProjectDirs;
use crate::{CoreError, Result};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    #[serde(default = "default_transport")]
    pub transport: Transport,
    
    #[serde(default = "default_autostart")]
    pub autostart: bool,
    
    #[serde(default = "default_socket_path")]
    pub socket_path: String,
    
    #[serde(default = "default_pipe_name")]
    pub pipe_name: String,
    
    #[serde(default = "default_tcp_addr")]
    pub tcp_addr: String,
    
    #[serde(default = "default_allow_full_context")]
    pub allow_full_context: bool,
    
    #[serde(default)]
    pub workspace_allow: Vec<PathBuf>,
    
    #[serde(default = "default_max_context_size")]
    pub max_context_size: usize,
    
    #[serde(default = "default_max_list")]
    pub max_list: usize,
    
    #[serde(default = "default_bundle_source_cap")]
    pub bundle_source_cap: usize,
    
    #[serde(default)]
    pub db_dir: Option<PathBuf>,
    
    #[serde(default = "default_db_file")]
    pub db_file: String,
    
    #[serde(default = "default_references_top_n")]
    pub references_top_n: usize,
    
    #[serde(default = "default_max_mem_mb")]
    pub max_mem_mb: usize,
    
    #[serde(default = "default_bench_queries")]
    pub bench_queries: u32,
    
    #[serde(default = "default_bench_duration_s")]
    pub bench_duration_s: u32,
    
    #[serde(default = "default_watcher_debounce_ms")]
    pub watcher_debounce_ms: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Transport {
    Auto,
    Unix,
    Pipe,
    Tcp,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            transport: default_transport(),
            autostart: default_autostart(),
            socket_path: default_socket_path(),
            pipe_name: default_pipe_name(),
            tcp_addr: default_tcp_addr(),
            allow_full_context: default_allow_full_context(),
            workspace_allow: vec![],
            max_context_size: default_max_context_size(),
            max_list: default_max_list(),
            bundle_source_cap: default_bundle_source_cap(),
            db_dir: None,
            db_file: default_db_file(),
            references_top_n: default_references_top_n(),
            max_mem_mb: default_max_mem_mb(),
            bench_queries: default_bench_queries(),
            bench_duration_s: default_bench_duration_s(),
            watcher_debounce_ms: default_watcher_debounce_ms(),
        }
    }
}

fn default_transport() -> Transport {
    Transport::Auto
}

fn default_autostart() -> bool {
    true
}

fn default_socket_path() -> String {
    "/tmp/ctd.sock".to_string()
}

fn default_pipe_name() -> String {
    r"\\.\pipe\ctd".to_string()
}

fn default_tcp_addr() -> String {
    "127.0.0.1:48732".to_string()
}

fn default_allow_full_context() -> bool {
    false
}

fn default_max_context_size() -> usize {
    16000
}

fn default_max_list() -> usize {
    200
}

fn default_bundle_source_cap() -> usize {
    3000
}

fn default_db_file() -> String {
    "symbols.sqlite".to_string()
}

fn default_references_top_n() -> usize {
    16
}

fn default_max_mem_mb() -> usize {
    512
}

fn default_bench_queries() -> u32 {
    200
}

fn default_bench_duration_s() -> u32 {
    5
}

fn default_watcher_debounce_ms() -> u64 {
    300
}

impl Config {
    pub fn load() -> Result<Self> {
        if let Ok(content) = std::fs::read_to_string("ct.toml") {
            toml::from_str(&content)
                .map_err(|e| CoreError::Config(format!("Failed to parse ct.toml: {}", e)))
        } else {
            Ok(Self::default())
        }
    }

    pub fn get_db_path(&self, workspace_fingerprint: &str) -> PathBuf {
        if let Some(dir) = &self.db_dir {
            dir.join(&self.db_file)
        } else {
            self.get_cache_dir(workspace_fingerprint).join(&self.db_file)
        }
    }

    pub fn get_cache_dir(&self, workspace_fingerprint: &str) -> PathBuf {
        if let Some(proj_dirs) = ProjectDirs::from("", "", "ct") {
            proj_dirs.cache_dir().join(workspace_fingerprint)
        } else {
            PathBuf::from(".ct").join(workspace_fingerprint)
        }
    }

    pub fn get_socket_path(&self, workspace_fingerprint: &str) -> String {
        if cfg!(unix) {
            format!("/tmp/ctd-{}.sock", &workspace_fingerprint[..8])
        } else {
            self.socket_path.clone()
        }
    }

    pub fn get_pipe_name(&self, workspace_fingerprint: &str) -> String {
        if cfg!(windows) {
            format!(r"\\.\pipe\ctd-{}", &workspace_fingerprint[..8])
        } else {
            self.pipe_name.clone()
        }
    }

    pub fn get_effective_transport(&self) -> Transport {
        match self.transport {
            Transport::Auto => {
                if cfg!(unix) {
                    Transport::Unix
                } else if cfg!(windows) {
                    Transport::Pipe
                } else {
                    Transport::Tcp
                }
            }
            other => other,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = Config::default();
        assert_eq!(config.max_context_size, 16000);
        assert_eq!(config.autostart, true);
    }

    #[test]
    fn test_effective_transport() {
        let config = Config::default();
        let transport = config.get_effective_transport();
        #[cfg(unix)]
        assert_eq!(transport, Transport::Unix);
        #[cfg(windows)]
        assert_eq!(transport, Transport::Pipe);
    }
}