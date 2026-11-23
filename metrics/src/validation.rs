//! Request validation for incoming HTTP query parameters.
//!
//! All public entry points return a structured [`ValidationError`] on failure
//! so callers can return a clear 422 response with a descriptive message.
//! No sensitive data is included in error messages.

use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::Serialize;

/// A structured validation error returned as JSON with HTTP 422 Unprocessable Entity.
#[derive(Debug, Serialize)]
pub struct ValidationError {
    /// Machine-readable error code, currently always `"validation_error"`.
    pub error: &'static str,
    /// Human-readable description of what failed validation, safe to show to API clients.
    pub message: String,
}

impl IntoResponse for ValidationError {
    fn into_response(self) -> Response {
        (StatusCode::UNPROCESSABLE_ENTITY, Json(self)).into_response()
    }
}

impl ValidationError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            error: "validation_error",
            message: message.into(),
        }
    }
}

// ── Validation rules ──────────────────────────────────────────────────────────

/// Required length of a Stellar contract ID (a base32-style strkey/address string).
const STELLAR_CONTRACT_ID_LEN: usize = 56;

/// Maximum allowed length of a route name.
const MAX_ROUTE_NAME_LEN: usize = 64;

/// Validate a contract ID: must be a `STELLAR_CONTRACT_ID_LEN`-character alphanumeric Stellar address.
pub fn validate_contract_id(id: &str) -> Result<(), ValidationError> {
    if id.is_empty() {
        return Err(ValidationError::new("contract_id must not be empty"));
    }
    if id.len() != STELLAR_CONTRACT_ID_LEN {
        return Err(ValidationError::new(format!(
            "contract_id must be exactly {STELLAR_CONTRACT_ID_LEN} characters"
        )));
    }
    if !id.starts_with('C') {
        return Err(ValidationError::new("contract_id must start with 'C'"));
    }
    if !id
        .chars()
        .all(|c| c.is_ascii_uppercase() || ('2'..='7').contains(&c))
    {
        return Err(ValidationError::new(
            "contract_id must contain only base32 characters (A-Z, 2-7)",
        ));
    }
    Ok(())
}

/// Validate a route name: non-empty, max `MAX_ROUTE_NAME_LEN` chars, alphanumeric + slash/hyphen.
///
/// Must stay in sync with the on-chain rules in
/// `contracts/router-core/src/lib.rs::validate_route_name` (allowed set
/// `[A-Za-z0-9/-]`; underscores are not allowed).
///
/// Not yet called from any handler — kept for validating route-name query
/// params once an endpoint accepts one.
#[allow(dead_code)]
pub fn validate_route_name(name: &str) -> Result<(), ValidationError> {
    if name.is_empty() {
        return Err(ValidationError::new("route name must not be empty"));
    }
    if name.len() > MAX_ROUTE_NAME_LEN {
        return Err(ValidationError::new(format!(
            "route name must be {MAX_ROUTE_NAME_LEN} characters or fewer"
        )));
    }
    if !name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '/' || c == '-')
    {
        return Err(ValidationError::new(
            "route name must contain only alphanumeric characters, slashes, or hyphens",
        ));
    }
    Ok(())
}

/// Validate a scrape interval: must be between 1 and 3600 seconds.
pub fn validate_scrape_interval(secs: u64) -> Result<(), ValidationError> {
    if secs == 0 {
        return Err(ValidationError::new(
            "scrape_interval_secs must be greater than 0",
        ));
    }
    if secs > 3600 {
        return Err(ValidationError::new(
            "scrape_interval_secs must not exceed 3600",
        ));
    }
    Ok(())
}

/// Validate the RPC request timeout: must be greater than 0 seconds.
pub fn validate_rpc_timeout(secs: u64) -> Result<(), ValidationError> {
    if secs == 0 {
        return Err(ValidationError::new(
            "rpc_timeout_secs must be greater than 0",
        ));
    }
    Ok(())
}

/// Validate the listen address: must be a valid `host:port` string.
pub fn validate_listen_addr(addr: &str) -> Result<(), ValidationError> {
    addr.parse::<std::net::SocketAddr>()
        .map(|_| ())
        .map_err(|_| {
            ValidationError::new("listen address must be a valid host:port (e.g. 0.0.0.0:9090)")
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_contract_id() {
        let id = "CDLZFC3SYJYDZT7K67VZ75HPJVIEUVNIXF47ZG2FB2RMQQVU2HHGCYSC";
        assert_eq!(id.len(), STELLAR_CONTRACT_ID_LEN);
        assert!(validate_contract_id(id).is_ok());
    }

    #[test]
    fn short_contract_id_rejected() {
        assert!(validate_contract_id("SHORT").is_err());
    }

    #[test]
    fn empty_contract_id_rejected() {
        assert!(validate_contract_id("").is_err());
    }

    #[test]
    fn contract_id_with_special_chars_rejected() {
        let id = format!("{}!", "A".repeat(STELLAR_CONTRACT_ID_LEN - 1));
        assert!(validate_contract_id(&id).is_err());
    }

    #[test]
    fn contract_id_lowercase_rejected() {
        let id = "CDLZFC3SYJYDZT7K67VZ75HPJVIEUVNIXF47ZG2FB2RMQQVU2HHGCYSC".to_lowercase();
        assert!(validate_contract_id(&id).is_err());
    }

    #[test]
    fn account_id_rejected() {
        let id = "GAAZI4TCR3TY5OJHCTJC2A4QSY6CJWJH5IAJTGKIN2ER7LBNVKOCCWN7";
        assert!(validate_contract_id(id).is_err());
    }

    #[test]
    fn contract_id_with_invalid_base32_digits_rejected() {
        for d in ['0', '1', '8', '9'] {
            let id = format!("C{}{}", d, "A".repeat(STELLAR_CONTRACT_ID_LEN - 2));
            assert!(validate_contract_id(&id).is_err(), "digit {d} accepted");
        }
    }

    #[test]
    fn valid_route_name() {
        assert!(validate_route_name("oracle/v2").is_ok());
    }

    #[test]
    fn route_name_with_slash_accepted() {
        assert!(validate_route_name("oracle/v2").is_ok());
    }

    #[test]
    fn route_name_with_underscore_rejected() {
        assert!(validate_route_name("oracle_feed").is_err());
    }

    #[test]
    fn empty_route_name_rejected() {
        assert!(validate_route_name("").is_err());
    }

    #[test]
    fn long_route_name_rejected() {
        assert!(validate_route_name(&"a".repeat(MAX_ROUTE_NAME_LEN + 1)).is_err());
    }

    #[test]
    fn route_name_with_spaces_rejected() {
        assert!(validate_route_name("bad name").is_err());
    }

    #[test]
    fn valid_scrape_interval() {
        assert!(validate_scrape_interval(15).is_ok());
    }

    #[test]
    fn zero_scrape_interval_rejected() {
        assert!(validate_scrape_interval(0).is_err());
    }

    #[test]
    fn too_large_scrape_interval_rejected() {
        assert!(validate_scrape_interval(3601).is_err());
    }

    #[test]
    fn valid_rpc_timeout() {
        assert!(validate_rpc_timeout(10).is_ok());
    }

    #[test]
    fn zero_rpc_timeout_rejected() {
        assert!(validate_rpc_timeout(0).is_err());
    }

    #[test]
    fn valid_listen_addr() {
        assert!(validate_listen_addr("0.0.0.0:9090").is_ok());
    }

    #[test]
    fn invalid_listen_addr_rejected() {
        assert!(validate_listen_addr("not-an-addr").is_err());
    }

    #[test]
    fn validation_error_responds_with_422() {
        let response = ValidationError::new("bad input").into_response();
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    }
}
