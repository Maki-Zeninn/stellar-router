//! Structured JSON logging setup.
//!
//! Initialises `tracing-subscriber` with:
//! - JSON format (machine-readable, no sensitive data)
//! - Timestamps (RFC 3339)
//! - Log levels controlled via `RUST_LOG` env var
//! - Request-ID propagation via [`RequestId`] span field

use anyhow::Result;
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

/// Initialise the global tracing subscriber with JSON output.
///
/// Log level is read from `RUST_LOG`; defaults to `info` for this crate.
pub fn init_logging(default_level: &str) -> Result<()> {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(default_level));

    tracing_subscriber::registry()
        .with(
            fmt::layer()
                .json()                        // JSON format
                .with_current_span(true)       // include active span fields
                .with_span_list(false)
                .with_target(true)             // module path
                .with_file(false)
                .with_line_number(false),
        )
        .with(filter)
        .try_init()
        .map_err(|e| anyhow::anyhow!("failed to init tracing: {e}"))?;

    Ok(())
}

/// Generate a new random request ID (UUID v4).
pub fn new_request_id() -> String {
    uuid::Uuid::new_v4().to_string()
}
