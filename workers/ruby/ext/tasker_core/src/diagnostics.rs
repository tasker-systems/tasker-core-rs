//! System diagnostics for Ruby FFI
//!
//! Provides diagnostic information about the Ruby FFI environment to help
//! troubleshoot architecture-specific issues (M2 vs M4 Pro, etc.)

use magnus::value::ReprValue;
use magnus::{Error, ExceptionClass, Ruby, Value};

/// Helper to get RuntimeError exception class
fn runtime_error_class() -> ExceptionClass {
    Ruby::get()
        .expect("Ruby runtime should be available")
        .exception_runtime_error()
}

/// Get system diagnostics for troubleshooting
///
/// Returns a hash with:
/// - rust_version: Cargo package version
/// - ruby_version: Ruby version string
/// - ruby_abi: Ruby ABI version
/// - thread_count: Available parallelism (CPU cores)
/// - arch: System architecture (aarch64, x86_64, etc.)
/// - tokio_runtime: Information about Tokio runtime if available
pub fn system_diagnostics() -> Result<Value, Error> {
    let ruby = Ruby::get().map_err(|err| {
        Error::new(
            runtime_error_class(),
            format!("Failed to get Ruby runtime: {}", err),
        )
    })?;

    let hash = ruby.hash_new();

    // Rust environment
    hash.aset("rust_version", env!("CARGO_PKG_VERSION"))?;
    hash.aset("magnus_version", "0.8")?;

    // Ruby environment
    let ruby_version = ruby
        .eval::<String>("RUBY_VERSION")
        .unwrap_or_else(|_| "unknown".to_string());
    hash.aset("ruby_version", ruby_version)?;

    // System information
    let thread_count = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(0);
    hash.aset("thread_count", thread_count)?;
    hash.aset("arch", std::env::consts::ARCH)?;
    hash.aset("os", std::env::consts::OS)?;

    // Build information - embed feature was removed for M4 Pro compatibility
    hash.aset("magnus_embed_feature", false)?;

    Ok(hash.as_value())
}
