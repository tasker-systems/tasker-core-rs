//! Generate the worker OpenAPI specification as JSON.
//!
//! Usage:
//!   cargo run --package tasker-worker --bin generate-openapi --features web-api

use tasker_worker::web::openapi::ApiDoc;
use utoipa::OpenApi;

fn main() {
    let spec = ApiDoc::openapi();
    println!(
        "{}",
        serde_json::to_string_pretty(&spec).expect("Failed to serialize OpenAPI spec")
    );
}
