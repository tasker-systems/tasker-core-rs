//! Simple test to verify YAML configuration loading works

use tasker_core::orchestration::config::ConfigurationManager;

const TEST_CONFIG_YAML: &str = r#"
auth:
  authentication_enabled: false
  strategy: "none"

orchestration:
  task_requests_queue_name: "test_task_requests_queue"
  orchestrator_id: "test-orchestrator-123"
  tasks_per_cycle: 10
  cycle_interval_seconds: 2
  max_cycles: 5
  active_namespaces:
    - "test_fulfillment"
    - "test_inventory"
    - "test_notifications"

backoff:
  default_backoff_seconds: [1, 2, 4, 8, 16]
"#;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🧪 Testing YAML Configuration Loading");
    
    // Test: Load configuration from YAML string
    let config_manager = ConfigurationManager::load_from_yaml(TEST_CONFIG_YAML)?;
    let tasker_config = config_manager.system_config();
    
    // Verify configuration values
    assert_eq!(tasker_config.orchestration.task_requests_queue_name, "test_task_requests_queue");
    assert_eq!(tasker_config.orchestration.orchestrator_id, Some("test-orchestrator-123".to_string()));
    assert_eq!(tasker_config.orchestration.tasks_per_cycle, 10);
    assert_eq!(tasker_config.orchestration.active_namespaces.len(), 3);
    
    println!("✅ Configuration loaded successfully:");
    println!("   - Queue name: {}", tasker_config.orchestration.task_requests_queue_name);
    println!("   - Orchestrator ID: {:?}", tasker_config.orchestration.orchestrator_id);
    println!("   - Tasks per cycle: {}", tasker_config.orchestration.tasks_per_cycle);
    println!("   - Active namespaces: {}", tasker_config.orchestration.active_namespaces.len());
    
    // Test: Convert to OrchestrationSystemConfig
    let system_config = tasker_config.orchestration.to_orchestration_system_config();
    println!("✅ OrchestrationSystemConfig converted successfully");
    println!("   - Orchestrator ID: {}", system_config.orchestrator_id);
    println!("   - Tasks per cycle: {}", system_config.orchestration_loop_config.tasks_per_cycle);
    
    println!("\n🎉 YAML Configuration Test Passed!");
    Ok(())
}