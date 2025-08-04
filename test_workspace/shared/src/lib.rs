//! Shared utilities and types

pub mod common {
    use std::collections::HashMap;

    /// Global configuration
    #[derive(Debug, Clone)]
    pub struct Config {
        pub debug: bool,
        pub max_connections: usize,
        pub timeout_ms: u64,
        settings: HashMap<String, String>,
    }

    impl Default for Config {
        fn default() -> Self {
            Config {
                debug: false,
                max_connections: 100,
                timeout_ms: 5000,
                settings: HashMap::new(),
            }
        }
    }

    impl Config {
        /// Create a new config with custom settings
        pub fn new(debug: bool) -> Self {
            Config {
                debug,
                ..Default::default()
            }
        }

        /// Get a setting value
        pub fn get_setting(&self, key: &str) -> Option<&String> {
            self.settings.get(key)
        }

        /// Set a setting value
        pub fn set_setting(&mut self, key: String, value: String) {
            self.settings.insert(key, value);
        }

        /// Validate the configuration
        pub fn validate(&self) -> Result<()> {
            if self.max_connections == 0 {
                return Err(Error::InvalidInput("max_connections must be greater than 0".to_string()));
            }
            if self.timeout_ms == 0 {
                return Err(Error::InvalidInput("timeout_ms must be greater than 0".to_string()));
            }
            Ok(())
        }
    }

    /// Simple logger
    pub struct Logger {
        prefix: String,
        enabled: bool,
    }

    impl Logger {
        pub fn new(prefix: &str) -> Self {
            Logger {
                prefix: prefix.to_string(),
                enabled: true,
            }
        }

        pub fn log(&self, message: &str) {
            if self.enabled {
                println!("[{}] {}", self.prefix, message);
            }
        }

        pub fn error(&self, message: &str) {
            if self.enabled {
                eprintln!("[{}] ERROR: {}", self.prefix, message);
            }
        }

        pub fn set_enabled(&mut self, enabled: bool) {
            self.enabled = enabled;
        }
    }

    /// Result type alias
    pub type Result<T> = std::result::Result<T, Error>;

    /// Custom error type
    #[derive(Debug)]
    pub enum Error {
        InvalidInput(String),
        NotFound(String),
        Internal(String),
    }

    impl std::fmt::Display for Error {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            match self {
                Error::InvalidInput(msg) => write!(f, "Invalid input: {}", msg),
                Error::NotFound(msg) => write!(f, "Not found: {}", msg),
                Error::Internal(msg) => write!(f, "Internal error: {}", msg),
            }
        }
    }

    impl std::error::Error for Error {}
}

pub mod constants {
    /// Default buffer size
    pub const BUFFER_SIZE: usize = 8192;
    
    /// Maximum retry attempts
    pub const MAX_RETRIES: u32 = 3;
    
    /// Version string
    pub const VERSION: &str = "0.1.0";
}

/// Re-export commonly used items
pub use common::{Config, Logger, Error, Result};