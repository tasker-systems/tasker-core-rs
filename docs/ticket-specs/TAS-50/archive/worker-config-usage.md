=== Tasker-Worker Configuration Usage Analysis ===

## Database Configuration

## Worker Configuration
- tasker-worker/src/web/state.rs:48:        let worker_config = config.worker.clone();
- tasker-worker/src/bootstrap.rs:182:            deployment_mode_hint: Some(config.event_systems.worker.deployment_mode.to_string()),

## Queue Configuration
- tasker-worker/src/worker/orchestration_result_sender.rs:135:        let queue_config = tasker_config.queues.clone();
- tasker-worker/src/worker/command_processor.rs:201:        let queue_config = context.tasker_config.queues.clone();

## MPSC Channels Configuration

## Event Systems Configuration
- tasker-worker/src/bootstrap.rs:182:            deployment_mode_hint: Some(config.event_systems.worker.deployment_mode.to_string()),

## Execution Configuration

## Backoff Configuration

## Telemetry Configuration

## Handler Configuration
- tasker-worker/src/worker/task_template_manager.rs:664:            .map(|st| st.handler.callable.clone())
- tasker-worker/src/worker/task_template_manager.rs:673:            if step.handler.callable.contains("FFI") {
- tasker-worker/src/worker/task_template_manager.rs:677:            if step.handler.callable.contains("External") {
- tasker-worker/src/worker/task_template_manager.rs:681:            if step.handler.callable.contains("Database") {
- tasker-worker/src/worker/event_publisher.rs:128:                    callable = %task_sequence_step.step_definition.handler.callable,

