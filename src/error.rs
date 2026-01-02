//! Error handling for the DataVis-RS application
//!
//! This module defines custom error types and a Result alias for use
//! throughout the application.

use thiserror::Error;

/// Main error type for DataVis-RS operations
#[derive(Error, Debug)]
pub enum DataVisError {
    /// Errors related to probe/SWD operations
    #[error("Probe error: {0}")]
    Probe(#[from] probe_rs::Error),

    /// Errors related to debug probe operations
    #[error("Debug probe error: {0}")]
    DebugProbe(#[from] probe_rs::probe::DebugProbeError),

    /// Errors related to target registry
    #[error("Registry error: {0}")]
    Registry(#[from] probe_rs::config::RegistryError),

    /// Errors related to Rhai script execution
    #[error("Script error: {0}")]
    Script(String),

    /// Errors related to configuration loading/saving
    #[error("Configuration error: {0}")]
    Config(String),

    /// Errors related to channel communication
    #[error("Channel error: {0}")]
    Channel(String),

    /// Errors related to variable parsing
    #[error("Variable error: {0}")]
    Variable(String),

    /// Errors related to memory access
    #[error("Memory access error at address 0x{address:08X}: {message}")]
    MemoryAccess { address: u64, message: String },

    /// Errors related to ELF file parsing
    #[error("ELF parsing error: {0}")]
    ElfParsing(String),

    /// Timeout errors
    #[error("Timeout: {0}")]
    Timeout(String),

    /// IO errors
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// Serialization errors
    #[error("Serialization error: {0}")]
    Serialization(String),

    /// Generic errors with context
    #[error("{context}: {source}")]
    WithContext {
        context: String,
        #[source]
        source: Box<DataVisError>,
    },
}

impl DataVisError {
    /// Add context to an error
    pub fn with_context(self, context: impl Into<String>) -> Self {
        DataVisError::WithContext {
            context: context.into(),
            source: Box::new(self),
        }
    }

    /// Create a script error from a Rhai error
    pub fn from_rhai_error(err: Box<rhai::EvalAltResult>) -> Self {
        DataVisError::Script(err.to_string())
    }
}

/// Result type alias for DataVis-RS operations
pub type Result<T> = std::result::Result<T, DataVisError>;

/// Extension trait for adding context to Results
pub trait ResultExt<T> {
    /// Add context to an error result
    fn context(self, context: impl Into<String>) -> Result<T>;

    /// Add context lazily to an error result
    fn with_context<F>(self, f: F) -> Result<T>
    where
        F: FnOnce() -> String;
}

impl<T> ResultExt<T> for Result<T> {
    fn context(self, context: impl Into<String>) -> Result<T> {
        self.map_err(|e| e.with_context(context))
    }

    fn with_context<F>(self, f: F) -> Result<T>
    where
        F: FnOnce() -> String,
    {
        self.map_err(|e| e.with_context(f()))
    }
}

impl<T> ResultExt<T> for std::result::Result<T, Box<rhai::EvalAltResult>> {
    fn context(self, context: impl Into<String>) -> Result<T> {
        self.map_err(|e| DataVisError::from_rhai_error(e).with_context(context))
    }

    fn with_context<F>(self, f: F) -> Result<T>
    where
        F: FnOnce() -> String,
    {
        self.map_err(|e| DataVisError::from_rhai_error(e).with_context(f()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_display() {
        let err = DataVisError::Variable("Invalid variable name".to_string());
        assert_eq!(err.to_string(), "Variable error: Invalid variable name");
    }

    #[test]
    fn test_error_with_context() {
        let err = DataVisError::Variable("test".to_string());
        let with_ctx = err.with_context("Failed to parse");
        assert!(with_ctx.to_string().contains("Failed to parse"));
    }

    #[test]
    fn test_memory_access_error() {
        let err = DataVisError::MemoryAccess {
            address: 0x2000_0000,
            message: "Access denied".to_string(),
        };
        assert!(err.to_string().contains("0x20000000"));
        assert!(err.to_string().contains("Access denied"));
    }
}
