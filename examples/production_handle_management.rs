//! # Production Handle Management Example
//!
//! This example demonstrates how to use validate_or_refresh() for resilient
//! production systems that run for days without human intervention.

use std::time::Duration;
use tasker_core::ffi::shared::handles::SharedOrchestrationHandle;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🎯 Production Handle Management Demo");
    println!("=====================================");

    // Simulate a long-running production system
    let mut handle = SharedOrchestrationHandle::get_global();

    // Production system loop - runs for days
    for iteration in 1..=5 {
        println!("\n📊 Production Iteration {iteration}");

        // This is the key pattern for production systems:
        // Always use validate_or_refresh() before critical operations
        match handle.validate_or_refresh() {
            Ok(validated_handle) => {
                handle = validated_handle;

                let info = handle.info();
                println!(
                    "✅ Handle ready: {} (expires in {}s)",
                    info.handle_id, info.expires_in_seconds
                );

                // Simulate production work
                simulate_production_work(&handle).await?;
            }
            Err(e) => {
                // This should only happen if refresh fails (very rare)
                eprintln!("🔥 CRITICAL: Handle validation and refresh failed: {e}");
                return Err(e.into());
            }
        }

        // Simulate time passing in production
        if iteration == 3 {
            println!("\n⏰ Simulating 2+ hours passing (handle expiry)...");
            // In real production, this would be actual time passing
            // Here we'll create an expired handle to demonstrate auto-recovery
            handle = std::sync::Arc::new(create_expired_handle_for_demo());
        }
    }

    println!("\n🎉 Production system completed successfully!");
    println!("   Handle validation and auto-refresh worked seamlessly");

    Ok(())
}

async fn simulate_production_work(
    _handle: &SharedOrchestrationHandle,
) -> Result<(), Box<dyn std::error::Error>> {
    // In a real system, this would be:
    // - Processing workflow tasks
    // - Handling user requests
    // - Managing system resources
    // - All using the validated handle

    println!("   🔧 Processing production workload...");
    tokio::time::sleep(Duration::from_millis(100)).await;
    println!("   ✅ Production work completed");

    Ok(())
}

fn create_expired_handle_for_demo() -> SharedOrchestrationHandle {
    // Create a handle with past timestamp to simulate expiry
    let orchestration_system =
        tasker_core::ffi::shared::orchestration_system::initialize_unified_orchestration_system();

    SharedOrchestrationHandle {
        orchestration_system,
        handle_id: "demo_expired_handle".to_string(),
        created_at: std::time::SystemTime::now() - Duration::from_secs(3 * 3600), // 3 hours ago
    }
}

// Example Ruby usage pattern for production systems:
//
// ```ruby
// class ProductionTaskProcessor
//   def initialize
//     @handle = TaskerCore::OrchestrationHandle.get_global
//   end
//
//   def process_task(task_id)
//     # Key pattern: Always validate_or_refresh before operations
//     @handle = @handle.validate_or_refresh
//
//     # Now safely use the handle for production work
//     result = @handle.process_workflow(task_id)
//
//     # Handle will automatically refresh if expired,
//     # ensuring continuous operation for days/weeks
//     result
//   rescue => e
//     # Only fails if refresh itself fails (very rare)
//     logger.error "Critical handle failure: #{e}"
//     raise
//   end
//
//   def health_check
//     # Check handle status for monitoring
//     info = @handle.info
//     {
//       handle_id: info.shared_handle_id,
//       expires_in: @handle.expires_in_seconds,
//       status: @handle.is_expired ? 'expired' : 'active'
//     }
//   end
// end
// ```
