//! Shared TOML Merge Utilities
//!
//! Provides deep merge functionality for TOML configurations, used by both
//! ConfigMerger (CLI tools) and UnifiedConfigLoader (runtime loading).
//!
//! ## Deep Merge Semantics
//!
//! - **Tables**: Recursively merge nested tables, preserving keys from both base and overlay
//! - **Scalars**: Overlay value replaces base value
//! - **Arrays**: Overlay value replaces base value (no array merging)
//! - **New Keys**: Keys only in overlay are inserted into base
//!
//! ## Example
//!
//! ```rust
//! use tasker_shared::config::merge::deep_merge_toml;
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let base = toml::from_str(r#"
//! [database]
//! url = "postgres://localhost"
//! [database.pool]
//! max_connections = 10
//! "#)?;
//!
//! let overlay = toml::from_str(r#"
//! [database.pool]
//! max_connections = 50
//! min_connections = 5
//! "#)?;
//!
//! let mut merged = base;
//! deep_merge_toml(&mut merged, overlay)?;
//!
//! // Result: database.url preserved, pool.max_connections = 50, pool.min_connections = 5
//! # Ok(())
//! # }
//! ```

use crate::config::error::ConfigResult;

/// Deep merge two TOML values
///
/// If both values are tables, recursively merge them.
/// Otherwise, the overlay value replaces the base value.
///
/// # Arguments
/// * `base` - Mutable base configuration to merge into
/// * `overlay` - Overlay configuration that takes precedence
///
/// # Returns
/// * `Ok(())` on success
/// * `Err(ConfigurationError)` if merge fails (currently infallible, but returns Result for future extensibility)
///
/// # Examples
///
/// ```rust
/// use tasker_shared::config::merge::deep_merge_toml;
///
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let mut base = toml::from_str(r#"
/// [mpsc_channels.orchestration]
/// command_buffer_size = 1000
/// "#)?;
///
/// let overlay = toml::from_str(r#"
/// [mpsc_channels.worker]
/// command_buffer_size = 500
/// "#)?;
///
/// deep_merge_toml(&mut base, overlay)?;
/// // Result: Both orchestration and worker channels preserved
/// # Ok(())
/// # }
/// ```
pub fn deep_merge_toml(base: &mut toml::Value, overlay: toml::Value) -> ConfigResult<()> {
    match (base, overlay) {
        (toml::Value::Table(base_table), toml::Value::Table(overlay_table)) => {
            // Both are tables - recursively merge
            deep_merge_tables(base_table, overlay_table);
            // Strip _docs sections from merged result
            strip_docs_sections(base_table);
        }
        (base_value, overlay_value) => {
            // Non-table values: overlay replaces base entirely
            *base_value = overlay_value;
        }
    }
    Ok(())
}

/// Strip documentation sections from a TOML table
///
/// Recursively removes all keys ending with `_docs` OR equal to `_docs` from the table and its nested tables.
/// This ensures that documentation metadata is not included in merged configurations.
///
/// Examples of stripped keys:
/// - `_docs` (exact match)
/// - `field_docs` (ends with _docs)
/// - Nested sections like `database.pool._docs.max_connections`
fn strip_docs_sections(table: &mut toml::value::Table) {
    // Remove all keys that are "_docs" or end with "_docs"
    table.retain(|key, _| key != "_docs" && !key.ends_with("_docs"));

    // Recursively strip from nested tables
    for (_key, value) in table.iter_mut() {
        if let toml::Value::Table(nested_table) = value {
            strip_docs_sections(nested_table);
        }
    }
}

/// Deep merge two TOML tables (internal implementation)
///
/// This is the core recursive merge logic. It handles:
/// - Recursive table merging when both base and overlay have the same key with table values
/// - Value replacement when keys exist in both but are not both tables
/// - Insertion of new keys from overlay that don't exist in base
/// - **Skips** keys ending with `_docs` (documentation sections)
///
/// # Arguments
/// * `base` - Mutable base table to merge into
/// * `overlay` - Overlay table whose values take precedence
fn deep_merge_tables(base: &mut toml::value::Table, overlay: toml::value::Table) {
    for (key, value) in overlay {
        // Skip documentation sections (keys ending with _docs)
        if key.ends_with("_docs") {
            continue;
        }

        match base.get_mut(&key) {
            Some(base_value) => {
                // Both base and overlay have this key
                // Check if both are tables before consuming them
                let both_are_tables = matches!(
                    (&*base_value, &value),
                    (toml::Value::Table(_), toml::Value::Table(_))
                );

                if both_are_tables {
                    // Both are tables - recursively merge
                    if let toml::Value::Table(base_table) = base_value {
                        if let toml::Value::Table(overlay_table) = value {
                            deep_merge_tables(base_table, overlay_table);
                        }
                    }
                } else {
                    // Not both tables - overlay wins
                    *base_value = value;
                }
            }
            None => {
                // Key only in overlay - insert it
                base.insert(key, value);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_deep_merge_nested_tables() {
        let mut base = toml::from_str(
            r#"
[database]
url = "postgres://localhost"
[database.pool]
max_connections = 10
min_connections = 2
"#,
        )
        .unwrap();

        let overlay = toml::from_str(
            r#"
[database.pool]
max_connections = 50
idle_timeout = 300
"#,
        )
        .unwrap();

        deep_merge_toml(&mut base, overlay).unwrap();

        let result = base.as_table().unwrap();
        let db = result.get("database").unwrap().as_table().unwrap();
        let pool = db.get("pool").unwrap().as_table().unwrap();

        // Overlay value took precedence
        assert_eq!(
            pool.get("max_connections").unwrap().as_integer().unwrap(),
            50
        );
        // Base value preserved
        assert_eq!(
            pool.get("min_connections").unwrap().as_integer().unwrap(),
            2
        );
        // New key from overlay added
        assert_eq!(pool.get("idle_timeout").unwrap().as_integer().unwrap(), 300);
        // Top-level key from base preserved
        assert_eq!(
            db.get("url").unwrap().as_str().unwrap(),
            "postgres://localhost"
        );
    }

    #[test]
    fn test_deep_merge_non_overlapping_tables() {
        let mut base = toml::from_str(
            r#"
[mpsc_channels.orchestration]
command_buffer_size = 1000
"#,
        )
        .unwrap();

        let overlay = toml::from_str(
            r#"
[mpsc_channels.worker]
command_buffer_size = 500
"#,
        )
        .unwrap();

        deep_merge_toml(&mut base, overlay).unwrap();

        let result = base.as_table().unwrap();
        let channels = result.get("mpsc_channels").unwrap().as_table().unwrap();

        // Both orchestration and worker preserved
        assert!(channels.contains_key("orchestration"));
        assert!(channels.contains_key("worker"));

        assert_eq!(
            channels
                .get("orchestration")
                .unwrap()
                .as_table()
                .unwrap()
                .get("command_buffer_size")
                .unwrap()
                .as_integer()
                .unwrap(),
            1000
        );
        assert_eq!(
            channels
                .get("worker")
                .unwrap()
                .as_table()
                .unwrap()
                .get("command_buffer_size")
                .unwrap()
                .as_integer()
                .unwrap(),
            500
        );
    }

    #[test]
    fn test_deep_merge_scalar_override() {
        let mut base = toml::from_str(r#"environment = "development""#).unwrap();
        let overlay = toml::from_str(r#"environment = "production""#).unwrap();

        deep_merge_toml(&mut base, overlay).unwrap();

        let result = base.as_table().unwrap();
        assert_eq!(
            result.get("environment").unwrap().as_str().unwrap(),
            "production"
        );
    }

    #[test]
    fn test_deep_merge_new_top_level_keys() {
        let mut base = toml::from_str(
            r#"[database]
url = "postgres://localhost"
"#,
        )
        .unwrap();

        let overlay = toml::from_str(
            r#"
[telemetry]
enabled = true
"#,
        )
        .unwrap();

        deep_merge_toml(&mut base, overlay).unwrap();

        let result = base.as_table().unwrap();
        assert!(result.contains_key("database"));
        assert!(result.contains_key("telemetry"));
    }

    #[test]
    fn test_deep_merge_array_replacement() {
        let mut base = toml::from_str(r#"values = [1, 2, 3]"#).unwrap();
        let overlay = toml::from_str(r#"values = [4, 5]"#).unwrap();

        deep_merge_toml(&mut base, overlay).unwrap();

        let result = base.as_table().unwrap();
        let values = result.get("values").unwrap().as_array().unwrap();
        // Arrays are replaced, not merged
        assert_eq!(values.len(), 2);
        assert_eq!(values[0].as_integer().unwrap(), 4);
        assert_eq!(values[1].as_integer().unwrap(), 5);
    }

    #[test]
    fn test_docs_stripping() {
        let mut config = toml::from_str(
            r#"
[database.pool]
max_connections = 30

[database.pool._docs]
description = "Pool configuration"

[database.pool._docs.max_connections]
description = "Max connections field"
"#,
        )
        .unwrap();

        deep_merge_toml(&mut config, toml::Value::Table(toml::value::Table::new())).unwrap();

        let result_string = toml::to_string(&config).unwrap();
        // Should not contain _docs
        assert!(
            !result_string.contains("_docs"),
            "Config should not contain _docs sections:\n{}",
            result_string
        );
    }
}
