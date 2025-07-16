# frozen_string_literal: true

module TaskerCore
  # Constants used throughout the TaskerCore gem
  #
  # This module contains constants for workflow step and task statuses,
  # validation states, and event definitions. These constants mirror the
  # Tasker Rails engine to maintain compatibility.
  module Constants
    # Status values for workflow steps
    module WorkflowStepStatuses
      # Step is waiting to be processed
      PENDING = 'pending'
      # Step is currently being processed
      IN_PROGRESS = 'in_progress'
      # Step encountered an error during processing
      ERROR = 'error'
      # Step completed successfully
      COMPLETE = 'complete'
      # Step was manually marked as resolved
      RESOLVED_MANUALLY = 'resolved_manually'
      # Step was cancelled
      CANCELLED = 'cancelled'
    end

    # Status values for tasks
    module TaskStatuses
      # Task is waiting to be processed
      PENDING = 'pending'
      # Task is currently being processed
      IN_PROGRESS = 'in_progress'
      # Task encountered an error during processing
      ERROR = 'error'
      # Task completed successfully
      COMPLETE = 'complete'
      # Task was manually marked as resolved
      RESOLVED_MANUALLY = 'resolved_manually'
      # Task was cancelled
      CANCELLED = 'cancelled'
    end

    # Task execution context status values from TaskExecutionContext view
    module TaskExecution
      # Execution status values - indicate current workflow execution state
      module ExecutionStatus
        # Task has steps ready for immediate execution
        HAS_READY_STEPS = 'has_ready_steps'
        # Task has steps currently being processed
        PROCESSING = 'processing'
        # Task is blocked by failed steps with no ready steps
        BLOCKED_BY_FAILURES = 'blocked_by_failures'
        # All task steps have completed successfully
        ALL_COMPLETE = 'all_complete'
        # Task is waiting for step dependencies to be satisfied
        WAITING_FOR_DEPENDENCIES = 'waiting_for_dependencies'
      end

      # Recommended action values - indicate what should happen next
      module RecommendedAction
        # Execute the steps that are ready for processing
        EXECUTE_READY_STEPS = 'execute_ready_steps'
        # Wait for currently processing steps to complete
        WAIT_FOR_COMPLETION = 'wait_for_completion'
        # Handle failed steps that are blocking progress
        HANDLE_FAILURES = 'handle_failures'
        # Finalize the task as all steps are complete
        FINALIZE_TASK = 'finalize_task'
        # Wait for dependencies to be satisfied
        WAIT_FOR_DEPENDENCIES = 'wait_for_dependencies'
      end

      # Health status values - indicate overall workflow health
      module HealthStatus
        # No failed steps, workflow is healthy
        HEALTHY = 'healthy'
        # Has failed steps but also has ready steps (can make progress)
        RECOVERING = 'recovering'
        # Has failed steps and no ready steps (intervention needed)
        BLOCKED = 'blocked'
        # Health status cannot be determined
        UNKNOWN = 'unknown'
      end
    end

    # All valid status values for workflow steps
    VALID_WORKFLOW_STEP_STATUSES = [
      WorkflowStepStatuses::PENDING,
      WorkflowStepStatuses::IN_PROGRESS,
      WorkflowStepStatuses::ERROR,
      WorkflowStepStatuses::COMPLETE,
      WorkflowStepStatuses::CANCELLED,
      WorkflowStepStatuses::RESOLVED_MANUALLY
    ].freeze

    # All valid status values for tasks
    VALID_TASK_STATUSES = [
      TaskStatuses::PENDING,
      TaskStatuses::IN_PROGRESS,
      TaskStatuses::ERROR,
      TaskStatuses::COMPLETE,
      TaskStatuses::CANCELLED,
      TaskStatuses::RESOLVED_MANUALLY
    ].freeze

    # Task lifecycle event constants
    module TaskEvents
      INITIALIZE_REQUESTED = 'task.initialize_requested'
      START_REQUESTED = 'task.start_requested'
      COMPLETED = 'task.completed'
      FAILED = 'task.failed'
      RETRY_REQUESTED = 'task.retry_requested'
      CANCELLED = 'task.cancelled'
      RESOLVED_MANUALLY = 'task.resolved_manually'
      BEFORE_TRANSITION = 'task.before_transition'
    end

    # Step lifecycle event constants
    module StepEvents
      INITIALIZE_REQUESTED = 'step.initialize_requested'
      EXECUTION_REQUESTED = 'step.execution_requested'
      BEFORE_HANDLE = 'step.before_handle'
      HANDLE = 'step.handle'
      COMPLETED = 'step.completed'
      FAILED = 'step.failed'
      RETRY_REQUESTED = 'step.retry_requested'
      CANCELLED = 'step.cancelled'
      RESOLVED_MANUALLY = 'step.resolved_manually'
      BEFORE_TRANSITION = 'step.before_transition'
    end

    # Workflow orchestration event constants
    module WorkflowEvents
      TASK_STARTED = 'workflow.task_started'
      TASK_COMPLETED = 'workflow.task_completed'
      TASK_FAILED = 'workflow.task_failed'
      STEP_COMPLETED = 'workflow.step_completed'
      STEP_FAILED = 'workflow.step_failed'
      VIABLE_STEPS_DISCOVERED = 'workflow.viable_steps_discovered'
      NO_VIABLE_STEPS = 'workflow.no_viable_steps'
      ORCHESTRATION_REQUESTED = 'workflow.orchestration_requested'
      TASK_FINALIZATION_STARTED = 'workflow.task_finalization_started'
      TASK_FINALIZATION_COMPLETED = 'workflow.task_finalization_completed'
      TASK_REENQUEUE_STARTED = 'workflow.task_reenqueue_started'
      TASK_REENQUEUE_REQUESTED = 'workflow.task_reenqueue_requested'
      TASK_REENQUEUE_FAILED = 'workflow.task_reenqueue_failed'
      TASK_REENQUEUE_DELAYED = 'workflow.task_reenqueue_delayed'
      TASK_STATE_UNCLEAR = 'workflow.task_state_unclear'
      STEP_EXECUTION_FAILED = 'workflow.step_execution_failed'
      VIABLE_STEPS_BATCH_READY = 'workflow.viable_steps_batch_ready'
      STEPS_EXECUTION_STARTED = 'workflow.steps_execution_started'
      STEPS_EXECUTION_COMPLETED = 'workflow.steps_execution_completed'
      TRANSITION_REQUESTED = 'workflow.transition_requested'
      INVALID_TRANSITION = 'workflow.invalid_transition'
      ORCHESTRATION_ERROR = 'workflow.orchestration_error'
    end

    # Observability and telemetry event constants
    module ObservabilityEvents
      # Task-level observability events
      module Task
        HANDLE = 'observability.task.handle'
        ENQUEUE = 'observability.task.enqueue'
        FINALIZE = 'observability.task.finalize'
      end

      # Step-level observability events
      module Step
        HANDLE = 'observability.step.handle'
        FIND_VIABLE = 'observability.step.find_viable'
        BACKOFF = 'observability.step.backoff'
        SKIP = 'observability.step.skip'
        MAX_RETRIES_REACHED = 'observability.step.max_retries_reached'
      end
    end

    # Test event constants
    module TestEvents
      BASIC_EVENT = 'test.event'
      SLOW_EVENT = 'slow.event'
      TEST_EVENT = 'test.event'
      CUSTOM_EVENT = 'custom.event'
      # Alternative casing event for testing
      TEST_DOT_EVENT = 'Test.Event'
    end

    # Task finalization reason constants
    module TaskFinalization
      # Error messages for task failure scenarios
      module ErrorMessages
        STEPS_IN_ERROR_STATE = 'steps_in_error_state'
      end

      # Reasons for re-enqueueing tasks (asynchronous processing)
      module ReenqueueReasons
        # Unable to determine context
        CONTEXT_UNAVAILABLE = 'context_unavailable'
        # Steps are currently in progress
        STEPS_IN_PROGRESS = 'steps_in_progress'
        # Waiting for dependency completion
        AWAITING_DEPENDENCIES = 'awaiting_dependencies'
        # Ready steps are available for processing
        READY_STEPS_AVAILABLE = 'ready_steps_available'
        # General workflow continuation
        CONTINUING_WORKFLOW = 'continuing_workflow'
        # Default reason for pending steps (from TaskReenqueuer)
        PENDING_STEPS_REMAINING = 'pending_steps_remaining'
        # Default reason for retry backoff (from TaskReenqueuer)
        RETRY_BACKOFF = 'retry_backoff'
      end

      # Reasons for setting tasks to pending (synchronous processing)
      module PendingReasons
        # Unable to determine context
        CONTEXT_UNAVAILABLE = 'context_unavailable'
        # Waiting for current steps to complete
        WAITING_FOR_STEP_COMPLETION = 'waiting_for_step_completion'
        # Waiting for dependencies to be satisfied
        WAITING_FOR_DEPENDENCIES = 'waiting_for_dependencies'
        # Ready for immediate processing
        READY_FOR_PROCESSING = 'ready_for_processing'
        # Workflow temporarily paused
        WORKFLOW_PAUSED = 'workflow_paused'
      end
    end

    # All valid re-enqueue reason values for task finalization
    VALID_TASK_REENQUEUE_REASONS = [
      TaskFinalization::ReenqueueReasons::CONTEXT_UNAVAILABLE,
      TaskFinalization::ReenqueueReasons::STEPS_IN_PROGRESS,
      TaskFinalization::ReenqueueReasons::AWAITING_DEPENDENCIES,
      TaskFinalization::ReenqueueReasons::READY_STEPS_AVAILABLE,
      TaskFinalization::ReenqueueReasons::CONTINUING_WORKFLOW,
      TaskFinalization::ReenqueueReasons::PENDING_STEPS_REMAINING,
      TaskFinalization::ReenqueueReasons::RETRY_BACKOFF
    ].freeze

    # All valid pending reason values for task finalization
    VALID_TASK_PENDING_REASONS = [
      TaskFinalization::PendingReasons::CONTEXT_UNAVAILABLE,
      TaskFinalization::PendingReasons::WAITING_FOR_STEP_COMPLETION,
      TaskFinalization::PendingReasons::WAITING_FOR_DEPENDENCIES,
      TaskFinalization::PendingReasons::READY_FOR_PROCESSING,
      TaskFinalization::PendingReasons::WORKFLOW_PAUSED
    ].freeze

    # Task Transition Event Map
    TASK_TRANSITION_EVENT_MAP = {
      # Initial state transitions (from nil/initial)
      [nil, TaskStatuses::PENDING] => TaskEvents::INITIALIZE_REQUESTED,
      [nil, TaskStatuses::IN_PROGRESS] => TaskEvents::START_REQUESTED,
      [nil, TaskStatuses::COMPLETE] => TaskEvents::COMPLETED,
      [nil, TaskStatuses::ERROR] => TaskEvents::FAILED,
      [nil, TaskStatuses::CANCELLED] => TaskEvents::CANCELLED,
      [nil, TaskStatuses::RESOLVED_MANUALLY] => TaskEvents::RESOLVED_MANUALLY,

      # Transitions from pending
      [TaskStatuses::PENDING, TaskStatuses::IN_PROGRESS] => TaskEvents::START_REQUESTED,
      [TaskStatuses::PENDING, TaskStatuses::CANCELLED] => TaskEvents::CANCELLED,
      [TaskStatuses::PENDING, TaskStatuses::ERROR] => TaskEvents::FAILED,

      # Transitions from in progress
      [TaskStatuses::IN_PROGRESS, TaskStatuses::PENDING] => TaskEvents::INITIALIZE_REQUESTED,
      [TaskStatuses::IN_PROGRESS, TaskStatuses::COMPLETE] => TaskEvents::COMPLETED,
      [TaskStatuses::IN_PROGRESS, TaskStatuses::ERROR] => TaskEvents::FAILED,
      [TaskStatuses::IN_PROGRESS, TaskStatuses::CANCELLED] => TaskEvents::CANCELLED,

      # Transitions from error state
      [TaskStatuses::ERROR, TaskStatuses::PENDING] => TaskEvents::RETRY_REQUESTED,
      [TaskStatuses::ERROR, TaskStatuses::RESOLVED_MANUALLY] => TaskEvents::RESOLVED_MANUALLY,

      # Transitions from complete state
      [TaskStatuses::COMPLETE, TaskStatuses::CANCELLED] => TaskEvents::CANCELLED,

      # Transitions from cancelled state
      [TaskStatuses::CANCELLED, TaskStatuses::CANCELLED] => TaskEvents::CANCELLED
    }.freeze

    # Step Transition Event Map
    STEP_TRANSITION_EVENT_MAP = {
      # Initial state transitions (from nil/initial)
      [nil, WorkflowStepStatuses::PENDING] => StepEvents::INITIALIZE_REQUESTED,
      [nil, WorkflowStepStatuses::IN_PROGRESS] => StepEvents::EXECUTION_REQUESTED,
      [nil, WorkflowStepStatuses::COMPLETE] => StepEvents::COMPLETED,
      [nil, WorkflowStepStatuses::ERROR] => StepEvents::FAILED,
      [nil, WorkflowStepStatuses::CANCELLED] => StepEvents::CANCELLED,
      [nil, WorkflowStepStatuses::RESOLVED_MANUALLY] => StepEvents::RESOLVED_MANUALLY,

      # Transitions from pending
      [WorkflowStepStatuses::PENDING, WorkflowStepStatuses::IN_PROGRESS] => StepEvents::EXECUTION_REQUESTED,
      [WorkflowStepStatuses::PENDING, WorkflowStepStatuses::CANCELLED] => StepEvents::CANCELLED,
      [WorkflowStepStatuses::PENDING, WorkflowStepStatuses::ERROR] => StepEvents::FAILED,

      # Transitions from in progress
      [WorkflowStepStatuses::IN_PROGRESS, WorkflowStepStatuses::PENDING] => StepEvents::INITIALIZE_REQUESTED,
      [WorkflowStepStatuses::IN_PROGRESS, WorkflowStepStatuses::COMPLETE] => StepEvents::COMPLETED,
      [WorkflowStepStatuses::IN_PROGRESS, WorkflowStepStatuses::ERROR] => StepEvents::FAILED,
      [WorkflowStepStatuses::IN_PROGRESS, WorkflowStepStatuses::CANCELLED] => StepEvents::CANCELLED,

      # Transitions from error state
      [WorkflowStepStatuses::ERROR, WorkflowStepStatuses::PENDING] => StepEvents::RETRY_REQUESTED,
      [WorkflowStepStatuses::ERROR, WorkflowStepStatuses::RESOLVED_MANUALLY] => StepEvents::RESOLVED_MANUALLY,

      # Transitions from complete state
      [WorkflowStepStatuses::COMPLETE, WorkflowStepStatuses::CANCELLED] => StepEvents::CANCELLED,

      # Transitions from cancelled state
      [WorkflowStepStatuses::CANCELLED, WorkflowStepStatuses::CANCELLED] => StepEvents::CANCELLED
    }.freeze
  end

  module ConfigSchemas
    API_CONFIG_SCHEMA = {
      url: { type: 'string', format: 'uri' },
      params: { type: 'object', additionalProperties: true,
                default: {} },
      headers: {
        type: 'object',
        additionalProperties: { type: 'string' },
        default: {}
      },
      ssl: {
        type: 'object',
        properties: {
          verify: { type: 'boolean', default: true },
          ca_file: { type: 'string' },
          ca_path: { type: 'string' },
          client_cert: { type: 'string' },
          client_key: { type: 'string' }
        }
      },
      timeout: { type: 'integer', minimum: 1, maximum: 300,
                 default: 30 },
      open_timeout: { type: 'integer', minimum: 1, maximum: 60,
                      default: 10 },
      enable_exponential_backoff: { type: 'boolean',
                                    default: true },
      retry_delay: { type: 'number', minimum: 0.1,
                     default: 1.0 },
      jitter_factor: { type: 'number', minimum: 0.0,
                       maximum: 1.0, default: 0.5 },
      auth: {
        type: 'object',
        properties: {
          type: { type: 'string',
                  enum: %w[bearer basic api_key] },
          token: { type: 'string' },
          username: { type: 'string' },
          password: { type: 'string' },
          api_key_header: { type: 'string',
                            default: 'X-API-Key' }
        }
      }
    }.freeze
  end
end
