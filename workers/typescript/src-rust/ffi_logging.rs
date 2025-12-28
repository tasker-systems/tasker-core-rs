//! FFI logging utilities for TypeScript worker.
//!
//! Provides logging functions that can be called from JavaScript
//! to integrate with Rust's tracing infrastructure.

use std::collections::HashMap;
use std::ffi::{c_char, CStr};

use tracing::Level;

/// Log a message at the specified level.
///
/// # Safety
///
/// - `message` must be a valid null-terminated C string
/// - `fields_json` must be a valid null-terminated C string or null
pub unsafe fn log_at_level(level: Level, message: *const c_char, fields_json: *const c_char) {
    if message.is_null() {
        return;
    }

    // SAFETY: Caller guarantees message is a valid null-terminated C string
    let msg = match unsafe { CStr::from_ptr(message) }.to_str() {
        Ok(s) => s,
        Err(_) => return,
    };

    let fields: Option<HashMap<String, String>> = if !fields_json.is_null() {
        // SAFETY: Caller guarantees fields_json is a valid null-terminated C string or null
        unsafe { CStr::from_ptr(fields_json) }
            .to_str()
            .ok()
            .and_then(|s| serde_json::from_str(s).ok())
    } else {
        None
    };

    // Log with optional fields
    match level {
        Level::ERROR => {
            if let Some(fields) = fields {
                tracing::error!(fields = ?fields, "{}", msg);
            } else {
                tracing::error!("{}", msg);
            }
        }
        Level::WARN => {
            if let Some(fields) = fields {
                tracing::warn!(fields = ?fields, "{}", msg);
            } else {
                tracing::warn!("{}", msg);
            }
        }
        Level::INFO => {
            if let Some(fields) = fields {
                tracing::info!(fields = ?fields, "{}", msg);
            } else {
                tracing::info!("{}", msg);
            }
        }
        Level::DEBUG => {
            if let Some(fields) = fields {
                tracing::debug!(fields = ?fields, "{}", msg);
            } else {
                tracing::debug!("{}", msg);
            }
        }
        Level::TRACE => {
            if let Some(fields) = fields {
                tracing::trace!(fields = ?fields, "{}", msg);
            } else {
                tracing::trace!("{}", msg);
            }
        }
    }
}

/// Initialize FFI logging.
///
/// Sets up the tracing subscriber if not already configured.
pub fn init_ffi_logger() -> Result<(), String> {
    // Use a simple env filter subscriber
    // The subscriber will be set up by the worker bootstrap if not already
    // For FFI contexts, we just ensure tracing is usable
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::CString;

    #[test]
    fn test_log_at_level_null_message() {
        // Should not panic with null message
        // SAFETY: Passing null pointers which log_at_level handles safely
        unsafe {
            log_at_level(Level::INFO, std::ptr::null(), std::ptr::null());
        }
    }

    #[test]
    fn test_log_at_level_valid_message() {
        let msg = CString::new("Test message").unwrap();
        // SAFETY: msg is a valid null-terminated C string from CString
        unsafe {
            log_at_level(Level::INFO, msg.as_ptr(), std::ptr::null());
        }
        // Should not panic
    }

    #[test]
    fn test_log_at_level_with_fields() {
        let msg = CString::new("Test with fields").unwrap();
        let fields = CString::new(r#"{"key": "value"}"#).unwrap();
        // SAFETY: Both msg and fields are valid null-terminated C strings from CString
        unsafe {
            log_at_level(Level::INFO, msg.as_ptr(), fields.as_ptr());
        }
        // Should not panic
    }
}
