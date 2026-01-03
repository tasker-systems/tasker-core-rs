/**
 * FFI type definitions for TypeScript/JavaScript workers.
 *
 * These types are automatically generated from Rust DTOs using ts-rs.
 * Run `cargo make generate-bindings` to regenerate the base types.
 *
 * This file re-exports generated types with API-friendly names and adds
 * runtime-specific types that aren't part of the DTO layer.
 */

// =============================================================================
// Generated Types (from ts-rs)
// =============================================================================
// These types are automatically generated from Rust DTOs in src-rust/dto.rs.
// Re-exported with API-friendly names (without Dto suffix).

import type { DependencyResultDto } from './generated/DependencyResultDto';
import type { FfiDispatchMetricsDto } from './generated/FfiDispatchMetricsDto';
import type { FfiStepEventDto } from './generated/FfiStepEventDto';
import type { HandlerDefinitionDto } from './generated/HandlerDefinitionDto';
import type { RetryConfigurationDto } from './generated/RetryConfigurationDto';
import type { StepDefinitionDto } from './generated/StepDefinitionDto';
import type { StepExecutionErrorDto } from './generated/StepExecutionErrorDto';
import type { TaskDto } from './generated/TaskDto';
import type { WorkflowStepDto } from './generated/WorkflowStepDto';

// Re-export with API-friendly names
export type Task = TaskDto;
export type WorkflowStep = WorkflowStepDto;
export type HandlerDefinition = HandlerDefinitionDto;
export type RetryConfiguration = RetryConfigurationDto;
export type StepDefinition = StepDefinitionDto;
export type StepExecutionError = StepExecutionErrorDto;
export type DependencyResult = DependencyResultDto;
export type FfiStepEvent = FfiStepEventDto;
export type FfiDispatchMetrics = FfiDispatchMetricsDto;

// Also export the Dto-suffixed types for explicit usage
export type {
  DependencyResultDto,
  FfiDispatchMetricsDto,
  FfiStepEventDto,
  HandlerDefinitionDto,
  RetryConfigurationDto,
  StepDefinitionDto,
  StepExecutionErrorDto,
  TaskDto,
  WorkflowStepDto,
};

// =============================================================================
// Runtime Types (not generated)
// =============================================================================
// These types are specific to the TypeScript runtime and don't exist in Rust DTOs.

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

// =============================================================================
// Domain Event Types (for in-process event polling)
// =============================================================================

/**
 * Metadata attached to every domain event from FFI.
 */
export interface FfiDomainEventMetadata {
  taskUuid: string;
  stepUuid: string | null;
  stepName: string | null;
  namespace: string;
  correlationId: string;
  firedAt: string;
  firedBy: string | null;
}

/**
 * Domain event from in-process event bus (fast path).
 *
 * Used for real-time notifications that don't require guaranteed delivery
 * (e.g., metrics updates, logging, notifications).
 */
export interface FfiDomainEvent {
  eventId: string;
  eventName: string;
  eventVersion: string;
  metadata: FfiDomainEventMetadata;
  payload: Record<string, unknown>;
}

// =============================================================================
// Checkpoint Types (TAS-125)
// =============================================================================

/**
 * Checkpoint yield data for batch processing handlers (TAS-125)
 *
 * Sent when a handler wants to persist progress and be re-dispatched.
 * The step remains in_process and will be re-executed with the checkpoint
 * data available.
 */
export interface CheckpointYieldData {
  step_uuid: string;
  cursor: unknown; // Flexible: number | string | object
  items_processed: number;
  accumulated_results?: Record<string, unknown>;
}
