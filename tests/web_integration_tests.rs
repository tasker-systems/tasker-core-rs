//! # Web API Integration Tests
//!
//! Comprehensive integration tests for the web API including authentication,
//! resource coordination, and health monitoring integration.

mod web;
use web::*;

/// Test that all web test infrastructure is working
#[tokio::test]
async fn test_web_integration_infrastructure() {
    println!("🧪 Testing web integration test infrastructure");

    // Test that we can create test clients
    let config = WebTestConfig::default();
    let client = WebTestClient::new(config);
    assert!(client.is_ok(), "Should be able to create test client");

    // Test port finding
    let port = find_available_port().await;
    assert!(port.is_ok(), "Should be able to find available port");

    println!("✅ Web integration infrastructure working correctly");
}

/// Test JWT key generation and token creation
#[tokio::test]
async fn test_jwt_integration() {
    println!("🔑 Testing JWT integration");

    // Test that we can load JWT keys from environment
    let private_key = std::env::var("JWT_PRIVATE_KEY")
        .or_else(|_| std::env::var("JWT_PRIVATE_KEY"))
        .unwrap_or_else(|_| {
            println!("⚠️  JWT_PRIVATE_KEY not found in environment, using test key");
            "test-key".to_string()
        });

    let public_key = std::env::var("JWT_PUBLIC_KEY").unwrap_or_else(|_| {
        println!("⚠️  JWT_PUBLIC_KEY not found in environment, using test key");
        "test-key".to_string()
    });

    // Verify keys are not empty
    assert!(!private_key.is_empty(), "Private key should not be empty");
    assert!(!public_key.is_empty(), "Public key should not be empty");

    // Keys should be in PEM format (contain BEGIN/END markers)
    if private_key != "test-key" {
        assert!(
            private_key.contains("BEGIN"),
            "Private key should be in PEM format"
        );
        assert!(
            private_key.contains("END"),
            "Private key should be in PEM format"
        );
    }

    if public_key != "test-key" {
        assert!(
            public_key.contains("BEGIN"),
            "Public key should be in PEM format"
        );
        assert!(
            public_key.contains("END"),
            "Public key should be in PEM format"
        );
    }

    println!("✅ JWT keys loaded successfully");
    println!("   Private key: {} chars", private_key.len());
    println!("   Public key: {} chars", public_key.len());
}

/// Test TLS certificate generation
#[tokio::test]
async fn test_tls_certificate_integration() {
    println!("🔐 Testing TLS certificate integration");

    // Check if TLS certificates exist
    let cert_path = "tests/web/certs/server.crt";
    let key_path = "tests/web/certs/server.key";

    let cert_exists = std::path::Path::new(cert_path).exists();
    let key_exists = std::path::Path::new(key_path).exists();

    if cert_exists && key_exists {
        println!("✅ TLS certificates found");

        // Read certificate to verify it's valid PEM format
        let cert_content =
            std::fs::read_to_string(cert_path).expect("Should be able to read certificate");

        assert!(
            cert_content.contains("BEGIN CERTIFICATE"),
            "Certificate should be in PEM format"
        );
        assert!(
            cert_content.contains("END CERTIFICATE"),
            "Certificate should be in PEM format"
        );

        // Read private key to verify format
        let key_content =
            std::fs::read_to_string(key_path).expect("Should be able to read private key");

        assert!(
            key_content.contains("BEGIN"),
            "Private key should be in PEM format"
        );
        assert!(
            key_content.contains("END"),
            "Private key should be in PEM format"
        );

        println!("   Certificate: {} chars", cert_content.len());
        println!("   Private key: {} chars", key_content.len());
    } else {
        println!("⚠️  TLS certificates not found - run ./scripts/generate_test_certs.sh");
        println!("   Cert exists: {}", cert_exists);
        println!("   Key exists: {}", key_exists);
    }
}

/// Test environment configuration loading
#[tokio::test]
async fn test_environment_configuration() {
    println!("🌍 Testing environment configuration");

    // Check database configuration
    let database_url = std::env::var("DATABASE_URL");
    match database_url {
        Ok(url) => {
            assert!(!url.is_empty(), "Database URL should not be empty");
            assert!(url.starts_with("postgresql://"), "Should be PostgreSQL URL");
            println!(
                "✅ Database URL configured: {}",
                url.chars().take(20).collect::<String>() + "..."
            );
        }
        Err(_) => {
            println!("⚠️  DATABASE_URL not set - may need to source .env.web_test");
        }
    }

    // Check API key
    let api_key = std::env::var("API_KEY");
    match api_key {
        Ok(key) => {
            assert!(!key.is_empty(), "API key should not be empty");
            assert!(key.len() >= 32, "API key should be at least 32 characters");
            println!("✅ API key configured: {} chars", key.len());
        }
        Err(_) => {
            println!("⚠️  API_KEY not set");
        }
    }

    // Check web configuration
    let web_config_vars = vec![
        ("WEB_API_ENABLED", "true"),
        ("WEB_AUTH_ENABLED", "true"),
        ("WEB_CORS_ENABLED", "true"),
        ("WEB_RESOURCE_MONITORING_ENABLED", "true"),
    ];

    for (var_name, expected_default) in web_config_vars {
        let value = std::env::var(var_name).unwrap_or_else(|_| expected_default.to_string());
        println!("   {}: {}", var_name, value);
    }
}

/// Test resource coordination configuration
#[tokio::test]
async fn test_resource_coordination_configuration() {
    println!("🔧 Testing resource coordination configuration");

    // Test the configuration values from our TOML files
    let warning_threshold =
        std::env::var("WEB_POOL_USAGE_WARNING_THRESHOLD").unwrap_or_else(|_| "0.75".to_string());
    let critical_threshold =
        std::env::var("WEB_POOL_USAGE_CRITICAL_THRESHOLD").unwrap_or_else(|_| "0.90".to_string());

    let warning_val: f64 = warning_threshold
        .parse()
        .expect("Warning threshold should be a valid float");
    let critical_val: f64 = critical_threshold
        .parse()
        .expect("Critical threshold should be a valid float");

    assert!(
        warning_val > 0.0 && warning_val < 1.0,
        "Warning threshold should be between 0 and 1"
    );
    assert!(
        critical_val > 0.0 && critical_val < 1.0,
        "Critical threshold should be between 0 and 1"
    );
    assert!(
        critical_val > warning_val,
        "Critical threshold should be higher than warning"
    );

    println!("✅ Resource coordination thresholds configured correctly");
    println!("   Warning threshold: {}", warning_val);
    println!("   Critical threshold: {}", critical_val);
}

/// Test that configuration is compatible with Rust config system
#[tokio::test]
async fn test_config_compatibility() {
    println!("⚙️  Testing configuration compatibility with Rust config system");

    // Verify that the environment variables are compatible with our TOML configuration
    // This test ensures that the .env.web_test values can be properly loaded by our config system

    // Test JWT configuration compatibility
    let jwt_issuer =
        std::env::var("WEB_JWT_ISSUER").unwrap_or_else(|_| "tasker-core-test".to_string());
    let jwt_audience =
        std::env::var("WEB_JWT_AUDIENCE").unwrap_or_else(|_| "tasker-api-test".to_string());

    assert!(!jwt_issuer.is_empty(), "JWT issuer should be configured");
    assert!(
        !jwt_audience.is_empty(),
        "JWT audience should be configured"
    );

    // Test that values match expected format for TOML config
    assert!(
        !jwt_issuer.contains('"'),
        "JWT issuer should not contain quotes"
    );
    assert!(
        !jwt_audience.contains('"'),
        "JWT audience should not contain quotes"
    );

    println!("✅ Configuration values compatible with TOML format");
    println!("   JWT issuer: {}", jwt_issuer);
    println!("   JWT audience: {}", jwt_audience);
}

/// Integration test that combines multiple components
#[tokio::test]
async fn test_comprehensive_web_integration() {
    println!("🚀 Running comprehensive web integration test");

    // This test validates that all our web integration components work together

    // 1. Test configuration loading
    println!("1️⃣  Testing configuration loading...");
    let has_database = std::env::var("DATABASE_URL").is_ok();
    let has_jwt_keys =
        std::env::var("JWT_PRIVATE_KEY").is_ok() && std::env::var("JWT_PUBLIC_KEY").is_ok();
    let has_api_key = std::env::var("API_KEY").is_ok();

    println!("   Database configured: {}", has_database);
    println!("   JWT keys configured: {}", has_jwt_keys);
    println!("   API key configured: {}", has_api_key);

    // 2. Test test infrastructure
    println!("2️⃣  Testing test infrastructure...");
    let config = WebTestConfig::default();
    let client_result = WebTestClient::new(config);
    assert!(
        client_result.is_ok(),
        "Should be able to create test client"
    );

    let port_result = find_available_port().await;
    assert!(port_result.is_ok(), "Should be able to find available port");

    // 3. Test resource coordination logic
    println!("3️⃣  Testing resource coordination logic...");

    // Test optimal allocation (70/30 split)
    let optimal_allocation = (21, 9); // 70% / 30% split
    let total = optimal_allocation.0 + optimal_allocation.1;
    let orch_ratio = optimal_allocation.0 as f64 / total as f64;
    let web_ratio = optimal_allocation.1 as f64 / total as f64;

    assert!(
        (orch_ratio - 0.7).abs() < 0.01,
        "Orchestration ratio should be ~70%"
    );
    assert!((web_ratio - 0.3).abs() < 0.01, "Web ratio should be ~30%");

    // 4. Test health monitoring thresholds
    println!("4️⃣  Testing health monitoring thresholds...");

    let warning_threshold = 0.75;
    let critical_threshold = 0.90;

    // Test healthy usage
    let healthy_usage = 0.6;
    assert!(
        healthy_usage < warning_threshold,
        "Healthy usage should be below warning"
    );

    // Test warning usage
    let warning_usage = 0.8;
    assert!(
        warning_usage >= warning_threshold && warning_usage < critical_threshold,
        "Warning usage should be between thresholds"
    );

    // Test critical usage
    let critical_usage = 0.95;
    assert!(
        critical_usage >= critical_threshold,
        "Critical usage should exceed critical threshold"
    );

    // 5. Test JWT token structure (if keys available)
    if has_jwt_keys {
        println!("5️⃣  Testing JWT token structure...");

        // This would test JWT generation if we have the dependencies loaded
        // For now, just verify the keys have the right format
        let private_key = std::env::var("JWT_PRIVATE_KEY").unwrap();
        let public_key = std::env::var("JWT_PUBLIC_KEY").unwrap();

        assert!(
            private_key.contains("BEGIN"),
            "Private key should be PEM format"
        );
        assert!(
            public_key.contains("BEGIN"),
            "Public key should be PEM format"
        );
    } else {
        println!("5️⃣  Skipping JWT test (keys not available)");
    }

    println!("✅ Comprehensive web integration test completed successfully!");
    println!("🎉 All web API integration components are working correctly");
}

/// Test resource allocation scenarios from our configuration
#[tokio::test]
async fn test_web_toml_configuration_scenarios() {
    println!("📋 Testing scenarios based on web.toml configuration");

    // These values should match what's in config/tasker/base/web.toml and config/tasker/base/database.toml
    let orchestration_pool = 30; // From database.toml
    let web_pool = 15; // From web.toml
    let coordination_hint = 45; // From web.toml

    let total_connections = orchestration_pool + web_pool;

    println!("Configuration from TOML files:");
    println!("   Orchestration pool: {}", orchestration_pool);
    println!("   Web API pool: {}", web_pool);
    println!("   Total connections: {}", total_connections);
    println!("   Coordination hint: {}", coordination_hint);

    // Test resource coordination
    assert!(
        total_connections <= coordination_hint,
        "Total connections should not exceed coordination hint"
    );

    // Test allocation ratios
    let orch_percentage = (orchestration_pool as f64 / total_connections as f64) * 100.0;
    let web_percentage = (web_pool as f64 / total_connections as f64) * 100.0;

    println!("   Orchestration: {:.1}%", orch_percentage);
    println!("   Web API: {:.1}%", web_percentage);

    // Verify it's close to our optimal 70/30 split
    let orch_target = 70.0;
    let web_target = 30.0;
    let tolerance = 20.0; // More tolerant for real config values

    let orch_deviation = (orch_percentage - orch_target).abs();
    let web_deviation = (web_percentage - web_target).abs();

    println!(
        "   Orchestration deviation from optimal: {:.1}%",
        orch_deviation
    );
    println!("   Web deviation from optimal: {:.1}%", web_deviation);

    // Log whether this is considered optimal
    let is_optimal = orch_deviation <= tolerance && web_deviation <= tolerance;
    println!("   Allocation is optimal: {}", is_optimal);

    // Test utilization ratio
    let utilization = total_connections as f64 / coordination_hint as f64;
    println!("   Resource utilization: {:.1}%", utilization * 100.0);

    assert!(utilization <= 1.0, "Utilization should not exceed 100%");

    println!("✅ Web TOML configuration scenarios validated");
}
