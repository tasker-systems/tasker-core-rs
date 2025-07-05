# Tasker State Machine Implementation Strategy

## Executive Summary

Based on comprehensive analysis of Rails Statesman implementation and Rust state machine libraries, we recommend **smlang** as the primary state machine library with custom persistence integration.

## Detailed Analysis

### Rails Statesman Requirements
- Complex guard conditions (dependency checking)
- Before/after callbacks for event publishing
- Custom transition models (not built-in persistence)
- Async event publishing integration
- Rich metadata support for debugging
- Idempotent transition handling
- Complex business logic integration

### Rust Library Recommendation: smlang

**Why smlang:**
- ✅ **Guards**: Essential for dependency checking (`step_dependencies_met?`)
- ✅ **Actions**: Perfect for callbacks and event publishing
- ✅ **Async Support**: Matches our async/await architecture
- ✅ **Pattern Matching**: Handles complex transition logic
- ✅ **Entry/Exit Actions**: Lifecycle management
- ✅ **DSL Expressiveness**: Complex workflow state logic

### Implementation Architecture

```rust
// State definitions
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TaskState {
    Pending,
    InProgress, 
    Complete,
    Error,
    Cancelled,
    ResolvedManually,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum WorkflowStepState {
    Pending,
    InProgress,
    Complete, 
    Error,
    Cancelled,
    ResolvedManually,
}

// Events
#[derive(Debug)]
pub enum TaskEvent {
    Start,
    Complete,
    Fail(String),
    Cancel,
    ResolveManually,
    Reset,
}

#[derive(Debug)]
pub enum StepEvent {
    Start,
    Complete(serde_json::Value), // results
    Fail(String),
    Cancel,
    ResolveManually,
    Retry,
}
```

### Guard Implementation Strategy

```rust
// Task guards
async fn all_steps_complete(task: &Task, pool: &PgPool) -> Result<bool, GuardError> {
    // Check if all workflow steps are in terminal states
    let incomplete_count = sqlx::query!(
        "SELECT COUNT(*) as count FROM tasker_workflow_steps ws
         LEFT JOIN (
             SELECT DISTINCT ON (workflow_step_id) workflow_step_id, to_state
             FROM tasker_workflow_step_transitions 
             WHERE most_recent = true
             ORDER BY workflow_step_id, sort_key DESC
         ) current_states ON current_states.workflow_step_id = ws.workflow_step_id
         WHERE ws.task_id = $1 
           AND (current_states.to_state IS NULL 
                OR current_states.to_state NOT IN ('complete', 'resolved_manually'))",
        task.task_id
    )
    .fetch_one(pool)
    .await?
    .count;
    
    Ok(incomplete_count.unwrap_or(1) == 0)
}

// Step guards  
async fn step_dependencies_met(step: &WorkflowStep, pool: &PgPool) -> Result<bool, GuardError> {
    // Complex dependency checking logic
    let unmet_dependencies = sqlx::query!(
        "SELECT COUNT(*) as count FROM tasker_workflow_step_edges wse
         INNER JOIN tasker_workflow_steps parent_ws ON wse.from_step_id = parent_ws.workflow_step_id
         LEFT JOIN (
             SELECT DISTINCT ON (workflow_step_id) workflow_step_id, to_state
             FROM tasker_workflow_step_transitions 
             WHERE most_recent = true
             ORDER BY workflow_step_id, sort_key DESC
         ) parent_states ON parent_states.workflow_step_id = parent_ws.workflow_step_id
         WHERE wse.to_step_id = $1
           AND (parent_states.to_state IS NULL 
                OR parent_states.to_state NOT IN ('complete', 'resolved_manually'))",
        step.workflow_step_id
    )
    .fetch_one(pool)
    .await?
    .count;
    
    Ok(unmet_dependencies.unwrap_or(1) == 0)
}
```

### Event Publishing Integration

```rust
// Action for event publishing
async fn publish_transition_event<T>(
    entity: &T, 
    from_state: Option<State>, 
    to_state: State,
    event_publisher: &EventPublisher
) -> Result<(), ActionError> 
where T: TransitionEventMapping 
{
    let event_name = entity.get_event_name(from_state, to_state);
    let payload = entity.build_event_payload(from_state, to_state);
    
    event_publisher.publish(event_name, payload).await?;
    Ok(())
}

// Trait for event mapping
trait TransitionEventMapping {
    fn get_event_name(&self, from: Option<State>, to: State) -> &'static str;
    fn build_event_payload(&self, from: Option<State>, to: State) -> serde_json::Value;
}
```

### State Machine Wrapper

```rust
pub struct TaskStateMachine {
    task: Task,
    pool: PgPool,
    event_publisher: EventPublisher,
    state_machine: smlang::StateMachine</* generated types */>,
}

impl TaskStateMachine {
    pub async fn new(task: Task, pool: PgPool, event_publisher: EventPublisher) -> Self {
        let current_state = Self::resolve_current_state(&task, &pool).await
            .unwrap_or(TaskState::Pending);
            
        Self {
            task,
            pool,
            event_publisher,
            state_machine: smlang::StateMachine::new(current_state),
        }
    }
    
    pub async fn transition(&mut self, event: TaskEvent) -> Result<TaskState, TransitionError> {
        // Attempt transition with guards and actions
        let result = self.state_machine.process_event(event).await?;
        
        // Persist transition
        self.persist_transition(result.from_state, result.to_state, &event).await?;
        
        Ok(result.to_state)
    }
    
    async fn persist_transition(
        &self, 
        from_state: Option<TaskState>, 
        to_state: TaskState, 
        event: &TaskEvent
    ) -> Result<(), PersistenceError> {
        // Create TaskTransition record
        let sort_key = self.get_next_sort_key().await?;
        
        sqlx::query!(
            "INSERT INTO tasker_task_transitions 
             (task_id, from_state, to_state, sort_key, most_recent, metadata)
             VALUES ($1, $2, $3, $4, true, $5)",
            self.task.task_id,
            from_state.map(|s| s.to_string()),
            to_state.to_string(),
            sort_key,
            serde_json::json!({
                "event": format!("{:?}", event),
                "timestamp": chrono::Utc::now(),
            })
        )
        .execute(&self.pool)
        .await?;
        
        // Update most_recent flags
        sqlx::query!(
            "UPDATE tasker_task_transitions 
             SET most_recent = false 
             WHERE task_id = $1 AND sort_key < $2",
            self.task.task_id,
            sort_key
        )
        .execute(&self.pool)
        .await?;
        
        Ok(())
    }
}
```

## Implementation Phases

### Phase 1: Core State Machine Setup
1. Add smlang dependency to Cargo.toml
2. Define state and event enums
3. Create basic state machine definitions
4. Implement state resolution from transition tables

### Phase 2: Guard Implementation  
1. Implement dependency checking guards
2. Add business logic validation
3. Create async guard infrastructure
4. Add comprehensive error handling

### Phase 3: Event Integration
1. Implement event publishing actions
2. Create transition event mapping
3. Add lifecycle event integration
4. Build event payload construction

### Phase 4: Persistence Layer
1. Custom transition persistence
2. State resolution optimization
3. Race condition handling
4. Audit trail maintenance

### Phase 5: Advanced Features
1. Idempotent transition handling
2. Rich metadata support
3. Performance optimizations
4. Integration with query builder

## Testing Strategy

### State Machine Tests
```rust
#[tokio::test]
async fn test_task_complete_requires_all_steps_complete() {
    let (task, steps) = create_test_workflow().await;
    let mut sm = TaskStateMachine::new(task, pool, publisher).await;
    
    // Should fail - not all steps complete
    assert!(sm.transition(TaskEvent::Complete).await.is_err());
    
    // Complete all steps
    for step in steps {
        complete_step(step).await;
    }
    
    // Should succeed now
    assert_eq!(sm.transition(TaskEvent::Complete).await?, TaskState::Complete);
}

#[tokio::test]
async fn test_step_start_requires_dependencies_met() {
    let workflow = create_diamond_workflow().await;
    let convergence_step = workflow.get_step("merge_branches");
    let mut sm = StepStateMachine::new(convergence_step, pool, publisher).await;
    
    // Should fail - dependencies not met
    assert!(sm.transition(StepEvent::Start).await.is_err());
    
    // Complete dependencies
    complete_step(workflow.get_step("branch_one_validate")).await;
    complete_step(workflow.get_step("branch_two_validate")).await;
    
    // Should succeed now
    assert_eq!(sm.transition(StepEvent::Start).await?, StepState::InProgress);
}
```

## Migration Strategy

### Gradual Integration
1. **Parallel Implementation**: Run both Rails and Rust state machines initially
2. **Validation Mode**: Compare results to ensure equivalent behavior  
3. **Feature Flags**: Gradually enable Rust state machine for different operations
4. **Monitoring**: Track performance and correctness metrics
5. **Full Migration**: Switch to Rust-only once validated

This strategy ensures we maintain the sophisticated business logic of the Rails implementation while gaining the performance and safety benefits of Rust's type system and async capabilities.