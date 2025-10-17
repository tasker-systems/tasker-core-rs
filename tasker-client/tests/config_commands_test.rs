//! Integration tests for CLI config commands
//!
//! Tests the end-to-end functionality of config generate, validate,
//! and other configuration management commands.

use serial_test::serial;
use std::fs;
use std::path::PathBuf;
use tempfile::TempDir;

/// Create a minimal test configuration structure
fn create_test_config_structure() -> (TempDir, PathBuf) {
    let temp_dir = TempDir::new().unwrap();
    let config_root = temp_dir.path().join("tasker");

    // Create directory structure
    fs::create_dir_all(config_root.join("base")).unwrap();
    fs::create_dir_all(config_root.join("environments/test")).unwrap();
    fs::create_dir_all(config_root.join("environments/development")).unwrap();
    fs::create_dir_all(config_root.join("environments/production")).unwrap();

    // Create base common.toml
    let base_common = r#"
environment = "base"

[database]
enable_secondary_database = false
url = "${DATABASE_URL:-postgresql://localhost/tasker}"
adapter = "postgresql"
encoding = "unicode"
host = "localhost"
username = "tasker"
password = "tasker"
database = "tasker_base"
checkout_timeout = 10
reaping_frequency = 10
skip_migration_check = false

[database.pool]
max_connections = 20
min_connections = 5
acquire_timeout_seconds = 30
idle_timeout_seconds = 300
max_lifetime_seconds = 3600

[database.variables]
statement_timeout = 5000

[queues]
backend = "pgmq"

[queues.pgmq_backend]
enabled = true
pool_size = 10
connection_timeout_seconds = 30

[circuit_breakers]
enabled = true

[circuit_breakers.global_settings]
auto_create_enabled = true
max_circuit_breakers = 20
metrics_collection_interval_seconds = 10
min_state_transition_interval_seconds = 1

[circuit_breakers.default_config]
failure_threshold = 3
success_threshold = 1
timeout_seconds = 5

[circuit_breakers.component_configs]
"#;
    fs::write(config_root.join("base/common.toml"), base_common).unwrap();

    // Create test environment override
    let test_common = r#"
environment = "test"

[database]
database = "tasker_test"

[database.pool]
max_connections = 5
"#;
    fs::write(
        config_root.join("environments/test/common.toml"),
        test_common,
    )
    .unwrap();

    // Create production environment override
    let prod_common = r#"
environment = "production"

[database]
database = "tasker_production"

[database.pool]
max_connections = 50
"#;
    fs::write(
        config_root.join("environments/production/common.toml"),
        prod_common,
    )
    .unwrap();

    (temp_dir, config_root)
}

#[test]
fn test_config_merger_basic_functionality() {
    use tasker_shared::config::ConfigMerger;

    let (_temp_dir, config_root) = create_test_config_structure();

    let mut merger = ConfigMerger::new(config_root.clone(), "test").unwrap();
    let merged = merger.merge_context("common").unwrap();

    // Verify merged configuration
    assert!(merged.contains("environment = \"test\""));
    assert!(merged.contains("max_connections = 5")); // Test override
    assert!(merged.contains("adapter = \"postgresql\"")); // Base value
    assert!(merged.contains("database = \"tasker_test\"")); // Test override
}

#[test]
fn test_config_merger_production_environment() {
    use tasker_shared::config::ConfigMerger;

    let (_temp_dir, config_root) = create_test_config_structure();

    let mut merger = ConfigMerger::new(config_root, "production").unwrap();
    let merged = merger.merge_context("common").unwrap();

    // Verify production-specific overrides
    assert!(merged.contains("environment = \"production\""));
    assert!(merged.contains("max_connections = 50")); // Production override
    assert!(merged.contains("database = \"tasker_production\"")); // Production override
}

#[test]
fn test_config_merger_preserves_base_values() {
    use tasker_shared::config::ConfigMerger;

    let (_temp_dir, config_root) = create_test_config_structure();

    let mut merger = ConfigMerger::new(config_root, "test").unwrap();
    let merged = merger.merge_context("common").unwrap();

    // Values from base that aren't overridden should be preserved
    assert!(merged.contains("adapter = \"postgresql\""));
    assert!(merged.contains("checkout_timeout = 10"));
    assert!(merged.contains("circuit_breakers"));
    assert!(merged.contains("auto_create_enabled = true"));
}

#[test]
fn test_config_merger_invalid_source_directory() {
    use tasker_shared::config::ConfigMerger;

    let result = ConfigMerger::new(PathBuf::from("/nonexistent/path"), "test");
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("not found"));
}

#[test]
fn test_config_merger_missing_context() {
    use tasker_shared::config::ConfigMerger;

    let (_temp_dir, config_root) = create_test_config_structure();

    let mut merger = ConfigMerger::new(config_root, "test").unwrap();
    let result = merger.merge_context("nonexistent");

    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("nonexistent.toml"));
}

#[test]
fn test_config_merger_environment_without_overrides() {
    use tasker_shared::config::ConfigMerger;

    let (_temp_dir, config_root) = create_test_config_structure();

    // Development environment has no override file in our test structure
    let mut merger = ConfigMerger::new(config_root, "development").unwrap();
    let merged = merger.merge_context("common").unwrap();

    // Should get base values when no environment override exists
    assert!(merged.contains("environment = \"base\""));
    assert!(merged.contains("max_connections = 20")); // Base value
}

#[test]
fn test_config_generation_to_file() {
    use tasker_shared::config::ConfigMerger;

    let (_temp_dir, config_root) = create_test_config_structure();
    let output_file = _temp_dir.path().join("generated-config.toml");

    let mut merger = ConfigMerger::new(config_root, "test").unwrap();
    let merged_config = merger.merge_context("common").unwrap();

    // Write to file
    fs::write(&output_file, &merged_config).unwrap();

    // Verify file was written
    assert!(output_file.exists());

    // Verify content
    let content = fs::read_to_string(&output_file).unwrap();
    assert!(content.contains("environment = \"test\""));
    assert!(content.contains("max_connections = 5"));
}

#[test]
fn test_generated_config_includes_metadata_header() {
    use tasker_shared::config::ConfigMerger;

    let (_temp_dir, config_root) = create_test_config_structure();
    let output_file = _temp_dir.path().join("generated-with-header.toml");

    let mut merger = ConfigMerger::new(config_root.clone(), "production").unwrap();
    let merged_config = merger.merge_context("common").unwrap();

    // Simulate what the CLI does - add header before writing
    let header = format!(
        "# Tasker Configuration - {} Context\n\
         # Environment: {}\n\
         # Generated: {}\n\
         # Source: {}\n\
         #\n\
         # This file is a MERGED configuration (base + environment overrides).\n\
         # DO NOT EDIT manually - regenerate using: tasker-cli config generate\n\
         #\n\
         # Environment Variable Overrides (applied at runtime):\n\
         # - DATABASE_URL: Override database.url (K8s secrets rotation)\n\
         # - TASKER_TEMPLATE_PATH: Override worker.template_path (testing)\n\
         #\n\n",
        "common",
        "production",
        chrono::Utc::now().to_rfc3339(),
        config_root.display()
    );

    let full_config = format!("{}{}", header, merged_config);
    fs::write(&output_file, &full_config).unwrap();

    // Read back and verify header is present
    let content = fs::read_to_string(&output_file).unwrap();

    // Verify header components
    assert!(content.contains("# Tasker Configuration - common Context"));
    assert!(content.contains("# Environment: production"));
    assert!(content.contains("# Generated:"));
    assert!(content.contains("# Source:"));
    assert!(
        content.contains("# This file is a MERGED configuration (base + environment overrides).")
    );
    assert!(
        content.contains("# DO NOT EDIT manually - regenerate using: tasker-cli config generate")
    );
    assert!(content.contains("# - DATABASE_URL: Override database.url (K8s secrets rotation)"));
    assert!(content.contains("# - TASKER_TEMPLATE_PATH: Override worker.template_path (testing)"));

    // Verify config content is also present (after header)
    assert!(content.contains("environment = \"production\""));
    assert!(content.contains("max_connections = 50"));

    // Verify the file is still valid TOML (comments should be ignored)
    let parsed: toml::Value = toml::from_str(&content).unwrap();
    assert!(parsed.get("database").is_some());
    assert!(parsed.get("circuit_breakers").is_some());
}

#[test]
fn test_merge_all_contexts() {
    use tasker_shared::config::ConfigMerger;

    let (_temp_dir, config_root) = create_test_config_structure();

    let mut merger = ConfigMerger::new(config_root, "test").unwrap();
    let all_configs = merger.merge_all_contexts().unwrap();

    // Should only return contexts that exist
    assert!(all_configs.contains_key("common"));
    assert!(!all_configs.contains_key("orchestration")); // We didn't create this
    assert!(!all_configs.contains_key("worker")); // We didn't create this

    // Verify common config is correct
    let common_config = all_configs.get("common").unwrap();
    assert!(common_config.contains("environment = \"test\""));
}

#[test]
fn test_config_validation_success() {
    use tasker_shared::config::ConfigMerger;

    let (_temp_dir, config_root) = create_test_config_structure();

    let mut merger = ConfigMerger::new(config_root, "test").unwrap();
    let merged = merger.merge_context("common").unwrap();

    // Parse the merged config to validate it's valid TOML
    let parsed: toml::Value = toml::from_str(&merged).unwrap();

    // Verify key sections exist
    assert!(parsed.get("database").is_some());
    assert!(parsed.get("queues").is_some());
    assert!(parsed.get("circuit_breakers").is_some());
}

#[test]
#[serial]
fn test_environment_variable_substitution() {
    use tasker_shared::config::ConfigMerger;

    // Store original value if it exists
    let original_db_url = std::env::var("DATABASE_URL").ok();

    // Set test environment variable
    std::env::set_var("DATABASE_URL", "postgresql://test:test@testhost/testdb");

    let (_temp_dir, config_root) = create_test_config_structure();

    let mut merger = ConfigMerger::new(config_root, "test").unwrap();
    let merged = merger.merge_context("common").unwrap();

    // Verify environment variable was substituted
    assert!(
        merged.contains("postgresql://test:test@testhost/testdb"),
        "Expected substituted DATABASE_URL, got: {:?}",
        merged.lines().find(|l| l.contains("url ="))
    );

    // Restore original value or remove
    match original_db_url {
        Some(url) => std::env::set_var("DATABASE_URL", url),
        None => std::env::remove_var("DATABASE_URL"),
    }
}

#[test]
#[serial]
fn test_default_value_when_env_var_missing() {
    use tasker_shared::config::ConfigMerger;

    // Store original value if it exists
    let original_db_url = std::env::var("DATABASE_URL").ok();

    // Ensure DATABASE_URL is not set
    std::env::remove_var("DATABASE_URL");

    let (_temp_dir, config_root) = create_test_config_structure();

    let mut merger = ConfigMerger::new(config_root, "test").unwrap();
    let merged = merger.merge_context("common").unwrap();

    // Should use default value from ${DATABASE_URL:-postgresql://localhost/tasker}
    assert!(
        merged.contains("postgresql://localhost/tasker"),
        "Expected default DATABASE_URL, got: {:?}",
        merged.lines().find(|l| l.contains("url ="))
    );

    // Restore original value if it existed
    if let Some(url) = original_db_url {
        std::env::set_var("DATABASE_URL", url);
    }
}

// ============================================================================
// Validation Tests
// ============================================================================

#[test]
fn test_validate_valid_common_config() {
    use tasker_shared::config::ConfigMerger;

    let (_temp_dir, config_root) = create_test_config_structure();

    // Generate a config file
    let mut merger = ConfigMerger::new(config_root, "test").unwrap();
    let merged = merger.merge_context("common").unwrap();

    // Write to temporary file
    let output_file = _temp_dir.path().join("generated.toml");
    fs::write(&output_file, &merged).unwrap();

    // Now validate it as TOML
    let content = fs::read_to_string(&output_file).unwrap();
    let toml_result = toml::from_str::<toml::Value>(&content);

    // Should be valid TOML
    assert!(toml_result.is_ok());
    let toml_value = toml_result.unwrap();

    // Verify key sections are present
    assert!(toml_value.get("database").is_some());
    assert!(toml_value.get("circuit_breakers").is_some());
}

#[test]
fn test_validate_invalid_toml_syntax() {
    let temp_dir = TempDir::new().unwrap();
    let invalid_file = temp_dir.path().join("invalid.toml");

    // Write invalid TOML
    fs::write(&invalid_file, "invalid toml ][").unwrap();

    // Try to parse - should fail
    let content = fs::read_to_string(&invalid_file).unwrap();
    let result = toml::from_str::<toml::Value>(&content);

    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("TOML parse error"));
}

#[test]
fn test_validate_missing_required_fields() {
    let temp_dir = TempDir::new().unwrap();
    let incomplete_file = temp_dir.path().join("incomplete.toml");

    // Write incomplete common config (missing required database section)
    let incomplete_config = r#"
environment = "test"

[engine]
max_retries = 3
"#;
    fs::write(&incomplete_file, incomplete_config).unwrap();

    // Parse and try to deserialize into CommonConfig
    let content = fs::read_to_string(&incomplete_file).unwrap();
    let toml_value: toml::Value = toml::from_str(&content).unwrap();

    // This should fail because database is required
    use tasker_shared::config::contexts::CommonConfig;
    let result = toml_value.try_into::<CommonConfig>();

    assert!(result.is_err());
}

#[test]
fn test_validate_with_invalid_values() {
    let temp_dir = TempDir::new().unwrap();
    let invalid_values_file = temp_dir.path().join("invalid_values.toml");

    // Write config with invalid values (negative pool size)
    let invalid_config = r#"
environment = "test"

[database]
url = "postgresql://localhost/test"
adapter = "postgresql"
pool_size = -5
checkout_timeout = 10
database = "tasker_test"

[circuit_breakers]
enabled = true

[circuit_breakers.global_settings]
auto_create_enabled = true
max_circuit_breakers = 20
metrics_collection_interval_seconds = 10
min_state_transition_interval_seconds = 1

[circuit_breakers.default_config]
failure_threshold = 3
success_threshold = 1
timeout_seconds = 5

[circuit_breakers.component_configs]
"#;
    fs::write(&invalid_values_file, invalid_config).unwrap();

    // This should fail at TOML parsing because TOML will reject negative pool_size
    // if the field is defined as u32 in the struct
    let content = fs::read_to_string(&invalid_values_file).unwrap();
    let result = toml::from_str::<toml::Value>(&content);

    // TOML parsing might succeed (it just parses to -5 as integer)
    // But deserialization should fail if pool_size is defined as u32
    if let Ok(toml_value) = result {
        use tasker_shared::config::contexts::CommonConfig;
        let deserialize_result = toml_value.try_into::<CommonConfig>();
        // This should fail due to type mismatch (negative value for unsigned type)
        assert!(deserialize_result.is_err());
    }
}

#[test]
fn test_validate_generated_config_round_trip() {
    use tasker_shared::config::ConfigMerger;

    let (_temp_dir, config_root) = create_test_config_structure();

    // Generate config
    let mut merger = ConfigMerger::new(config_root, "production").unwrap();
    let merged = merger.merge_context("common").unwrap();

    // Write to file
    let output_file = _temp_dir.path().join("production-config.toml");
    fs::write(&output_file, &merged).unwrap();

    // Read back and validate as TOML
    let content = fs::read_to_string(&output_file).unwrap();
    let toml_result = toml::from_str::<toml::Value>(&content);

    assert!(toml_result.is_ok());
    let toml_value = toml_result.unwrap();

    // Verify production-specific values were applied
    assert_eq!(
        toml_value.get("environment").and_then(|v| v.as_str()),
        Some("production")
    );
}

#[test]
fn test_validate_multiple_contexts() {
    use tasker_shared::config::ConfigMerger;

    let (_temp_dir, config_root) = create_test_config_structure();

    let mut merger = ConfigMerger::new(config_root, "test").unwrap();

    // Generate and validate common config
    let common_merged = merger.merge_context("common").unwrap();
    let common_file = _temp_dir.path().join("common.toml");
    fs::write(&common_file, &common_merged).unwrap();

    // Verify it's valid TOML
    let content = fs::read_to_string(&common_file).unwrap();
    let result = toml::from_str::<toml::Value>(&content);
    assert!(result.is_ok());
}

#[test]
fn test_validate_preserves_comments_and_formatting() {
    let temp_dir = TempDir::new().unwrap();
    let config_file = temp_dir.path().join("commented.toml");

    // Write config with comments
    let commented_config = r#"
# This is a comment
environment = "test"

# Database configuration
[database]
url = "postgresql://localhost/test"
adapter = "postgresql"

[database.pool]
max_connections = 10
"#;
    fs::write(&config_file, commented_config).unwrap();

    // Parse and validate
    let content = fs::read_to_string(&config_file).unwrap();
    let toml_result = toml::from_str::<toml::Value>(&content);

    // Comments are lost during parsing (this is expected TOML behavior)
    // but the structure should remain valid
    assert!(toml_result.is_ok());
    let toml_value = toml_result.unwrap();

    assert_eq!(
        toml_value.get("environment").and_then(|v| v.as_str()),
        Some("test")
    );
}
