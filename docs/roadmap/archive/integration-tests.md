# Integration Test Strategy

**Phase 1 Testing Foundation - Week 1 Focus**

## Overview

Integration tests serve as the forcing function to complete critical placeholders and validate that our foundation actually works. Rather than building more features on potentially broken foundations, we test complex workflows end-to-end to expose and fix fundamental issues.

## Testing Philosophy

### "Tests Drive Implementation"
- Write failing tests that expose placeholders
- Implement **complete** functionality to make tests pass
- No partial implementations or new stubs
- Use test failures to guide implementation priorities

### "Complex Workflows First"
- Test the most challenging patterns first (Diamond, Parallel, Tree)
- Simple linear workflows often hide integration issues
- Complex patterns stress-test all system boundaries
- Real-world patterns validate architectural decisions

## Week 1 Test Implementation Plan

### Day 1-2: Complex Workflow Integration Tests ✅ IN PROGRESS

#### Test File: `tests/orchestration/complex_workflow_integration_test.rs` ✅ IMPLEMENTED

**Status**: Successfully created integration test infrastructure that systematically exposes critical placeholders

**Achievements**:
- ✅ Created MockFrameworkIntegration for testing orchestration without Ruby FFI
- ✅ Established real task + workflow step creation through factory system
- ✅ End-to-end orchestration test reaching step state validation
- ✅ Fixed 4 critical schema/type alignment issues through systematic testing

**Current Investigation**: Step state initialization issue - steps created as 'unknown' instead of 'pending'

**Linear Workflow Test**
```rust
#[sqlx::test]
async fn test_linear_workflow_execution() {
    let pool = setup_test_db().await;
    
    // Create A→B→C→D workflow using factories
    let task = LinearWorkflowFactory::create(&pool, LinearWorkflowConfig {
        step_count: 4,
        step_states: vec![
            StepState::Pending,
            StepState::Pending, 
            StepState::Pending,
            StepState::Pending,
        ],
    }).await.unwrap();
    
    // Execute workflow through WorkflowCoordinator
    let coordinator = WorkflowCoordinator::new(pool.clone());
    let mock_integration = MockFrameworkIntegration::new();
    
    let result = coordinator.execute_task_workflow(
        task.id, 
        Arc::new(mock_integration)
    ).await.unwrap();
    
    // Validate execution
    assert_eq!(result.status, TaskOrchestrationResult::Complete);
    
    // Validate all steps were executed in order
    let step_transitions = WorkflowStepTransition::find_by_task_id(&pool, task.id).await.unwrap();
    assert_eq!(step_transitions.len(), 4);
    
    // Validate state machine transitions occurred
    let task_transitions = TaskTransition::find_by_task_id(&pool, task.id).await.unwrap();
    assert!(task_transitions.iter().any(|t| t.to_state == "completed"));
}
```

**Diamond Workflow Test**
```rust
#[sqlx::test]
async fn test_diamond_workflow_concurrent_execution() {
    let pool = setup_test_db().await;
    
    // Create A→(B,C)→D pattern using factories
    let task = DiamondWorkflowFactory::create(&pool, DiamondWorkflowConfig {
        root_step: "setup",
        parallel_steps: vec!["process_a", "process_b"],
        merge_step: "finalize",
    }).await.unwrap();
    
    let coordinator = WorkflowCoordinator::new(pool.clone());
    let mock_integration = MockFrameworkIntegration::with_delays(
        // B and C should execute concurrently
        vec![("process_a", 100), ("process_b", 100)]
    );
    
    let start_time = Instant::now();
    let result = coordinator.execute_task_workflow(
        task.id,
        Arc::new(mock_integration)
    ).await.unwrap();
    let execution_time = start_time.elapsed();
    
    // Should complete in ~100ms, not 200ms (proving concurrency)
    assert!(execution_time < Duration::from_millis(150));
    assert_eq!(result.status, TaskOrchestrationResult::Complete);
    
    // Validate parallel steps executed concurrently
    let step_transitions = WorkflowStepTransition::find_by_task_id(&pool, task.id).await.unwrap();
    let parallel_executions = step_transitions.iter()
        .filter(|t| t.step_name.contains("process"))
        .collect::<Vec<_>>();
    
    // Both parallel steps should have started within 10ms of each other
    let start_times: Vec<_> = parallel_executions.iter()
        .map(|t| t.created_at)
        .collect();
    assert!(start_times[1] - start_times[0] < Duration::from_millis(10));
}
```

**Tree Workflow Test**
```rust
#[sqlx::test] 
async fn test_tree_workflow_complex_dependencies() {
    let pool = setup_test_db().await;
    
    // Create complex tree pattern
    let task = TreeWorkflowFactory::create(&pool, TreeWorkflowConfig {
        levels: vec![
            vec!["root"],
            vec!["branch_a", "branch_b"],
            vec!["leaf_a1", "leaf_a2", "leaf_b1"], 
            vec!["final"]
        ],
        dependencies: HashMap::from([
            ("branch_a", vec!["root"]),
            ("branch_b", vec!["root"]),
            ("leaf_a1", vec!["branch_a"]),
            ("leaf_a2", vec!["branch_a"]),
            ("leaf_b1", vec!["branch_b"]),
            ("final", vec!["leaf_a1", "leaf_a2", "leaf_b1"]),
        ]),
    }).await.unwrap();
    
    let coordinator = WorkflowCoordinator::new(pool.clone());
    let mock_integration = MockFrameworkIntegration::new();
    
    let result = coordinator.execute_task_workflow(
        task.id,
        Arc::new(mock_integration)  
    ).await.unwrap();
    
    assert_eq!(result.status, TaskOrchestrationResult::Complete);
    
    // Validate execution order respects dependencies
    let step_transitions = WorkflowStepTransition::find_by_task_id(&pool, task.id).await.unwrap();
    
    // Root must execute before branches
    let root_time = find_step_completion_time(&step_transitions, "root");
    let branch_a_time = find_step_completion_time(&step_transitions, "branch_a");
    let branch_b_time = find_step_completion_time(&step_transitions, "branch_b");
    assert!(root_time < branch_a_time);
    assert!(root_time < branch_b_time);
    
    // Final must execute after all leaves
    let final_time = find_step_completion_time(&step_transitions, "final");
    let leaf_times: Vec<_> = ["leaf_a1", "leaf_a2", "leaf_b1"].iter()
        .map(|name| find_step_completion_time(&step_transitions, name))
        .collect();
    assert!(leaf_times.iter().all(|&time| time < final_time));
}
```

### Day 3-4: SQL Function Integration Tests

#### Test File: `tests/sql_functions/workflow_execution_integration_test.rs`

**Step Readiness with Complex Dependencies**
```rust
#[sqlx::test]
async fn test_step_readiness_complex_dependencies() {
    let pool = setup_test_db().await;
    
    // Create workflow with complex dependency chain
    let task = ComplexWorkflowFactory::create(&pool, ComplexWorkflowConfig {
        pattern: WorkflowPattern::Mixed,
        step_count: 10,
        dependency_density: 0.3, // 30% of possible dependencies
    }).await.unwrap();
    
    // Mark some steps as completed to test readiness calculation
    let step_a = task.workflow_steps.iter().find(|s| s.name == "step_a").unwrap();
    WorkflowStepTransition::create(&pool, NewWorkflowStepTransition {
        workflow_step_id: step_a.id,
        from_state: "pending".to_string(),
        to_state: "completed".to_string(),
        reason: "test completion".to_string(),
        context: Some(json!({"test": true})),
    }).await.unwrap();
    
    // Test viable step discovery
    let viable_steps = ViableStepDiscovery::find_ready_steps(&pool, task.id).await.unwrap();
    
    // Validate that only steps with satisfied dependencies are returned
    for viable_step in &viable_steps {
        let dependencies = WorkflowStepEdge::find_dependencies(&pool, viable_step.step_id).await.unwrap();
        for dep in dependencies {
            let dep_step = WorkflowStep::find(&pool, dep.from_workflow_step_id).await.unwrap();
            let latest_transition = WorkflowStepTransition::latest_for_step(&pool, dep_step.id).await.unwrap();
            assert_eq!(latest_transition.to_state, "completed");
        }
    }
    
    // Validate SQL function performance
    let start_time = Instant::now();
    let _viable_steps = ViableStepDiscovery::find_ready_steps(&pool, task.id).await.unwrap();
    let query_time = start_time.elapsed();
    
    // Should be fast even with complex dependencies
    assert!(query_time < Duration::from_millis(50));
}
```

**Concurrent Step Discovery**
```rust
#[sqlx::test]
async fn test_concurrent_step_discovery_accuracy() {
    let pool = setup_test_db().await;
    
    // Create multiple tasks with overlapping execution windows
    let tasks = futures::future::join_all((0..5).map(|i| {
        DiamondWorkflowFactory::create(&pool, DiamondWorkflowConfig {
            root_step: format!("root_{}", i),
            parallel_steps: vec![format!("parallel_a_{}", i), format!("parallel_b_{}", i)],
            merge_step: format!("merge_{}", i),
        })
    })).await.into_iter().collect::<Result<Vec<_>, _>>().unwrap();
    
    // Complete root steps for all tasks simultaneously
    for task in &tasks {
        let root_step = task.workflow_steps.iter().find(|s| s.name.starts_with("root")).unwrap();
        WorkflowStepTransition::create(&pool, NewWorkflowStepTransition {
            workflow_step_id: root_step.id,
            from_state: "pending".to_string(),
            to_state: "completed".to_string(),
            reason: "concurrent test".to_string(),
            context: None,
        }).await.unwrap();
    }
    
    // Discover viable steps for all tasks concurrently
    let viable_step_futures: Vec<_> = tasks.iter()
        .map(|task| ViableStepDiscovery::find_ready_steps(&pool, task.id))
        .collect();
    
    let results = futures::future::join_all(viable_step_futures).await;
    
    // Each task should have exactly 2 viable steps (the parallel ones)
    for result in results {
        let viable_steps = result.unwrap();
        assert_eq!(viable_steps.len(), 2);
        
        // Both should be the parallel steps for this task
        assert!(viable_steps.iter().any(|s| s.step_name.contains("parallel_a")));
        assert!(viable_steps.iter().any(|s| s.step_name.contains("parallel_b")));
    }
}
```

### Day 5: Ruby Binding Basic Tests

#### Test File: `bindings/ruby/spec/tasker_core_integration_spec.rb`

**Module Loading and Initialization**
```ruby
RSpec.describe TaskerCore do
  describe "module initialization" do
    it "loads without errors" do
      expect { require 'tasker_core' }.not_to raise_error
    end
    
    it "defines expected constants" do
      expect(TaskerCore::RUST_VERSION).to be_present
      expect(TaskerCore::STATUS).to eq("rails_integration")
      expect(TaskerCore::FEATURES).to include("handler_foundation")
    end
    
    it "defines error hierarchy" do
      expect(TaskerCore::Error).to be < StandardError
      expect(TaskerCore::OrchestrationError).to be < TaskerCore::Error
      expect(TaskerCore::DatabaseError).to be < TaskerCore::Error
      expect(TaskerCore::StateTransitionError).to be < TaskerCore::Error
    end
  end
end
```

**Handler Instantiation**
```ruby
RSpec.describe TaskerCore::BaseTaskHandler do
  let(:database_url) { "postgresql://localhost/tasker_test" }
  
  describe "#initialize_task" do
    it "creates task from request" do
      handler = described_class.new
      task_request = build_task_request(
        name: "test_handler",
        namespace: "testing", 
        version: "1.0.0"
      )
      
      result = handler.initialize_task(task_request)
      
      expect(result).to be_a(Hash)
      expect(result["status"]).to eq("created")
      expect(result["task_id"]).to be_present
    end
    
    it "handles missing YAML configuration gracefully" do
      handler = described_class.new
      task_request = build_task_request(name: "nonexistent_handler")
      
      expect {
        handler.initialize_task(task_request)
      }.to raise_error(TaskerCore::ValidationError, /handler not found/)
    end
  end
  
  describe "#handle" do
    it "delegates to Rust orchestration" do
      handler = described_class.new
      
      # Create a task first
      task_result = handler.initialize_task(build_task_request)
      task_id = task_result["task_id"]
      
      # Execute the task
      result = handler.handle(task_id)
      
      expect(result).to be_a(Hash)
      expect(result["status"]).to be_in(["complete", "in_progress", "failed"])
    end
  end
end
```

**Context Serialization**
```ruby
RSpec.describe "Context serialization" do
  it "handles Ruby to Rust data conversion" do
    context_data = {
      "string_field" => "test value",
      "integer_field" => 42,
      "array_field" => [1, 2, 3],
      "hash_field" => { "nested" => "value" },
      "time_field" => Time.now
    }
    
    # This would be called internally during handler execution
    # Testing the conversion boundary
    expect {
      TaskerCore::TestUtilities.test_context_conversion(context_data)
    }.not_to raise_error
  end
  
  it "preserves data types through conversion" do
    original_data = {
      "count" => 100,
      "rate" => 0.95,
      "active" => true,
      "tags" => ["urgent", "payment"]
    }
    
    converted_data = TaskerCore::TestUtilities.round_trip_conversion(original_data)
    
    expect(converted_data["count"]).to eq(100)
    expect(converted_data["rate"]).to be_within(0.001).of(0.95)
    expect(converted_data["active"]).to be(true)
    expect(converted_data["tags"]).to eq(["urgent", "payment"])
  end
end
```

## Test Infrastructure

### MockFrameworkIntegration
```rust
pub struct MockFrameworkIntegration {
    step_delays: HashMap<String, Duration>,
    step_results: HashMap<String, StepResult>,
    execution_log: Arc<Mutex<Vec<StepExecution>>>,
}

impl MockFrameworkIntegration {
    pub fn new() -> Self { /* ... */ }
    
    pub fn with_delays(delays: Vec<(&str, u64)>) -> Self { /* ... */ }
    
    pub fn set_step_result(&mut self, step_name: &str, result: StepResult) { /* ... */ }
    
    pub fn get_execution_log(&self) -> Vec<StepExecution> { /* ... */ }
}

#[async_trait]
impl FrameworkIntegration for MockFrameworkIntegration {
    async fn execute_single_step(
        &self,
        step: &ViableStep,
        context: &TaskContext,
    ) -> Result<StepResult, OrchestrationError> {
        // Log execution for test validation
        self.execution_log.lock().unwrap().push(StepExecution {
            step_id: step.step_id,
            step_name: step.step_name.clone(),
            started_at: Utc::now(),
        });
        
        // Apply configured delay
        if let Some(delay) = self.step_delays.get(&step.step_name) {
            tokio::time::sleep(*delay).await;
        }
        
        // Return configured result or default success
        Ok(self.step_results.get(&step.step_name)
            .cloned()
            .unwrap_or_else(|| StepResult::Completed {
                data: json!({"mock": true}),
                duration_ms: 50,
            }))
    }
}
```

### Test Database Setup
```rust
pub async fn setup_test_db() -> PgPool {
    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgresql://localhost/tasker_test".to_string());
    
    let pool = PgPool::connect(&database_url).await.unwrap();
    
    // Run migrations
    sqlx::migrate!("./migrations").run(&pool).await.unwrap();
    
    // Clean up any existing test data
    sqlx::query("TRUNCATE workflow_step_transitions, task_transitions, workflow_step_edges, workflow_steps, tasks CASCADE")
        .execute(&pool)
        .await
        .unwrap();
    
    pool
}
```

## Success Criteria

### Week 1 Completion Criteria
- 🔄 **Complex workflow patterns execute correctly** (IN PROGRESS - Step state initialization issue)
- ✅ **SQL schema alignment validated** (Fixed error_steps, named_step_id type mismatches)
- ✅ **Type system integrity confirmed** (Fixed BigDecimal conversions)
- 🔄 **SQL functions handle concurrent operations properly** (Basic validation complete, step state issue remains)
- ⏳ **Ruby bindings load and instantiate handlers without errors** (Pending)
- ⏳ **Context serialization works bidirectionally** (Pending)
- 🔄 **State machine transitions occur during workflow execution** (State initialization issue blocking)
- ⏳ **Event publishing flows from Rust to mock subscribers** (Pending)

### Critical Placeholders Fixed Through Testing ✅
1. **SQL Schema Alignment** - Fixed `error_steps` vs `failed_steps` column mismatch
2. **Type System Integrity** - Fixed BigDecimal to f64 conversion in TaskFinalizer
3. **SQL Type Compatibility** - Fixed `named_step_id` i64 vs i32 mismatch across components
4. **Database Function Integration** - Verified get_task_execution_context alignment

### Current Active Issue 🔍
**Step State Initialization**: Workflow steps created in 'unknown' state instead of 'pending'
- **Test**: `test_orchestration_with_real_task` successfully creates task + steps
- **Failure Point**: OrchestrationCoordinator → ViableStepDiscovery → SQL function returns 'unknown' state
- **Next Investigation**: WorkflowStepFactory state initialization vs SQL function state retrieval

### Test Coverage Goals
- **Complex Workflows**: Linear, Diamond, Parallel, Tree patterns
- **SQL Functions**: Step readiness, concurrent discovery, performance validation
- **Ruby Integration**: Loading, instantiation, context conversion, error handling
- **State Management**: Task and step state transitions during execution
- **Performance**: All operations complete within acceptable time bounds

---
**Purpose**: Force completion of critical placeholders through comprehensive testing  
**Timeline**: Week 1 of Phase 1  
**Success Metric**: All tests pass, no remaining critical placeholders