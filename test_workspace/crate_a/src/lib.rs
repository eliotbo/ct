//! Crate A - Example crate for testing ct tool

use shared::common::Config;

/// Main application state
pub struct State {
    /// Current configuration
    pub config: Config,
    /// Application name
    pub name: String,
    /// Whether the app is running
    pub running: bool,
    /// Internal counter
    counter: u32,
}

impl State {
    /// Create a new State instance
    pub fn new(name: String) -> Self {
        State {
            config: Config::default(),
            name,
            running: false,
            counter: 0,
        }
    }

    /// Start the application
    pub fn start(&mut self) {
        self.running = true;
    }

    /// Stop the application
    pub fn stop(&mut self) {
        self.running = false;
    }

    /// Get the current counter value
    pub fn counter(&self) -> u32 {
        self.counter
    }

    /// Increment the counter
    pub fn increment(&mut self) {
        self.counter += 1;
    }

    /// Reset everything
    pub fn reset(&mut self) {
        todo!("Implement reset functionality")
    }
}

/// Different types of operations
#[derive(Debug, Clone)]
pub enum Operation {
    /// Read operation
    Read { path: String },
    /// Write operation
    Write { path: String, data: Vec<u8> },
    /// Delete operation
    Delete { path: String },
    /// List operation
    List,
}

impl Operation {
    /// Execute the operation
    pub fn execute(&self) -> Result<(), String> {
        match self {
            Operation::Read { path } => {
                println!("Reading from {}", path);
                Ok(())
            }
            Operation::Write { path, data } => {
                println!("Writing {} bytes to {}", data.len(), path);
                Ok(())
            }
            Operation::Delete { path } => {
                unimplemented!("Delete operation not yet implemented")
            }
            Operation::List => {
                println!("Listing files");
                Ok(())
            }
        }
    }
}

/// A trait for processors
pub trait Processor {
    /// Process some data
    fn process(&mut self, data: &[u8]) -> Vec<u8>;
    
    /// Get processor name
    fn name(&self) -> &str;
}

/// Simple processor implementation
pub struct SimpleProcessor {
    name: String,
}

impl SimpleProcessor {
    pub fn new(name: String) -> Self {
        SimpleProcessor { name }
    }
}

impl Processor for SimpleProcessor {
    fn process(&mut self, data: &[u8]) -> Vec<u8> {
        // TODO: Implement actual processing logic
        data.to_vec()
    }

    fn name(&self) -> &str {
        &self.name
    }
}

pub mod utils {
    /// Helper function to validate input
    pub fn validate_input(input: &str) -> bool {
        !input.is_empty() && input.len() < 1000
    }

    /// Parse a configuration string
    pub fn parse_config(config_str: &str) -> Result<super::Config, String> {
        // FIXME: This is a temporary implementation
        Ok(super::Config::default())
    }
}