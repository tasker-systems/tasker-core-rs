//! Tests for configuration secret redaction
//!
//! Verifies that sensitive values are properly redacted from configuration
//! responses while preserving non-sensitive data.

use serde_json::json;
use tasker_shared::types::api::orchestration::redact_secrets;

#[test]
fn test_redact_database_url() {
    let config = json!({
        "database": {
            "url": "postgresql://user:password@localhost/db",
            "adapter": "postgresql"
        }
    });

    let (redacted, redacted_fields) = redact_secrets(config);

    // URL should be redacted
    assert_eq!(redacted["database"]["url"], json!("***REDACTED***"));

    // Non-sensitive field should be preserved
    assert_eq!(redacted["database"]["adapter"], json!("postgresql"));

    // Should track which fields were redacted
    assert!(redacted_fields.contains(&"database.url".to_string()));
    assert_eq!(redacted_fields.len(), 1);
}

#[test]
fn test_redact_api_keys_and_secrets() {
    let config = json!({
        "auth": {
            "api_key": "sk_live_1234567890",
            "jwt_private_key": "-----BEGIN PRIVATE KEY-----...",
            "jwt_public_key": "-----BEGIN PUBLIC KEY-----...",
            "enabled": true
        }
    });

    let (redacted, redacted_fields) = redact_secrets(config);

    // All keys should be redacted
    assert_eq!(redacted["auth"]["api_key"], json!("***REDACTED***"));
    assert_eq!(redacted["auth"]["jwt_private_key"], json!("***REDACTED***"));
    assert_eq!(redacted["auth"]["jwt_public_key"], json!("***REDACTED***"));

    // Boolean should not be redacted
    assert_eq!(redacted["auth"]["enabled"], json!(true));

    // Should track all redacted fields
    assert!(redacted_fields.contains(&"auth.api_key".to_string()));
    assert!(redacted_fields.contains(&"auth.jwt_private_key".to_string()));
    assert!(redacted_fields.contains(&"auth.jwt_public_key".to_string()));
    assert_eq!(redacted_fields.len(), 3);
}

#[test]
fn test_redact_empty_values_not_redacted() {
    let config = json!({
        "auth": {
            "api_key": "",
            "jwt_private_key": "",
            "enabled": false
        }
    });

    let (redacted, redacted_fields) = redact_secrets(config);

    // Empty strings should not be redacted
    assert_eq!(redacted["auth"]["api_key"], json!(""));
    assert_eq!(redacted["auth"]["jwt_private_key"], json!(""));

    // Booleans should never be redacted
    assert_eq!(redacted["auth"]["enabled"], json!(false));

    // No fields should be marked as redacted
    assert!(redacted_fields.is_empty());
}

#[test]
fn test_redact_nested_secrets() {
    let config = json!({
        "database": {
            "primary": {
                "url": "postgresql://user:password@primary/db",
                "password": "secret123"
            },
            "secondary": {
                "url": "postgresql://user:password@secondary/db",
                "password": "secret456"
            }
        }
    });

    let (redacted, redacted_fields) = redact_secrets(config);

    // All passwords and URLs should be redacted
    assert_eq!(
        redacted["database"]["primary"]["url"],
        json!("***REDACTED***")
    );
    assert_eq!(
        redacted["database"]["primary"]["password"],
        json!("***REDACTED***")
    );
    assert_eq!(
        redacted["database"]["secondary"]["url"],
        json!("***REDACTED***")
    );
    assert_eq!(
        redacted["database"]["secondary"]["password"],
        json!("***REDACTED***")
    );

    // Should track all redacted fields with full paths
    assert!(redacted_fields.contains(&"database.primary.url".to_string()));
    assert!(redacted_fields.contains(&"database.primary.password".to_string()));
    assert!(redacted_fields.contains(&"database.secondary.url".to_string()));
    assert!(redacted_fields.contains(&"database.secondary.password".to_string()));
    assert_eq!(redacted_fields.len(), 4);
}

#[test]
fn test_redact_arrays_with_objects() {
    // Test that objects inside arrays are recursively processed
    let config = json!({
        "services": [
            {
                "name": "service1",
                "token": "token123",
                "enabled": true
            },
            {
                "name": "service2",
                "api_key": "key456",
                "enabled": false
            }
        ]
    });

    let (redacted, redacted_fields) = redact_secrets(config);

    // Tokens and API keys in array items should be redacted
    assert_eq!(
        redacted["services"][0]["token"],
        json!("***REDACTED***"),
        "Token in first array item should be redacted"
    );
    assert_eq!(
        redacted["services"][1]["api_key"],
        json!("***REDACTED***"),
        "API key in second array item should be redacted"
    );

    // Non-sensitive fields should be preserved
    assert_eq!(redacted["services"][0]["name"], json!("service1"));
    assert_eq!(redacted["services"][1]["name"], json!("service2"));
    assert_eq!(redacted["services"][0]["enabled"], json!(true));
    assert_eq!(redacted["services"][1]["enabled"], json!(false));

    // Should have 2 redacted fields
    assert_eq!(redacted_fields.len(), 2);
}

#[test]
fn test_redact_preserves_structure() {
    let config = json!({
        "system": {
            "name": "tasker",
            "version": "1.0.0"
        },
        "database": {
            "url": "postgresql://localhost/db",
            "max_connections": 20
        },
        "auth": {
            "secret": "mysecret",
            "timeout": 3600
        }
    });

    let (redacted, _) = redact_secrets(config);

    // Structure should be preserved
    assert!(redacted.get("system").is_some());
    assert!(redacted.get("database").is_some());
    assert!(redacted.get("auth").is_some());

    // Non-sensitive values should be intact
    assert_eq!(redacted["system"]["name"], json!("tasker"));
    assert_eq!(redacted["system"]["version"], json!("1.0.0"));
    assert_eq!(redacted["database"]["max_connections"], json!(20));
    assert_eq!(redacted["auth"]["timeout"], json!(3600));

    // Sensitive values should be redacted
    assert_eq!(redacted["database"]["url"], json!("***REDACTED***"));
    assert_eq!(redacted["auth"]["secret"], json!("***REDACTED***"));
}

#[test]
fn test_case_insensitive_key_matching() {
    let config = json!({
        "API_KEY": "key123",
        "Secret_Token": "token456",
        "database_PASSWORD": "pass789"
    });

    let (redacted, redacted_fields) = redact_secrets(config);

    // All should be redacted regardless of case
    assert_eq!(redacted["API_KEY"], json!("***REDACTED***"));
    assert_eq!(redacted["Secret_Token"], json!("***REDACTED***"));
    assert_eq!(redacted["database_PASSWORD"], json!("***REDACTED***"));

    assert_eq!(redacted_fields.len(), 3);
}
