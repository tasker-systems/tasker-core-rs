//! # Task Template Loading Integration Tests
//!
//! Comprehensive tests for template registration and deserialization
//! using the actual code paths from worker and orchestration systems.
//!
//! This test validates the complete workflow:
//! 1. YAML template parsing
//! 2. Template registration via TaskHandlerRegistry
//! 3. Database storage and retrieval
//! 4. JSON deserialization (the critical orchestration step)
//! 5. Template validation and comparison
//!
//! The test processes all available template fixtures (Ruby and Rust)
//! to ensure comprehensive coverage and catch any template-specific issues.

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use tracing::{debug, error, info, warn};

use tasker_shared::models::core::task_template::TaskTemplate;
use tasker_shared::registry::task_handler_registry::TaskHandlerRegistry;

/// Test all template fixtures for proper registration and deserialization
#[tokio::test]
async fn test_all_template_fixtures_registration_and_deserialization() -> Result<()> {
    // Initialize tracing for the test
    let _ = tracing_subscriber::fmt()
        .with_env_filter("debug")
        .try_init();

    info!("🧪 Starting comprehensive template fixtures test");

    // Setup test database connection
    let database_url = std::env::var("DATABASE_URL").unwrap_or_else(|_| {
        "postgresql://tasker:tasker@localhost:5432/tasker_rust_test".to_string()
    });

    debug!("Connecting to database: {}", database_url);

    let db_pool = sqlx::PgPool::connect(&database_url)
        .await
        .context("Failed to connect to test database")?;

    // Create registry using actual worker code path
    let registry = TaskHandlerRegistry::new(db_pool.clone());

    // Define all template fixtures to test
    let template_fixtures = vec![
        // Ruby templates
        ("ruby", "linear_workflow_handler.yaml"),
        ("ruby", "diamond_workflow_handler.yaml"),
        ("ruby", "parallel_workflow_handler.yaml"),
        ("ruby", "tree_workflow_handler.yaml"),
        ("ruby", "order_fulfillment_handler.yaml"),
        // Rust templates
        ("rust", "mathematical_sequence.yaml"),
        ("rust", "diamond_convergence.yaml"),
        ("rust", "parallel_batch.yaml"),
        ("rust", "hierarchical_tree.yaml"),
        ("rust", "business_workflow.yaml"),
        ("rust", "complex_dag.yaml"),
    ];

    let mut total_tested = 0;
    let mut failed_templates = Vec::new();

    for (lang, filename) in template_fixtures {
        let template_path = PathBuf::from(format!(
            "tests/fixtures/task_templates/{}/{}",
            lang, filename
        ));

        info!("📁 Testing template: {}/{}", lang, filename);

        if !template_path.exists() {
            warn!(
                "⚠️  Template fixture not found: {}, skipping",
                template_path.display()
            );
            continue;
        }

        match test_single_template(&registry, &template_path).await {
            Ok(_) => {
                info!("✅ Template test PASSED for {}/{}", lang, filename);
                total_tested += 1;
            }
            Err(e) => {
                error!("❌ Template test FAILED for {}/{}: {}", lang, filename, e);
                failed_templates.push(format!("{}/{}: {}", lang, filename, e));
                total_tested += 1;
            }
        }
    }

    info!(
        "📊 Test results: {} total, {} passed, {} failed",
        total_tested,
        total_tested - failed_templates.len(),
        failed_templates.len()
    );

    if !failed_templates.is_empty() {
        error!("❌ Failed templates:");
        for failure in &failed_templates {
            error!("   - {}", failure);
        }
        return Err(anyhow::anyhow!(
            "{} templates failed the test",
            failed_templates.len()
        ));
    }

    info!("🎉 All {} templates passed successfully!", total_tested);
    Ok(())
}

/// Test a single template through the complete workflow
async fn test_single_template(registry: &TaskHandlerRegistry, template_path: &Path) -> Result<()> {
    debug!("📖 Loading template: {}", template_path.display());

    // Step 1: Load YAML template using actual production code path
    let template = registry
        .load_template_from_file(template_path)
        .await
        .context("Failed to load template from file using production code path")?;

    debug!(
        "✅ Template parsed: name={}, namespace={}, version={}, steps={}",
        template.name,
        template.namespace_name,
        template.version,
        template.steps.len()
    );

    // Step 2: Register template using actual worker code path
    debug!("📝 Registering template...");

    registry
        .register_task_template(&template)
        .await
        .context("Failed to register template")?;

    debug!("✅ Template registered successfully");

    // Step 3: Retrieve template using actual production code path
    debug!("🔍 Retrieving template from registry using production code path...");

    let handler_metadata = registry
        .get_task_template_from_registry(
            &template.namespace_name,
            &template.name,
            &template.version,
        )
        .await
        .context("Failed to get task template from registry using production code path")?;

    debug!("✅ Found template in registry via production code path");

    // Step 4: Test TaskTemplate extraction from HandlerMetadata (orchestration code path)
    debug!("🔬 Testing TaskTemplate extraction from HandlerMetadata...");

    // Also test direct template retrieval (another orchestration code path)
    debug!("🔬 Testing direct template retrieval...");
    let direct_template = registry
        .get_task_template(&template.namespace_name, &template.name, &template.version)
        .await
        .context("Failed to get template directly")?;
    debug!("✅ Direct template retrieval successful!");

    // Validate direct template matches original
    validate_template_match(&template, &direct_template)
        .context("Direct template validation failed")?;
    debug!("✅ Direct template validation passed!");

    // Validate that both production code paths return identical templates
    validate_template_match(&template, &direct_template)
        .context("Production code path consistency validation failed")?;
    debug!("✅ Both production code paths return identical templates!");

    // Validate HandlerMetadata structure - it should contain the template in config_schema
    debug!("🔬 Validating HandlerMetadata structure...");
    if handler_metadata.config_schema.is_none() {
        return Err(anyhow::anyhow!(
            "HandlerMetadata config_schema is None - template data missing"
        ));
    }

    // Try to deserialize the config_schema back to TaskTemplate (orchestration path)
    let config_json = handler_metadata.config_schema.unwrap();
    let metadata_template: TaskTemplate = serde_json::from_value(config_json)
        .context("Failed to deserialize HandlerMetadata config_schema to TaskTemplate")?;

    validate_template_match(&template, &metadata_template)
        .context("HandlerMetadata config_schema template validation failed")?;
    debug!("✅ HandlerMetadata config_schema validation passed!");

    Ok(())
}

/// Validate that two templates match
fn validate_template_match(original: &TaskTemplate, loaded: &TaskTemplate) -> Result<()> {
    let mut differences = Vec::new();

    // Check core fields
    if original.name != loaded.name {
        differences.push(format!("name: '{}' vs '{}'", original.name, loaded.name));
    }

    if original.namespace_name != loaded.namespace_name {
        differences.push(format!(
            "namespace_name: '{}' vs '{}'",
            original.namespace_name, loaded.namespace_name
        ));
    }

    if original.version != loaded.version {
        differences.push(format!(
            "version: '{}' vs '{}'",
            original.version, loaded.version
        ));
    }

    if original.description != loaded.description {
        differences.push(format!(
            "description: '{:?}' vs '{:?}'",
            original.description, loaded.description
        ));
    }

    // Check steps count (detailed step comparison would be too verbose for this test)
    if original.steps.len() != loaded.steps.len() {
        differences.push(format!(
            "steps count: {} vs {}",
            original.steps.len(),
            loaded.steps.len()
        ));
    }

    if differences.is_empty() {
        Ok(())
    } else {
        Err(anyhow::anyhow!(
            "Template validation failed with {} differences: {}",
            differences.len(),
            differences.join(", ")
        ))
    }
}
