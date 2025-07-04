use chrono;
use tasker_core::database::DatabaseConnection;
use tasker_core::models::{DependentSystem, NewDependentSystem, StepDagRelationship};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🔍 Verifying Tasker Core models and database connection...");

    // Test database connection
    let db = DatabaseConnection::new().await?;
    let pool = db.pool();
    println!("✅ Database connection established");

    // Test DependentSystem model with find_or_create
    let system_name = format!(
        "test_verification_system_{}",
        chrono::Utc::now().timestamp()
    );
    let created_system = DependentSystem::find_or_create_by_name(pool, &system_name).await?;
    println!(
        "✅ DependentSystem model works - created/found system with ID: {}",
        created_system.dependent_system_id
    );

    // Test listing all systems
    let all_systems = DependentSystem::list_all(pool).await?;
    println!(
        "✅ Found {} dependent systems in database",
        all_systems.len()
    );

    // Test StepDagRelationship view access (stub implementation)
    let relationships = StepDagRelationship::get_by_task(pool, 1).await?;
    println!(
        "✅ StepDagRelationship view access works - found {} relationships",
        relationships.len()
    );

    // Cleanup our test system
    DependentSystem::delete(pool, created_system.dependent_system_id).await?;
    println!("✅ Cleanup completed");

    db.close().await;
    println!("🎉 All model verifications passed!");

    Ok(())
}
