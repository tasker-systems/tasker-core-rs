/**
 * FFI type coherence tests.
 *
 * Verifies that FFI types are correctly structured and can be
 * used for JSON serialization/deserialization as expected.
 */

import { describe, expect, it } from 'bun:test';
import type {
  BootstrapConfig,
  BootstrapResult,
  DependencyResult,
  FfiDispatchMetrics,
  FfiStepEvent,
  HandlerDefinition,
  LogFields,
  RetryConfiguration,
  StepDefinition,
  StepExecutionError,
  StepExecutionResult,
  StopResult,
  Task,
  WorkerStatus,
  WorkflowStep,
} from '../../../src/ffi/types.js';

describe('FFI Types', () => {
  describe('Task', () => {
    it('can be created with required fields', () => {
      const task: Task = createValidTask();
      expect(task.task_uuid).toBeDefined();
      expect(task.name).toBeDefined();
      expect(task.namespace).toBeDefined();
    });

    it('serializes to JSON correctly', () => {
      const task = createValidTask();
      const json = JSON.stringify(task);
      const parsed = JSON.parse(json) as Task;

      expect(parsed.task_uuid).toBe(task.task_uuid);
      expect(parsed.name).toBe(task.name);
      expect(parsed.namespace).toBe(task.namespace);
    });

    it('supports optional nullable fields', () => {
      const task = createValidTask();
      expect(task.context).toBeNull();
      expect(task.parent_correlation_id).toBeNull();
      expect(task.initiator).toBeNull();
    });
  });

  describe('WorkflowStep', () => {
    it('can be created with required fields', () => {
      const step: WorkflowStep = createValidWorkflowStep();
      expect(step.workflow_step_uuid).toBeDefined();
      expect(step.task_uuid).toBeDefined();
      expect(step.name).toBeDefined();
    });

    it('serializes to JSON correctly', () => {
      const step = createValidWorkflowStep();
      const json = JSON.stringify(step);
      const parsed = JSON.parse(json) as WorkflowStep;

      expect(parsed.workflow_step_uuid).toBe(step.workflow_step_uuid);
      expect(parsed.name).toBe(step.name);
      expect(parsed.retryable).toBe(step.retryable);
    });

    it('supports optional nullable fields', () => {
      const step = createValidWorkflowStep();
      expect(step.max_attempts).toBe(3);
      expect(step.inputs).toBeNull();
      expect(step.results).toBeNull();
    });
  });

  describe('HandlerDefinition', () => {
    it('can be created with required fields', () => {
      const handler: HandlerDefinition = {
        callable: 'MyHandler',
        initialization: { key: 'value' },
      };

      expect(handler.callable).toBe('MyHandler');
      expect(handler.initialization).toEqual({ key: 'value' });
    });

    it('supports empty initialization', () => {
      const handler: HandlerDefinition = {
        callable: 'SimpleHandler',
        initialization: {},
      };

      expect(handler.initialization).toEqual({});
    });
  });

  describe('RetryConfiguration', () => {
    it('can be created with all fields', () => {
      const retry: RetryConfiguration = {
        retryable: true,
        max_attempts: 5,
        backoff: 'exponential',
        backoff_base_ms: 1000,
        max_backoff_ms: 60000,
      };

      expect(retry.retryable).toBe(true);
      expect(retry.max_attempts).toBe(5);
      expect(retry.backoff).toBe('exponential');
    });

    it('supports nullable backoff fields', () => {
      const retry: RetryConfiguration = {
        retryable: false,
        max_attempts: 1,
        backoff: 'none',
        backoff_base_ms: null,
        max_backoff_ms: null,
      };

      expect(retry.backoff_base_ms).toBeNull();
      expect(retry.max_backoff_ms).toBeNull();
    });
  });

  describe('StepDefinition', () => {
    it('can be created with required fields', () => {
      const definition: StepDefinition = createValidStepDefinition();
      expect(definition.name).toBeDefined();
      expect(definition.handler).toBeDefined();
      expect(definition.retry).toBeDefined();
    });

    it('serializes to JSON with nested structures', () => {
      const definition = createValidStepDefinition();
      const json = JSON.stringify(definition);
      const parsed = JSON.parse(json) as StepDefinition;

      expect(parsed.handler.callable).toBe(definition.handler.callable);
      expect(parsed.retry.max_attempts).toBe(definition.retry.max_attempts);
    });
  });

  describe('StepExecutionError', () => {
    it('can be created with required fields', () => {
      const error: StepExecutionError = {
        message: 'Something went wrong',
        error_type: 'ValidationError',
        retryable: true,
        status_code: 400,
        backtrace: ['line1', 'line2'],
      };

      expect(error.message).toBe('Something went wrong');
      expect(error.retryable).toBe(true);
    });

    it('supports nullable optional fields', () => {
      const error: StepExecutionError = {
        message: 'Error',
        error_type: null,
        retryable: false,
        status_code: null,
        backtrace: null,
      };

      expect(error.error_type).toBeNull();
      expect(error.status_code).toBeNull();
      expect(error.backtrace).toBeNull();
    });
  });

  describe('DependencyResult', () => {
    it('can be created for successful dependency', () => {
      const result: DependencyResult = {
        step_uuid: 'step-123',
        success: true,
        result: { data: 'output' },
        status: 'completed',
        error: null,
      };

      expect(result.success).toBe(true);
      expect(result.error).toBeNull();
    });

    it('can be created for failed dependency', () => {
      const result: DependencyResult = {
        step_uuid: 'step-123',
        success: false,
        result: null,
        status: 'failed',
        error: {
          message: 'Dependency failed',
          error_type: 'RuntimeError',
          retryable: false,
          status_code: 500,
          backtrace: null,
        },
      };

      expect(result.success).toBe(false);
      expect(result.error).not.toBeNull();
    });
  });

  describe('FfiStepEvent', () => {
    it('can be created with all required fields', () => {
      const event: FfiStepEvent = createValidFfiStepEvent();

      expect(event.event_id).toBeDefined();
      expect(event.task_uuid).toBeDefined();
      expect(event.step_uuid).toBeDefined();
      expect(event.task).toBeDefined();
      expect(event.workflow_step).toBeDefined();
      expect(event.step_definition).toBeDefined();
    });

    it('serializes to JSON correctly', () => {
      const event = createValidFfiStepEvent();
      const json = JSON.stringify(event);
      const parsed = JSON.parse(json) as FfiStepEvent;

      expect(parsed.event_id).toBe(event.event_id);
      expect(parsed.task.task_uuid).toBe(event.task.task_uuid);
      expect(parsed.workflow_step.name).toBe(event.workflow_step.name);
    });

    it('supports dependency results map', () => {
      const event = createValidFfiStepEvent();
      event.dependency_results = {
        'dep-step-1': {
          step_uuid: 'dep-step-1',
          success: true,
          result: { value: 42 },
          status: 'completed',
          error: null,
        },
      };

      expect(event.dependency_results['dep-step-1'].success).toBe(true);
    });
  });

  describe('BootstrapConfig', () => {
    it('can be created with minimal fields', () => {
      const config: BootstrapConfig = {};
      expect(config).toBeDefined();
    });

    it('supports all optional fields', () => {
      const config: BootstrapConfig = {
        worker_id: 'worker-123',
        log_level: 'debug',
        database_url: 'postgresql://localhost/tasker',
        custom_field: 'custom_value',
      };

      expect(config.worker_id).toBe('worker-123');
      expect(config.log_level).toBe('debug');
      expect(config.custom_field).toBe('custom_value');
    });
  });

  describe('BootstrapResult', () => {
    it('can represent successful start', () => {
      const result: BootstrapResult = {
        success: true,
        status: 'started',
        message: 'Worker started successfully',
        worker_id: 'worker-123',
      };

      expect(result.success).toBe(true);
      expect(result.status).toBe('started');
    });

    it('can represent already running state', () => {
      const result: BootstrapResult = {
        success: true,
        status: 'already_running',
        message: 'Worker is already running',
        worker_id: 'worker-123',
      };

      expect(result.status).toBe('already_running');
    });

    it('can represent error state', () => {
      const result: BootstrapResult = {
        success: false,
        status: 'error',
        message: 'Failed to start worker',
        error: 'Database connection failed',
      };

      expect(result.success).toBe(false);
      expect(result.error).toBeDefined();
    });
  });

  describe('WorkerStatus', () => {
    it('can represent running worker', () => {
      const status: WorkerStatus = {
        success: true,
        running: true,
        worker_id: 'worker-123',
        environment: 'production',
        worker_core_status: 'active',
        web_api_enabled: true,
        supported_namespaces: ['orders', 'payments'],
        database_pool_size: 10,
        database_pool_idle: 8,
      };

      expect(status.running).toBe(true);
      expect(status.supported_namespaces).toContain('orders');
    });

    it('can represent stopped worker', () => {
      const status: WorkerStatus = {
        success: true,
        running: false,
        status: 'stopped',
      };

      expect(status.running).toBe(false);
      expect(status.status).toBe('stopped');
    });
  });

  describe('StopResult', () => {
    it('can represent successful stop', () => {
      const result: StopResult = {
        success: true,
        status: 'stopped',
        message: 'Worker stopped gracefully',
        worker_id: 'worker-123',
      };

      expect(result.success).toBe(true);
      expect(result.status).toBe('stopped');
    });

    it('can represent not running state', () => {
      const result: StopResult = {
        success: true,
        status: 'not_running',
        message: 'Worker was not running',
      };

      expect(result.status).toBe('not_running');
    });
  });

  describe('FfiDispatchMetrics', () => {
    it('can represent healthy state', () => {
      const metrics: FfiDispatchMetrics = {
        pending_count: 5,
        starvation_detected: false,
        starving_event_count: 0,
        oldest_pending_age_ms: 100,
        newest_pending_age_ms: 10,
      };

      expect(metrics.starvation_detected).toBe(false);
      expect(metrics.starving_event_count).toBe(0);
    });

    it('can represent starvation state', () => {
      const metrics: FfiDispatchMetrics = {
        pending_count: 50,
        starvation_detected: true,
        starving_event_count: 10,
        oldest_pending_age_ms: 30000,
        newest_pending_age_ms: 5000,
      };

      expect(metrics.starvation_detected).toBe(true);
      expect(metrics.starving_event_count).toBe(10);
    });

    it('supports null age fields when empty', () => {
      const metrics: FfiDispatchMetrics = {
        pending_count: 0,
        starvation_detected: false,
        starving_event_count: 0,
        oldest_pending_age_ms: null,
        newest_pending_age_ms: null,
      };

      expect(metrics.oldest_pending_age_ms).toBeNull();
    });
  });

  describe('StepExecutionResult', () => {
    it('can represent successful completion', () => {
      const result: StepExecutionResult = {
        step_uuid: 'step-123',
        success: true,
        result: { output: 'data' },
        metadata: {
          execution_time_ms: 150,
          worker_id: 'worker-123',
          handler_name: 'MyHandler',
        },
        status: 'completed',
      };

      expect(result.success).toBe(true);
      expect(result.status).toBe('completed');
    });

    it('can represent failure', () => {
      const result: StepExecutionResult = {
        step_uuid: 'step-123',
        success: false,
        result: {},
        metadata: {
          execution_time_ms: 50,
        },
        status: 'failed',
        error: {
          message: 'Handler threw exception',
          error_type: 'RuntimeError',
          retryable: true,
          status_code: null,
          backtrace: null,
        },
      };

      expect(result.success).toBe(false);
      expect(result.status).toBe('failed');
      expect(result.error).toBeDefined();
    });

    it('supports orchestration metadata', () => {
      const result: StepExecutionResult = {
        step_uuid: 'step-123',
        success: true,
        result: {},
        metadata: { execution_time_ms: 100 },
        status: 'completed',
        orchestration_metadata: {
          routing_context: { region: 'us-east' },
          next_steps: ['step-456', 'step-789'],
        },
      };

      expect(result.orchestration_metadata?.next_steps).toContain('step-456');
    });
  });

  describe('LogFields', () => {
    it('supports string values', () => {
      const fields: LogFields = {
        request_id: 'req-123',
        handler: 'MyHandler',
      };

      expect(fields.request_id).toBe('req-123');
    });

    it('supports numeric values', () => {
      const fields: LogFields = {
        execution_time_ms: 150,
        attempt: 2,
      };

      expect(fields.execution_time_ms).toBe(150);
    });

    it('supports boolean values', () => {
      const fields: LogFields = {
        success: true,
        retryable: false,
      };

      expect(fields.success).toBe(true);
    });

    it('supports null values', () => {
      const fields: LogFields = {
        optional_field: null,
      };

      expect(fields.optional_field).toBeNull();
    });
  });
});

// Test helpers

function createValidTask(): Task {
  return {
    task_uuid: 'task-123',
    named_task_uuid: 'named-task-001',
    name: 'TestTask',
    namespace: 'test',
    version: '1.0.0',
    context: null,
    correlation_id: 'corr-001',
    parent_correlation_id: null,
    complete: false,
    priority: 0,
    initiator: null,
    source_system: null,
    reason: null,
    tags: null,
    identity_hash: 'hash-123',
    created_at: new Date().toISOString(),
    updated_at: new Date().toISOString(),
    requested_at: new Date().toISOString(),
  };
}

function createValidWorkflowStep(): WorkflowStep {
  return {
    workflow_step_uuid: 'step-123',
    task_uuid: 'task-123',
    named_step_uuid: 'named-step-001',
    name: 'TestStep',
    template_step_name: 'test_step',
    retryable: true,
    max_attempts: 3,
    attempts: 0,
    in_process: false,
    processed: false,
    skippable: false,
    inputs: null,
    results: null,
    backoff_request_seconds: null,
    processed_at: null,
    last_attempted_at: null,
    created_at: new Date().toISOString(),
    updated_at: new Date().toISOString(),
  };
}

function createValidStepDefinition(): StepDefinition {
  return {
    name: 'test_step',
    description: 'A test step',
    handler: {
      callable: 'TestHandler',
      initialization: {},
    },
    system_dependency: null,
    dependencies: [],
    timeout_seconds: 30,
    retry: {
      retryable: true,
      max_attempts: 3,
      backoff: 'exponential',
      backoff_base_ms: 1000,
      max_backoff_ms: 30000,
    },
  };
}

function createValidFfiStepEvent(): FfiStepEvent {
  return {
    event_id: 'event-123',
    task_uuid: 'task-456',
    step_uuid: 'step-789',
    correlation_id: 'corr-001',
    trace_id: null,
    span_id: null,
    task_correlation_id: 'task-corr-001',
    parent_correlation_id: null,
    task: createValidTask(),
    workflow_step: createValidWorkflowStep(),
    step_definition: createValidStepDefinition(),
    dependency_results: {},
  };
}
