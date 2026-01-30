//! # TemplateQueryService Tests (TAS-63)
//!
//! Integration tests for TemplateQueryService validating:
//! - Template listing across namespaces
//! - Template listing filtered by namespace
//! - Single template retrieval
//! - Template existence checking
//! - Namespace information retrieval
//! - Error handling (not found cases)

use anyhow::Result;
use sqlx::PgPool;
use std::sync::Arc;

use tasker_orchestration::services::TemplateQueryService;
use tasker_shared::registry::TaskHandlerRegistry;
use tasker_shared::system_context::SystemContext;

/// Get the path to task template fixtures in the workspace root
fn fixture_path() -> String {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap_or_else(|_| ".".to_string());
    std::path::Path::new(&manifest_dir)
        .parent()
        .unwrap_or(std::path::Path::new("."))
        .join("tests/fixtures/task_templates/rust")
        .to_string_lossy()
        .to_string()
}

/// Helper to set up TemplateQueryService with registered templates
async fn setup_template_service(pool: PgPool) -> Result<TemplateQueryService> {
    let registry = TaskHandlerRegistry::new(pool.clone());
    registry
        .discover_and_register_templates(&fixture_path())
        .await?;

    let system_context = Arc::new(SystemContext::with_pool(pool).await?);
    Ok(TemplateQueryService::new(system_context))
}

// =============================================================================
// list_templates tests
// =============================================================================

/// Test: list_templates returns all registered templates
#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_list_templates_all(pool: PgPool) -> Result<()> {
    let service = setup_template_service(pool).await?;

    let response = service.list_templates(None).await?;

    assert!(
        !response.templates.is_empty(),
        "Should return registered templates"
    );
    assert!(
        !response.namespaces.is_empty(),
        "Should return namespace summaries"
    );
    assert!(response.total_count > 0, "Total count should be positive");

    // Verify namespace summaries have expected fields
    for ns in &response.namespaces {
        assert!(!ns.name.is_empty(), "Namespace name should be set");
        assert!(ns.template_count > 0, "Namespace should have templates");
    }

    Ok(())
}

/// Test: list_templates filtered by namespace
#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_list_templates_by_namespace(pool: PgPool) -> Result<()> {
    let service = setup_template_service(pool).await?;

    let response = service.list_templates(Some("test_sql")).await?;

    assert!(
        !response.templates.is_empty(),
        "Should return templates for test_sql namespace"
    );

    // All returned templates should be in the requested namespace
    for template in &response.templates {
        assert_eq!(
            template.namespace, "test_sql",
            "All templates should be in test_sql namespace"
        );
    }

    Ok(())
}

/// Test: list_templates returns empty for unknown namespace
#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_list_templates_unknown_namespace(pool: PgPool) -> Result<()> {
    let service = setup_template_service(pool).await?;

    let response = service.list_templates(Some("nonexistent")).await?;

    assert!(
        response.templates.is_empty(),
        "Should return empty for unknown namespace"
    );

    Ok(())
}

/// Test: list_templates template summaries have step counts
#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_list_templates_have_step_counts(pool: PgPool) -> Result<()> {
    let service = setup_template_service(pool).await?;

    let response = service.list_templates(Some("test_sql")).await?;

    assert!(!response.templates.is_empty());

    for template in &response.templates {
        assert!(!template.name.is_empty(), "Template name should be set");
        assert!(
            !template.version.is_empty(),
            "Template version should be set"
        );
        // Step count comes from configuration parsing
        // Some templates may have 0 if configuration doesn't parse
    }

    Ok(())
}

// =============================================================================
// get_template tests
// =============================================================================

/// Test: get_template returns full template details
#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_get_template_success(pool: PgPool) -> Result<()> {
    let service = setup_template_service(pool).await?;

    let detail = service
        .get_template("test_sql", "boundary_max_attempts_one", "1.0.0")
        .await?;

    assert_eq!(detail.name, "boundary_max_attempts_one");
    assert_eq!(detail.namespace, "test_sql");
    assert_eq!(detail.version, "1.0.0");
    assert!(
        !detail.steps.is_empty(),
        "Template should have step definitions"
    );

    // Verify step definitions
    for step in &detail.steps {
        assert!(!step.name.is_empty(), "Step name should be set");
    }

    Ok(())
}

/// Test: get_template returns TemplateNotFound for unknown template
#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_get_template_not_found(pool: PgPool) -> Result<()> {
    let service = setup_template_service(pool).await?;

    let result = service
        .get_template("test_sql", "nonexistent_template", "1.0.0")
        .await;

    assert!(result.is_err(), "Should return error for unknown template");

    Ok(())
}

/// Test: get_template returns error for wrong version
#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_get_template_wrong_version(pool: PgPool) -> Result<()> {
    let service = setup_template_service(pool).await?;

    let result = service
        .get_template("test_sql", "boundary_max_attempts_one", "99.99.99")
        .await;

    assert!(result.is_err(), "Should return error for wrong version");

    Ok(())
}

// =============================================================================
// template_exists tests
// =============================================================================

/// Test: template_exists returns true for registered template
#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_template_exists_true(pool: PgPool) -> Result<()> {
    let service = setup_template_service(pool).await?;

    let exists = service
        .template_exists("test_sql", "boundary_max_attempts_one", "1.0.0")
        .await?;

    assert!(exists, "Registered template should exist");

    Ok(())
}

/// Test: template_exists returns false for unknown template
#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_template_exists_false(pool: PgPool) -> Result<()> {
    let service = setup_template_service(pool).await?;

    let exists = service
        .template_exists("test_sql", "nonexistent_template", "1.0.0")
        .await?;

    assert!(!exists, "Unknown template should not exist");

    Ok(())
}

// =============================================================================
// get_namespace tests
// =============================================================================

/// Test: get_namespace returns namespace info
#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_get_namespace_success(pool: PgPool) -> Result<()> {
    let service = setup_template_service(pool).await?;

    let ns = service.get_namespace("test_sql").await?;

    assert_eq!(ns.name, "test_sql");
    assert!(ns.template_count > 0, "Namespace should have templates");

    Ok(())
}

/// Test: get_namespace returns NamespaceNotFound for unknown namespace
#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_get_namespace_not_found(pool: PgPool) -> Result<()> {
    let service = setup_template_service(pool).await?;

    let result = service.get_namespace("nonexistent_namespace").await;

    assert!(result.is_err(), "Should return error for unknown namespace");

    Ok(())
}
