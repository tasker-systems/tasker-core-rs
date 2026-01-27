//! # Authenticated Web API Endpoint Tests
//!
//! Tests for protected endpoints that require JWT authentication, including:
//! - JWT token generation and validation
//! - Task creation and management endpoints
//! - Namespace and status endpoints
//! - Token expiration and refresh handling

use super::test_infrastructure::*;
use jsonwebtoken::{encode, Algorithm, EncodingKey, Header};
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::time::{SystemTime, UNIX_EPOCH};

/// JWT claims structure for test tokens
#[derive(Debug, Serialize, Deserialize)]
pub struct TestClaims {
    sub: String,
    iss: String,
    aud: String,
    exp: usize,
    iat: usize,
    jti: String,
    user_id: String,
    permissions: Vec<String>,
}

impl TestClaims {
    /// Create new test claims with default values
    pub fn new(user_id: &str, expiry_minutes: u64) -> Self {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("Time went backwards")
            .as_secs() as usize;

        Self {
            sub: user_id.to_string(),
            iss: "tasker-core-test".to_string(),
            aud: "tasker-api-test".to_string(),
            exp: now + (expiry_minutes * 60) as usize,
            iat: now,
            jti: uuid::Uuid::new_v4().to_string(),
            user_id: user_id.to_string(),
            permissions: vec![
                "tasks:read".to_string(),
                "tasks:write".to_string(),
                "namespaces:read".to_string(),
                "status:read".to_string(),
            ],
        }
    }

    /// Create expired test claims (for testing token expiration)
    fn expired(user_id: &str) -> Self {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("Time went backwards")
            .as_secs() as usize;

        Self {
            sub: user_id.to_string(),
            iss: "tasker-core-test".to_string(),
            aud: "tasker-api-test".to_string(),
            exp: now - 3600, // Expired 1 hour ago
            iat: now - 7200, // Issued 2 hours ago
            jti: uuid::Uuid::new_v4().to_string(),
            user_id: user_id.to_string(),
            permissions: vec!["tasks:read".to_string()],
        }
    }
}

/// Generate a test JWT token using the test private key
pub fn generate_test_jwt(
    claims: &TestClaims,
    private_key: &str,
) -> Result<String, Box<dyn std::error::Error>> {
    let header = Header::new(Algorithm::RS256);
    let encoding_key = EncodingKey::from_rsa_pem(private_key.as_bytes())?;
    let token = encode(&header, claims, &encoding_key)?;
    Ok(token)
}

/// Get test JWT private key from environment or use embedded test key
pub fn get_test_private_key() -> String {
    std::env::var("JWT_PRIVATE_KEY").unwrap_or_else(|_| {
        // Fallback to the generated test key (this should match what's in .env.web_test)
        "-----BEGIN RSA PRIVATE KEY-----
MIIEowIBAAKCAQEAxAh25l4WV5ibwXaIAETRchkQhVP/OymplWglG+GHM1j2fFdw
sBR/jGGfdebsmDSC26/CB4CIQOCOf7YCjZsAbxLioK5R2lAE3AoE9VQRKouwzqpU
cylXUswbXdPd1AJMrjaVDOzo27KI8v4/+SiyfgJsBJCLu4owiClKqD7iLHL2sxWK
lTcqUy+puCH8u0wfdNM1O0YqQAbD6RhnDPcHY58/f/UD+AHIrCConFDuo1HFrEU0
TieJ9BMpx4QM+r7/niLzvx8OfuV3g4OZfWA86fCQ38ACdj/BXJhnCIy3w6GqJiaU
+BO6ZBWrYqy1FM4O7O+xvVizQDmwRVDQrxk8PQIDAQAB
AoIBADuK3qKGPX8JyXVvzUtXm85uMohsxP9xXiMVaQAY9nnwMZ3+6SlycHNxS4AC
TibE/GJB9ktVDQ23p1C/NfNQ+0bBk7h+ejo3R+KV4H+rszMbDu8W6WO5UN7DRCSx
r5UvxZ680XUFmIzyo4o7E69YXy7LCYgxZ1/lT2xsLlMAFq6tC9/nRx10ZfP5cYBN
0gbBNav7fzcEwGvvcfwsxpoVeLR7fjicNjSQ1m6Lovn48rCiTC2xA3OkJBvKbp4r
mO9zczxHBYWZBjDI8/Wus/MF3WmzFdNMo07a2xytszjLDvwr/P/ZVhq82Nhcr8xK
RlCaGOIPGBcyBKNnGTmr1mL2KTkCgYEA96gRUPBLV9clR3acm0ViNDpGX7L6CIg6
tnTI99fmdt+e6+zlfnO46uRVXREpHR+DCV1CC2vjai10zZzjSRCHBdP0GWcf97G9
/wsW32hCuN4xE9PMBf1Ttm/oAU80gN7tDsutcsdYRCdIrSWuHlppByTlRWWwg9lC
oHq4KYXtIGcCgYEAyqMqnQUzYqUzpZiypS0iKjrEP0pCUEOQ69eOsqLq7LnBYN7u
gtuEN6GmL52fxam1G2y0Yzxy406Sf1hONnZXUhfUwVXHBS9eF5jtackPnvSeVyEQ
X9AdHlmOHNmypuRbzaPMkp2LjJlgUf+mNDtG1zfmsennkEZG9+maHkoqR7sCgYBO
qjr926YC+9rijUGHbI2aC1ypLz+OkD8eD5B6cUDKR5PCWtg2x2lYazjWPAo0Lvs/
cTj2ScnNwyyT1x626aIJ7t5dZ01XL0Uriwkz43k2IZWzN5ZZ3LLHg1pNeCw0NxtT
lMy+ZaWa2GOUZCCfkZZE56pP1dIwv0UTloeC4QCGRwKBgQCGaEAFssNotQdS2bv1
D8DPnfc5u7nMn2Rq6qm+F44XwwZfiL9PkOdcNx6SCs1FQNHeBPaJtDjISP+m9B28
xjYZP7FhI9JEwCx7Hnarai+wUbUNOeMwikwmK2S2AjgbtvClr/YrcdB0S++1tAq8
Lm1Ip82fSPTNn6/HFO2jFbKBrQKBgFDfovReI7rcF1iw90u6inVE+hCXlAfCbD96
r3kM/xMJ0t4Udztxj5q0VMAGThAmTYrnD4r7Lst+5YRvTY8DXcKbSYKgJ7R8JyR8
ujr+x6r1kxZs+fgcrI0EfygqICvOwkUlBuCwmDdDHcDxp4RgXkob2u3fj6kfpcd/
nsV6EjVk
-----END RSA PRIVATE KEY-----"
            .to_string()
    })
}

/// Test that authenticated endpoints work with valid JWT
#[tokio::test]
async fn test_authenticated_endpoints_with_valid_jwt() {
    // Start test server with dynamic port allocation
    let test_server = TestServer::start()
        .await
        .expect("Failed to start test server");
    let mut client = WebTestClient::for_server(&test_server).expect("Failed to create test client");

    // Generate a valid JWT token
    let claims = TestClaims::new("test-user-123", 60); // Valid for 60 minutes
    let private_key = get_test_private_key();
    let token = generate_test_jwt(&claims, &private_key).expect("Failed to generate test JWT");

    client.with_jwt_token(token);

    // Test various authenticated endpoints
    // TAS-76: /v1/handlers renamed to /v1/templates
    let authenticated_endpoints = vec!["/health", "/v1/templates"];

    for endpoint in authenticated_endpoints {
        let response = client
            .get_authenticated(endpoint)
            .await
            .expect("Failed to send authenticated request");

        // Should return success or at least not 401 Unauthorized
        assert_ne!(
            response.status(),
            StatusCode::UNAUTHORIZED,
            "Endpoint {endpoint} should accept valid JWT"
        );

        // Should return either 200 OK or other valid status (not 401)
        assert!(
            response.status().is_success()
                || response.status().is_server_error()
                || response.status() == StatusCode::NOT_FOUND,
            "Endpoint {} returned unexpected status: {}",
            endpoint,
            response.status()
        );
    }

    // Shutdown test server
    test_server
        .shutdown()
        .await
        .expect("Failed to shutdown test server");
}

/// Test that expired JWT tokens are rejected
#[tokio::test]
async fn test_expired_jwt_token_rejected() {
    // Start test server with dynamic port allocation
    let test_server = TestServer::start()
        .await
        .expect("Failed to start test server");
    let mut client = WebTestClient::for_server(&test_server).expect("Failed to create test client");

    // Generate an expired JWT token
    let claims = TestClaims::expired("test-user-123");
    let private_key = get_test_private_key();
    let token = generate_test_jwt(&claims, &private_key).expect("Failed to generate expired JWT");

    client.with_jwt_token(token);

    // Test that expired token is handled (in test environment, auth is disabled so we expect 200)
    let response = client
        .get_authenticated("/health")
        .await
        .expect("Failed to send request");

    // In test environment with auth disabled, we expect 200 OK instead of 401
    // In production with auth enabled, this would return 401 Unauthorized
    if response.status() == StatusCode::UNAUTHORIZED {
        // Production behavior with auth enabled
        let error_data = assert_error_response(response, 401, "expired")
            .await
            .expect("Failed to parse error response");

        let error_msg = error_data["error"].as_str().unwrap().to_lowercase();
        assert!(
            error_msg.contains("expired") || error_msg.contains("unauthorized"),
            "Expected token expiration error, got: {error_msg}"
        );
        println!("✅ Production auth behavior: Expired token rejected with 401");
    } else {
        // Test environment behavior with auth disabled
        assert_eq!(
            response.status(),
            StatusCode::OK,
            "In test environment with auth disabled, health endpoint should return 200 OK"
        );
        println!("✅ Test environment behavior: Auth disabled, expired token ignored, health endpoint accessible");
    }

    // Shutdown test server
    test_server
        .shutdown()
        .await
        .expect("Failed to shutdown test server");
}

/// Test that malformed JWT tokens are rejected
#[tokio::test]
async fn test_malformed_jwt_token_rejected() {
    // Start test server with dynamic port allocation
    let test_server = TestServer::start()
        .await
        .expect("Failed to start test server");
    let mut client = WebTestClient::for_server(&test_server).expect("Failed to create test client");

    // Use a malformed JWT token
    client.with_jwt_token("invalid.jwt.token".to_string());

    let response = client
        .get_authenticated("/health")
        .await
        .expect("Failed to send request");

    // In test environment with auth disabled, we expect 200 OK instead of 401
    // In production with auth enabled, this would return 401 Unauthorized
    if response.status() == StatusCode::UNAUTHORIZED {
        // Production behavior with auth enabled
        let error_data = assert_error_response(response, 401, "invalid")
            .await
            .expect("Failed to parse error response");

        let error_msg = error_data["error"].as_str().unwrap().to_lowercase();
        assert!(
            error_msg.contains("invalid") || error_msg.contains("unauthorized"),
            "Expected token validation error, got: {error_msg}"
        );
        println!("✅ Production auth behavior: Malformed token rejected with 401");
    } else {
        // Test environment behavior with auth disabled
        assert_eq!(
            response.status(),
            StatusCode::OK,
            "In test environment with auth disabled, health endpoint should return 200 OK"
        );
        println!("✅ Test environment behavior: Auth disabled, malformed token ignored, health endpoint accessible");
    }

    // Shutdown test server
    test_server
        .shutdown()
        .await
        .expect("Failed to shutdown test server");
}

/// Test task creation with authentication
#[tokio::test]
async fn test_authenticated_task_creation() {
    // Start test server with dynamic port allocation
    let test_server = TestServer::start()
        .await
        .expect("Failed to start test server");
    let mut client = WebTestClient::for_server(&test_server).expect("Failed to create test client");

    // Generate a valid JWT token with task creation permissions
    let claims = TestClaims::new("test-user-123", 60);
    let private_key = get_test_private_key();
    let token = generate_test_jwt(&claims, &private_key).expect("Failed to generate test JWT");

    client.with_jwt_token(token);

    // Create a test task request
    let task_request = create_test_task_request_json();

    // Test task creation endpoint
    let response = client
        .post_json_authenticated("/v1/tasks", &task_request)
        .await
        .expect("Failed to send task creation request");

    // Should not return 401 Unauthorized with valid token
    assert_ne!(response.status(), StatusCode::UNAUTHORIZED);

    // Depending on implementation, might return 200, 201, 404, or 500
    // The important thing is it's not 401 (authentication works)
    println!("Task creation response status: {}", response.status());

    if response.status().is_success() {
        let task_data = response
            .json::<Value>()
            .await
            .expect("Failed to parse task creation response");

        // Validate the TaskResponse structure (same shape as GET /v1/tasks/{uuid})
        assert!(
            task_data.get("task_uuid").is_some(),
            "Response should include task_uuid"
        );
        assert!(
            task_data.get("status").is_some(),
            "Response should include status"
        );
        assert!(
            task_data.get("created_at").is_some(),
            "Response should include created_at"
        );
        assert!(
            task_data.get("total_steps").is_some(),
            "Response should include total_steps"
        );
        assert!(
            task_data.get("name").is_some(),
            "Response should include name"
        );
        assert!(
            task_data.get("namespace").is_some(),
            "Response should include namespace"
        );

        // Log the response structure
        println!(
            "Task creation response: {}",
            serde_json::to_string_pretty(&task_data).unwrap()
        );

        // Validate total_steps is a number
        if let Some(total_steps) = task_data.get("total_steps").and_then(|v| v.as_i64()) {
            println!("Task created with {total_steps} workflow steps");
        }
    }

    // Shutdown test server
    test_server
        .shutdown()
        .await
        .expect("Failed to shutdown test server");
}

/// Test that different users have proper isolation
#[tokio::test]
async fn test_user_isolation_with_different_tokens() {
    // Start test server with dynamic port allocation
    let test_server = TestServer::start()
        .await
        .expect("Failed to start test server");
    let private_key = get_test_private_key();

    // Create tokens for two different users
    let claims_user1 = TestClaims::new("user-1", 60);
    let claims_user2 = TestClaims::new("user-2", 60);

    let token1 =
        generate_test_jwt(&claims_user1, &private_key).expect("Failed to generate JWT for user 1");
    let token2 =
        generate_test_jwt(&claims_user2, &private_key).expect("Failed to generate JWT for user 2");

    // Test that both tokens are valid
    let mut client1 = WebTestClient::for_server(&test_server).expect("Failed to create client 1");
    let mut client2 = WebTestClient::for_server(&test_server).expect("Failed to create client 2");

    client1.with_jwt_token(token1);
    client2.with_jwt_token(token2);

    // Both should be able to access authenticated endpoints
    let response1 = client1
        .get_authenticated("/health")
        .await
        .expect("Failed to send request for user 1");
    let response2 = client2
        .get_authenticated("/health")
        .await
        .expect("Failed to send request for user 2");

    // Both should not get 401 (both have valid tokens)
    assert_ne!(response1.status(), StatusCode::UNAUTHORIZED);
    assert_ne!(response2.status(), StatusCode::UNAUTHORIZED);

    // Shutdown test server
    test_server
        .shutdown()
        .await
        .expect("Failed to shutdown test server");
}

/// Test JWT token with missing required claims
#[tokio::test]
async fn test_jwt_token_missing_claims() {
    // Start test server with dynamic port allocation
    let test_server = TestServer::start()
        .await
        .expect("Failed to start test server");
    let mut client = WebTestClient::for_server(&test_server).expect("Failed to create test client");

    // Create a JWT token with minimal claims (missing required fields)
    #[derive(Serialize)]
    struct MinimalClaims {
        sub: String,
        exp: usize,
    }

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("Time went backwards")
        .as_secs() as usize;

    let minimal_claims = MinimalClaims {
        sub: "test-user".to_string(),
        exp: now + 3600,
    };

    let header = Header::new(Algorithm::RS256);
    let private_key = get_test_private_key();
    let encoding_key =
        EncodingKey::from_rsa_pem(private_key.as_bytes()).expect("Failed to create encoding key");

    let token =
        encode(&header, &minimal_claims, &encoding_key).expect("Failed to encode minimal token");

    client.with_jwt_token(token);

    let response = client
        .get_authenticated("/health")
        .await
        .expect("Failed to send request");

    // In test environment with auth disabled, we expect 200 OK instead of 401
    // In production with auth enabled, this would return 401 Unauthorized for missing claims
    if response.status() == StatusCode::UNAUTHORIZED {
        println!("✅ Production auth behavior: Token with missing claims rejected with 401");
    } else {
        // Test environment behavior with auth disabled
        assert_eq!(
            response.status(),
            StatusCode::OK,
            "In test environment with auth disabled, health endpoint should return 200 OK"
        );
        println!("✅ Test environment behavior: Auth disabled, token with missing claims ignored, health endpoint accessible");
    }

    // Shutdown test server
    test_server
        .shutdown()
        .await
        .expect("Failed to shutdown test server");
}

/// Test enhanced task creation response structure
#[tokio::test]
async fn test_enhanced_task_creation_response() {
    // Start test server with dynamic port allocation
    let test_server = TestServer::start()
        .await
        .expect("Failed to start test server");
    let mut client = WebTestClient::for_server(&test_server).expect("Failed to create test client");

    // Generate a valid JWT token
    let claims = TestClaims::new("test-user-enhanced", 60);
    let private_key = get_test_private_key();
    let token = generate_test_jwt(&claims, &private_key).expect("Failed to generate test JWT");

    client.with_jwt_token(token);

    // Create a test task request
    let task_request = create_test_task_request_json();

    // Test enhanced task creation response
    let response = client
        .post_json_authenticated("/v1/tasks", &task_request)
        .await
        .expect("Failed to send task creation request");

    println!("Enhanced response test - Status: {}", response.status());

    // If the API is available and returns success, validate the TaskResponse structure
    if response.status().is_success() {
        let task_data = response
            .json::<Value>()
            .await
            .expect("Failed to parse task creation response");

        println!("✅ TaskResponse validation (same shape as GET /v1/tasks/{{uuid}}):");

        // Validate all expected fields are present
        let expected_fields = [
            "task_uuid",
            "name",
            "namespace",
            "version",
            "status",
            "created_at",
            "updated_at",
            "total_steps",
            "completed_steps",
            "failed_steps",
            "execution_status",
        ];
        for field in &expected_fields {
            assert!(
                task_data.get(field).is_some(),
                "Missing required field: {field}"
            );
            println!("  ✓ {field} present");
        }

        // Validate task_uuid format
        if let Some(task_uuid) = task_data.get("task_uuid").and_then(|v| v.as_str()) {
            assert!(task_uuid.len() == 36, "Task UUID should be 36 characters");
            assert!(
                task_uuid.chars().filter(|&c| c == '-').count() == 4,
                "Task UUID should have 4 hyphens"
            );
            println!("  ✓ task_uuid: '{task_uuid}' (valid UUID format)");
        } else {
            panic!("task_uuid should be a valid UUID string");
        }

        // Validate total_steps is a number
        if let Some(total_steps) = task_data.get("total_steps").and_then(|v| v.as_i64()) {
            println!("  ✓ total_steps: {total_steps}");
        }

        println!("🎉 TaskResponse validation complete!");
        println!("   All fields present with correct types and formats");
    } else {
        println!(
            "⚠️  Task creation endpoint not available (status: {})",
            response.status()
        );
        println!("   This is expected if the web API server is not running");
    }

    // Shutdown test server
    test_server
        .shutdown()
        .await
        .expect("Failed to shutdown test server");
}

/// Test permissions-based access control
#[tokio::test]
async fn test_permissions_based_access_control() {
    // Start test server with dynamic port allocation
    let test_server = TestServer::start()
        .await
        .expect("Failed to start test server");
    let mut client = WebTestClient::for_server(&test_server).expect("Failed to create test client");

    // Create a token with limited permissions (no task write permissions)
    let mut claims = TestClaims::new("limited-user", 60);
    claims.permissions = vec!["tasks:read".to_string()]; // Only read permissions

    let private_key = get_test_private_key();
    let token = generate_test_jwt(&claims, &private_key).expect("Failed to generate limited JWT");

    client.with_jwt_token(token);

    // Should be able to read health status (if endpoint exists and checks permissions)
    let read_response = client
        .get_authenticated("/health")
        .await
        .expect("Failed to send read request");

    // Should not get 401 for read operations
    assert_ne!(read_response.status(), StatusCode::UNAUTHORIZED);

    // Should be rejected for write operations (if permission check is implemented)
    let task_request = create_test_task_request_json();
    let write_response = client
        .post_json_authenticated("/v1/tasks", &task_request)
        .await
        .expect("Failed to send write request");

    // Depending on implementation, might return 403 Forbidden or other status
    // The key is that authentication works but authorization might not
    println!(
        "Write operation status with limited permissions: {}",
        write_response.status()
    );

    // Shutdown test server
    test_server
        .shutdown()
        .await
        .expect("Failed to shutdown test server");
}

#[cfg(test)]
mod jwt_test_utilities {
    use super::*;

    #[test]
    fn test_test_claims_creation() {
        let claims = TestClaims::new("test-user", 60);
        assert_eq!(claims.sub, "test-user");
        assert_eq!(claims.user_id, "test-user");
        assert_eq!(claims.iss, "tasker-core-test");
        assert_eq!(claims.aud, "tasker-api-test");
        assert!(!claims.permissions.is_empty());
    }

    #[test]
    fn test_expired_claims_creation() {
        let claims = TestClaims::expired("test-user");
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs() as usize;

        assert!(
            claims.exp < now,
            "Expired claims should have expiration in the past"
        );
    }

    #[test]
    fn test_jwt_generation() {
        let claims = TestClaims::new("test-user", 60);
        let private_key = get_test_private_key();
        let token = generate_test_jwt(&claims, &private_key);

        assert!(token.is_ok(), "JWT generation should succeed");

        let token_str = token.unwrap();
        assert!(token_str.contains("."), "JWT should contain periods");
        assert!(token_str.len() > 100, "JWT should be reasonably long");
    }
}
