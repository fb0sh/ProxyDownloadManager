use std::fmt;

/// Structured error type for the ProxyDownloadManager domain.
#[derive(Debug, Clone, PartialEq)]
pub enum PdmError {
    /// Download was cancelled by the user.
    Cancelled,
    /// HTTP error with status code.
    Http(u16),
    /// Failed to build an HTTP client (invalid proxy, TLS error).
    ClientBuild(String),
    /// Probe failed (all user-agents exhausted).
    Probe(String),
    /// Download not found.
    NotFound(u64),
    /// Database error.
    Db(String),
    /// Configuration error (load, save, parse).
    Config(String),
    /// I/O error (file operations).
    Io(String),
    /// WebSocket / network error.
    Network(String),
    /// Generic fallback with message.
    Other(String),
}

impl fmt::Display for PdmError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Cancelled => write!(f, "Cancelled"),
            Self::Http(code) => write!(f, "HTTP {}", code),
            Self::ClientBuild(msg) => write!(f, "Client build failed: {}", msg),
            Self::Probe(msg) => write!(f, "Probe failed: {}", msg),
            Self::NotFound(id) => write!(f, "Download {} not found", id),
            Self::Db(msg) => write!(f, "Database error: {}", msg),
            Self::Config(msg) => write!(f, "Config error: {}", msg),
            Self::Io(msg) => write!(f, "I/O error: {}", msg),
            Self::Network(msg) => write!(f, "Network error: {}", msg),
            Self::Other(msg) => write!(f, "{}", msg),
        }
    }
}

impl std::error::Error for PdmError {}

impl From<reqwest::Error> for PdmError {
    fn from(e: reqwest::Error) -> Self {
        Self::Network(e.to_string())
    }
}

impl From<rusqlite::Error> for PdmError {
    fn from(e: rusqlite::Error) -> Self {
        Self::Db(e.to_string())
    }
}

impl From<std::io::Error> for PdmError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e.to_string())
    }
}

impl From<String> for PdmError {
    fn from(s: String) -> Self {
        Self::Other(s)
    }
}

impl From<&str> for PdmError {
    fn from(s: &str) -> Self {
        Self::Other(s.to_string())
    }
}

impl<T> From<std::sync::PoisonError<T>> for PdmError {
    fn from(_: std::sync::PoisonError<T>) -> Self {
        Self::Other("Mutex poisoned".to_string())
    }
}

impl From<serde_json::Error> for PdmError {
    fn from(e: serde_json::Error) -> Self {
        Self::Other(e.to_string())
    }
}

/// Convenience alias for domain results.
pub type PdmResult<T> = Result<T, PdmError>;
