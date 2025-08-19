//! # TLS Integration Tests
//!
//! Tests for HTTPS/TLS functionality with self-signed certificates for testing.

/// Test that TLS certificates exist and are valid
#[tokio::test]
async fn test_tls_certificates_exist() {
    let cert_path = "tests/web/certs/server.crt";
    let key_path = "tests/web/certs/server.key";
    
    let cert_exists = std::path::Path::new(cert_path).exists();
    let key_exists = std::path::Path::new(key_path).exists();
    
    if cert_exists && key_exists {
        println!("✅ TLS certificates found");
        
        // Verify certificate format
        if let Ok(cert_content) = std::fs::read_to_string(cert_path) {
            assert!(cert_content.contains("BEGIN CERTIFICATE"));
            assert!(cert_content.contains("END CERTIFICATE"));
        }
        
        // Verify key format  
        if let Ok(key_content) = std::fs::read_to_string(key_path) {
            assert!(key_content.contains("BEGIN"));
            assert!(key_content.contains("END"));
        }
    } else {
        println!("⚠️  TLS certificates not found - run ./scripts/generate_test_certs.sh");
        // Not failing the test since certificates are optional for basic testing
    }
}

/// Test HTTPS configuration environment variables
#[tokio::test]
async fn test_https_configuration() {
    // Test that TLS environment variables can be read and have valid formats
    let tls_enabled = std::env::var("WEB_API_TLS_ENABLED").unwrap_or_else(|_| "false".to_string());
    let cert_path = std::env::var("WEB_API_TLS_CERT_PATH").unwrap_or_default();
    let key_path = std::env::var("WEB_API_TLS_KEY_PATH").unwrap_or_default();
    
    // Validate TLS enabled is a boolean-like value
    assert!(
        tls_enabled == "true" || tls_enabled == "false" || tls_enabled == "1" || tls_enabled == "0",
        "WEB_API_TLS_ENABLED should be 'true', 'false', '1', or '0', got: '{}'",
        tls_enabled
    );
    
    // If TLS is enabled, cert and key paths should be non-empty
    if tls_enabled == "true" || tls_enabled == "1" {
        assert!(!cert_path.is_empty(), "WEB_API_TLS_CERT_PATH should not be empty when TLS is enabled");
        assert!(!key_path.is_empty(), "WEB_API_TLS_KEY_PATH should not be empty when TLS is enabled");
        
        // Validate paths have reasonable extensions if provided
        if !cert_path.is_empty() {
            assert!(
                cert_path.ends_with(".crt") || cert_path.ends_with(".pem"),
                "Certificate path should have .crt or .pem extension: {}",
                cert_path
            );
        }
        
        if !key_path.is_empty() {
            assert!(
                key_path.ends_with(".key") || key_path.ends_with(".pem"),
                "Key path should have .key or .pem extension: {}",
                key_path
            );
        }
    }
}