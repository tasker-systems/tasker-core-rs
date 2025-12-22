/**
 * FFI type definitions for TypeScript/JavaScript workers.
 *
 * These types mirror the Rust FFI structures and provide strongly-typed
 * access to the worker functionality.
 */

/**
 * Task information from TaskForOrchestration
 */
export interface Task {
  task_uuid: string;
  named_task_uuid: string;
  name: string;
  namespace: string;
  version: string;
  context: Record<string, unknown> | null;
  correlation_id: string;
  parent_correlation_id: string | null;
  complete: boolean;
  priority: number;
  initiator: string | null;
  source_system: string | null;
  reason: string | null;
  tags: Record<string, unknown> | null;
  identity_hash: string;
  created_at: string;
  updated_at: string;
  requested_at: string;
}

/**
 * Workflow step information from WorkflowStepWithName
 */
export interface WorkflowStep {
  workflow_step_uuid: string;
  task_uuid: string;
  named_step_uuid: string;
  name: string;
  template_step_name: string;
  retryable: boolean;
  max_attempts: number | null;
  attempts: number | null;
  in_process: boolean;
  processed: boolean;
  skippable: boolean;
  inputs: Record<string, unknown> | null;
  results: Record<string, unknown> | null;
  backoff_request_seconds: number | null;
  processed_at: string | null;
  last_attempted_at: string | null;
  created_at: string;
  updated_at: string;
}

/**
 * Handler definition from StepDefinition
 */
export interface HandlerDefinition {
  callable: string;
  initialization: Record<string, unknown>;
}

/**
 * Retry configuration from RetryConfiguration
 */
export interface RetryConfiguration {
  retryable: boolean;
  max_attempts: number;
  backoff: string;
  backoff_base_ms: number | null;
  max_backoff_ms: number | null;
}

/**
 * Step definition from StepDefinition
 */
export interface StepDefinition {
  name: string;
  description: string | null;
  handler: HandlerDefinition;
  system_dependency: string | null;
  dependencies: string[];
  timeout_seconds: number | null;
  retry: RetryConfiguration;
}

/**
 * Step execution error from StepExecutionError
 */
export interface StepExecutionError {
  message: string;
  error_type: string | null;
  retryable: boolean;
  status_code: number | null;
  backtrace: string[] | null;
}

/**
 * Dependency step result from StepExecutionResult
 */
export interface DependencyResult {
  step_uuid: string;
  success: boolean;
  result: unknown;
  status: string;
  error: StepExecutionError | null;
}

/**
 * Full step event from FfiStepEvent
 *
 * This is the primary structure received when polling for step events.
 * Contains all information needed to execute a step handler.
 */
export interface FfiStepEvent {
  // Event identification
  event_id: string;
  task_uuid: string;
  step_uuid: string;
  correlation_id: string;

  // Trace context
  trace_id: string | null;
  span_id: string | null;

  // Task correlation IDs
  task_correlation_id: string;
  parent_correlation_id: string | null;

  // Task information
  task: Task;

  // Workflow step information
  workflow_step: WorkflowStep;

  // Step definition
  step_definition: StepDefinition;

  // Dependency results from parent steps
  dependency_results: Record<string, DependencyResult>;
}

/**
 * Bootstrap configuration for the worker
 */
export interface BootstrapConfig {
  worker_id?: string;
  log_level?: 'trace' | 'debug' | 'info' | 'warn' | 'error';
  database_url?: string;
  [key: string]: unknown;
}

/**
 * Bootstrap result from Rust
 */
export interface BootstrapResult {
  success: boolean;
  status: 'started' | 'already_running' | 'error';
  message: string;
  worker_id?: string;
  error?: string;
}

/**
 * Worker status from get_worker_status
 */
export interface WorkerStatus {
  success: boolean;
  running: boolean;
  status?: 'stopped';
  worker_id?: string;
  environment?: string;
  worker_core_status?: string;
  web_api_enabled?: boolean;
  supported_namespaces?: string[];
  database_pool_size?: number;
  database_pool_idle?: number;
}

/**
 * Stop result from stop_worker
 */
export interface StopResult {
  success: boolean;
  status: 'stopped' | 'not_running' | 'error';
  message: string;
  worker_id?: string;
  error?: string;
}

/**
 * FFI dispatch metrics from FfiDispatchMetrics
 */
export interface FfiDispatchMetrics {
  pending_count: number;
  starvation_detected: boolean;
  starving_event_count: number;
  oldest_pending_age_ms: number | null;
  newest_pending_age_ms: number | null;
}

/**
 * Step execution result to send back to Rust
 *
 * This structure matches tasker_shared::messaging::StepExecutionResult
 */
export interface StepExecutionResult {
  step_uuid: string;
  success: boolean;
  result: Record<string, unknown>;
  metadata: StepExecutionMetadata;
  status: 'completed' | 'failed' | 'error';
  error?: StepExecutionError;
  orchestration_metadata?: OrchestrationMetadata;
}

/**
 * Metadata about step execution
 */
export interface StepExecutionMetadata {
  execution_time_ms: number;
  worker_id?: string;
  handler_name?: string;
  attempt_number?: number;
  [key: string]: unknown;
}

/**
 * Orchestration metadata for routing
 */
export interface OrchestrationMetadata {
  routing_context?: Record<string, unknown>;
  next_steps?: string[];
  [key: string]: unknown;
}

/**
 * Log fields for structured logging
 */
export interface LogFields {
  [key: string]: string | number | boolean | null;
}
