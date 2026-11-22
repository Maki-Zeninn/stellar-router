use utoipa::openapi::security::{ApiKey, ApiKeyValue, HttpAuthScheme, HttpBuilder, SecurityScheme};
use utoipa::{Modify, OpenApi};

#[derive(OpenApi)]
#[openapi(
    paths(
        crate::server::metrics_handler,
        crate::server::health_handler,
        crate::server::ready_handler,
    ),
    info(title = "Router Metrics Exporter API"),
    modifiers(&ApiDocAddon)
)]
pub struct ApiDoc;

/// Fills in what the derive can't express directly: the crate version (so the
/// spec never drifts from `Cargo.toml`) and the two ways `/metrics` accepts an
/// API key (see `auth.rs`).
struct ApiDocAddon;

impl Modify for ApiDocAddon {
    fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
        openapi.info.version = env!("CARGO_PKG_VERSION").to_string();

        let components = openapi.components.get_or_insert_with(Default::default);
        components.add_security_scheme(
            "api_key",
            SecurityScheme::ApiKey(ApiKey::Header(ApiKeyValue::new("X-API-Key"))),
        );
        components.add_security_scheme(
            "bearer",
            SecurityScheme::Http(HttpBuilder::new().scheme(HttpAuthScheme::Bearer).build()),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    fn spec() -> Value {
        serde_json::from_str(&ApiDoc::openapi().to_json().unwrap()).unwrap()
    }

    fn response_codes(spec: &Value, path: &str) -> Vec<String> {
        let mut codes: Vec<String> = spec["paths"][path]["get"]["responses"]
            .as_object()
            .unwrap_or_else(|| panic!("no responses for {path}"))
            .keys()
            .cloned()
            .collect();
        codes.sort();
        codes
    }

    #[test]
    fn version_comes_from_cargo_toml() {
        assert_eq!(spec()["info"]["version"], env!("CARGO_PKG_VERSION"));
    }

    #[test]
    fn declares_api_key_and_bearer_security_schemes() {
        let spec = spec();
        let schemes = &spec["components"]["securitySchemes"];
        assert_eq!(schemes["api_key"]["type"], "apiKey");
        assert_eq!(schemes["api_key"]["in"], "header");
        assert_eq!(schemes["api_key"]["name"], "X-API-Key");
        assert_eq!(schemes["bearer"]["type"], "http");
        assert_eq!(schemes["bearer"]["scheme"], "bearer");
    }

    #[test]
    fn metrics_requires_auth_and_documents_every_status() {
        let spec = spec();
        assert_eq!(
            response_codes(&spec, "/metrics"),
            ["200", "400", "401", "409", "429", "500"]
        );
        let security = spec["paths"]["/metrics"]["get"]["security"]
            .as_array()
            .expect("/metrics declares security requirements");
        assert!(security.iter().any(|s| s.get("api_key").is_some()));
        assert!(security.iter().any(|s| s.get("bearer").is_some()));
        assert!(
            spec["paths"]["/metrics"]["get"]["responses"]["429"]["headers"]["Retry-After"]
                .is_object(),
            "429 documents the Retry-After header"
        );
    }

    #[test]
    fn probes_document_rate_limiting() {
        let spec = spec();
        assert!(response_codes(&spec, "/health").contains(&"429".to_string()));
        assert!(response_codes(&spec, "/ready").contains(&"429".to_string()));
    }
}
