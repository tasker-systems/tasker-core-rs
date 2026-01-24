//! # OpenAPI Security Scheme Definitions
//!
//! Shared security scheme modifier for utoipa OpenAPI specs. Used by both
//! orchestration and worker crates to declare consistent auth requirements.

use utoipa::openapi::security::{ApiKey, ApiKeyValue, Http, HttpAuthScheme, SecurityScheme};
use utoipa::openapi::OpenApi;
use utoipa::Modify;

/// Adds JWT Bearer and API Key security schemes to an OpenAPI spec.
///
/// Both orchestration and worker APIs share the same auth mechanisms:
/// - `bearer_auth`: HTTP Bearer token (JWT, RS256)
/// - `api_key_auth`: API key in configurable header (default: `X-API-Key`)
///
/// # Usage
///
/// ```ignore
/// #[derive(OpenApi)]
/// #[openapi(modifiers(&SecurityAddon), ...)]
/// struct ApiDoc;
/// ```
#[derive(Debug)]
pub struct SecurityAddon;

impl Modify for SecurityAddon {
    fn modify(&self, openapi: &mut OpenApi) {
        let components = openapi.components.get_or_insert_with(Default::default);

        components.add_security_scheme(
            "bearer_auth",
            SecurityScheme::Http(Http::new(HttpAuthScheme::Bearer)),
        );

        components.add_security_scheme(
            "api_key_auth",
            SecurityScheme::ApiKey(ApiKey::Header(ApiKeyValue::new("X-API-Key"))),
        );
    }
}
