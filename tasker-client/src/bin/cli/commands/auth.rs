//! Authentication command handlers for the Tasker CLI (TAS-150)

use rsa::pkcs1::{EncodeRsaPrivateKey, EncodeRsaPublicKey};
use rsa::rand_core::OsRng;
use rsa::RsaPrivateKey;
use std::path::Path;
use tasker_client::ClientResult;
use tasker_shared::config::WebAuthConfig;
use tasker_shared::types::auth::JwtAuthenticator;
use tasker_shared::types::permissions::Permission;

use crate::AuthCommands;

pub async fn handle_auth_command(cmd: AuthCommands) -> ClientResult<()> {
    match cmd {
        AuthCommands::GenerateKeys {
            output_dir,
            key_size,
        } => generate_keys(&output_dir, key_size),
        AuthCommands::GenerateToken {
            permissions,
            subject,
            private_key,
            expiry_hours,
            issuer,
            audience,
        } => generate_token(&permissions, &subject, &private_key, expiry_hours, &issuer, &audience),
        AuthCommands::ShowPermissions => show_permissions(),
        AuthCommands::ValidateToken { token, public_key } => validate_token(&token, &public_key),
    }
}

fn generate_keys(output_dir: &str, key_size: usize) -> ClientResult<()> {
    let output_path = Path::new(output_dir);

    // Validate key size
    if key_size < 2048 {
        eprintln!("Error: Key size must be at least 2048 bits for security");
        return Err(tasker_client::ClientError::InvalidInput(
            "Key size must be at least 2048 bits".to_string(),
        ));
    }

    // Create output directory
    std::fs::create_dir_all(output_path).map_err(|e| {
        tasker_client::ClientError::config_error(format!(
            "Failed to create output directory: {}",
            e
        ))
    })?;

    println!("Generating {}-bit RSA key pair...", key_size);

    let mut rng = OsRng;
    let private_key = RsaPrivateKey::new(&mut rng, key_size).map_err(|e| {
        tasker_client::ClientError::config_error(format!("Failed to generate RSA key: {}", e))
    })?;

    let public_key = private_key.to_public_key();

    // Export as PEM (PKCS#1 format, compatible with jsonwebtoken)
    let private_pem = private_key.to_pkcs1_pem(rsa::pkcs1::LineEnding::LF).map_err(|e| {
        tasker_client::ClientError::config_error(format!(
            "Failed to encode private key: {}",
            e
        ))
    })?;

    let public_pem = public_key.to_pkcs1_pem(rsa::pkcs1::LineEnding::LF).map_err(|e| {
        tasker_client::ClientError::config_error(format!(
            "Failed to encode public key: {}",
            e
        ))
    })?;

    let private_key_path = output_path.join("jwt-private-key.pem");
    let public_key_path = output_path.join("jwt-public-key.pem");

    std::fs::write(&private_key_path, private_pem.as_bytes()).map_err(|e| {
        tasker_client::ClientError::config_error(format!("Failed to write private key: {}", e))
    })?;

    std::fs::write(&public_key_path, public_pem.as_bytes()).map_err(|e| {
        tasker_client::ClientError::config_error(format!("Failed to write public key: {}", e))
    })?;

    println!("Keys generated successfully:");
    println!("  Private key: {}", private_key_path.display());
    println!("  Public key:  {}", public_key_path.display());
    println!();
    println!("Add to your configuration:");
    println!("  jwt_public_key_path = \"{}\"", public_key_path.display());
    println!();
    println!("Or set environment variable:");
    println!("  export TASKER_JWT_PUBLIC_KEY_PATH={}", public_key_path.display());

    Ok(())
}

fn generate_token(
    permissions: &str,
    subject: &str,
    private_key_path: &str,
    expiry_hours: u64,
    issuer: &str,
    audience: &str,
) -> ClientResult<()> {
    // Read private key
    let private_key_pem = std::fs::read_to_string(private_key_path).map_err(|e| {
        tasker_client::ClientError::config_error(format!(
            "Failed to read private key from '{}': {}",
            private_key_path, e
        ))
    })?;

    // Parse permissions
    let perms: Vec<String> = permissions
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();

    if perms.is_empty() {
        return Err(tasker_client::ClientError::InvalidInput(
            "At least one permission is required".to_string(),
        ));
    }

    // Validate permissions (warn about unknown ones)
    let unknown = tasker_shared::types::permissions::validate_permissions(&perms);
    if !unknown.is_empty() {
        eprintln!(
            "Warning: Unknown permissions (will still be included): {}",
            unknown.join(", ")
        );
    }

    // Create authenticator with the private key
    let config = WebAuthConfig {
        enabled: true,
        jwt_private_key: private_key_pem,
        jwt_issuer: issuer.to_string(),
        jwt_audience: audience.to_string(),
        jwt_token_expiry_hours: expiry_hours,
        ..Default::default()
    };

    let authenticator = JwtAuthenticator::from_config(&config).map_err(|e| {
        tasker_client::ClientError::config_error(format!(
            "Failed to create JWT authenticator: {}",
            e
        ))
    })?;

    // Generate token
    let token = authenticator
        .generate_token(subject, perms.clone())
        .map_err(|e| {
            tasker_client::ClientError::config_error(format!("Failed to generate token: {}", e))
        })?;

    println!("{}", token);
    eprintln!();
    eprintln!("Token details:");
    eprintln!("  Subject: {}", subject);
    eprintln!("  Issuer: {}", issuer);
    eprintln!("  Audience: {}", audience);
    eprintln!("  Expires in: {} hours", expiry_hours);
    eprintln!("  Permissions: {}", perms.join(", "));
    eprintln!();
    eprintln!("Use with:");
    eprintln!("  export TASKER_AUTH_TOKEN=<token above>");
    eprintln!("  curl -H \"Authorization: Bearer <token>\" http://localhost:8080/v1/tasks");

    Ok(())
}

fn show_permissions() -> ClientResult<()> {
    println!("Available Permissions");
    println!("=====================");
    println!();
    println!("{:<25} {:<15} Description", "Permission", "Resource");
    println!("{:<25} {:<15} -----------", "---------", "--------");

    let all_permissions = [
        (Permission::TasksCreate, "Create new tasks"),
        (Permission::TasksRead, "Read task details"),
        (Permission::TasksList, "List tasks"),
        (Permission::TasksCancel, "Cancel running tasks"),
        (Permission::TasksContextRead, "Read task context data"),
        (Permission::StepsRead, "Read workflow step details"),
        (Permission::StepsResolve, "Manually resolve steps"),
        (Permission::DlqRead, "Read DLQ entries"),
        (Permission::DlqUpdate, "Update DLQ investigations"),
        (Permission::DlqStats, "View DLQ statistics"),
        (Permission::TemplatesRead, "Read task templates"),
        (Permission::TemplatesValidate, "Validate templates"),
        (Permission::SystemConfigRead, "Read system configuration"),
        (Permission::HandlersRead, "Read handler registry"),
        (Permission::AnalyticsRead, "Read analytics data"),
        (Permission::WorkerConfigRead, "Read worker configuration"),
        (Permission::WorkerTemplatesRead, "Read worker templates"),
    ];

    for (perm, description) in &all_permissions {
        println!("{:<25} {:<15} {}", perm.as_str(), perm.resource(), description);
    }

    println!();
    println!("Wildcards:");
    println!("  tasks:*    - All task permissions");
    println!("  steps:*    - All step permissions");
    println!("  dlq:*      - All DLQ permissions");
    println!("  *          - All permissions (superuser)");

    Ok(())
}

fn validate_token(token: &str, public_key_path: &str) -> ClientResult<()> {
    // Read public key
    let public_key_pem = std::fs::read_to_string(public_key_path).map_err(|e| {
        tasker_client::ClientError::config_error(format!(
            "Failed to read public key from '{}': {}",
            public_key_path, e
        ))
    })?;

    // Create authenticator with the public key
    let config = WebAuthConfig {
        enabled: true,
        jwt_public_key: public_key_pem,
        ..Default::default()
    };

    let authenticator = JwtAuthenticator::from_config(&config).map_err(|e| {
        tasker_client::ClientError::config_error(format!(
            "Failed to create JWT authenticator: {}",
            e
        ))
    })?;

    // Validate token
    match authenticator.validate_token(token) {
        Ok(claims) => {
            println!("Token is valid");
            println!();
            println!("Claims:");
            println!("  Subject: {}", claims.sub);
            println!("  Issuer: {}", claims.iss);
            println!("  Audience: {}", claims.aud);
            println!(
                "  Issued at: {}",
                chrono::DateTime::from_timestamp(claims.iat, 0)
                    .map(|dt| dt.to_rfc3339())
                    .unwrap_or_else(|| "unknown".to_string())
            );
            println!(
                "  Expires: {}",
                chrono::DateTime::from_timestamp(claims.exp, 0)
                    .map(|dt| dt.to_rfc3339())
                    .unwrap_or_else(|| "unknown".to_string())
            );

            if !claims.permissions.is_empty() {
                println!("  Permissions:");
                for perm in &claims.permissions {
                    let known = Permission::from_str_opt(perm).is_some();
                    let marker = if known { "" } else { " (unknown)" };
                    println!("    - {}{}", perm, marker);
                }
            }

            if !claims.worker_namespaces.is_empty() {
                println!("  Worker namespaces: {}", claims.worker_namespaces.join(", "));
            }

            // Check expiry
            let now = chrono::Utc::now().timestamp();
            if claims.exp < now {
                eprintln!();
                eprintln!("WARNING: Token has expired!");
            }
        }
        Err(e) => {
            eprintln!("Token validation failed: {}", e);
            return Err(tasker_client::ClientError::config_error(format!(
                "Token validation failed: {}",
                e
            )));
        }
    }

    Ok(())
}
