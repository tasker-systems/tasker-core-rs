# frozen_string_literal: true

require 'dry-types'
require 'dry-struct'

module TaskerCore
  module Types
    # Dry-types version of Rust TaskerConfig structure
    # This provides type-safe configuration access with validation,
    # replacing hash-dig syntax with proper dry-types structures
    module Config
      include Dry.Types()

      # Define type aliases for better compatibility
      Bool = Strict::Bool
      Str = Strict::String
      Int = Strict::Integer
      Flt = Strict::Float
      Arr = Strict::Array
      Hsh = Strict::Hash
      Sym = Strict::Symbol

      # Base configuration struct with common attributes
      class BaseConfig < Dry::Struct
        # Enable transform_keys to handle symbolization
        transform_keys(&:to_sym)
      end

      # Authentication and authorization configuration
      class AuthConfig < BaseConfig
        attribute :authentication_enabled, Bool
        attribute :strategy, Str
        attribute :current_user_method, Str
        attribute :authenticate_user_method, Str
        attribute :authorization_enabled, Bool
        attribute :authorization_coordinator_class, Str
      end

      # Database variables configuration
      class DatabaseVariables < BaseConfig
        attribute :statement_timeout, Int
      end

      # Database connection and pooling configuration
      class DatabaseConfig < BaseConfig
        attribute :enable_secondary_database, Bool
        attribute :url, Str.optional
        attribute :adapter, Str
        attribute :encoding, Str
        attribute :host, Str
        attribute :username, Str
        attribute :password, Str
        attribute :pool, Int # Simple pool size for ActiveRecord compatibility
        attribute :variables, DatabaseVariables
        attribute :checkout_timeout, Int
        attribute :reaping_frequency, Int
        attribute :database, Str.optional # Environment-specific database name override
        attribute :skip_migration_check, Bool.default(false)

        # Get database name for the current environment
        def database_name(environment)
          # Use explicit database name if provided (from environment overrides)
          return database if database

          # Otherwise use environment-based naming convention
          case environment
          when 'development'
            'tasker_rust_development'
          when 'test'
            'tasker_rust_test'
          when 'production'
            ENV['POSTGRES_DB'] || 'tasker_production'
          else
            "tasker_rust_#{environment}"
          end
        end

        # Build complete database URL from configuration
        def database_url(environment)
          # If URL is explicitly provided (with ${DATABASE_URL} expansion), use it
          if url && (url == '${DATABASE_URL}' || url.start_with?('${DATABASE_URL}'))
            # Try to expand ${DATABASE_URL} environment variable
            env_url = ENV.fetch('DATABASE_URL', nil)
            return env_url if env_url && !env_url.empty?
            # If DATABASE_URL is not set, fall through to build from components
          elsif url && !url.empty?
            # Use the URL as-is (not a variable reference)
            return url
          end

          # Build URL from components
          port = ENV['DATABASE_PORT'] || '5432'
          "postgresql://#{username}:#{password}@#{host}:#{port}/#{database_name(environment)}"
        end
      end

      # Telemetry and monitoring configuration
      class TelemetryConfig < BaseConfig
        attribute :enabled, Bool
        attribute :service_name, Str
        attribute :sample_rate, Flt
      end

      # Task processing engine configuration
      class EngineConfig < BaseConfig
        attribute :task_handler_directory, Str
        attribute :task_config_directory, Str
        attribute :identity_strategy, Str
        attribute :custom_events_directories, Arr.of(Str)
      end

      # TaskTemplate discovery configuration
      class TaskTemplatesConfig < BaseConfig
        attribute :search_paths, Arr.of(Str)
      end

      # Alert thresholds configuration
      class AlertThresholds < BaseConfig
        attribute :error_rate, Flt
        attribute :queue_depth, Flt
      end

      # Health monitoring configuration
      class HealthConfig < BaseConfig
        attribute :enabled, Bool
        attribute :check_interval_seconds, Int
        attribute :alert_thresholds, AlertThresholds
      end

      # Dependency graph processing configuration
      class DependencyGraphConfig < BaseConfig
        attribute :max_depth, Int
        attribute :cycle_detection_enabled, Bool
        attribute :optimization_enabled, Bool
      end

      # System-wide configuration
      class SystemConfig < BaseConfig
        attribute :default_dependent_system, Str
        attribute :default_queue_name, Str
        attribute :version, Str
        attribute :max_recursion_depth, Int
      end

      # Reenqueue delays configuration
      class ReenqueueDelays < BaseConfig
        attribute :has_ready_steps, Int
        attribute :waiting_for_dependencies, Int
        attribute :processing, Int
      end

      # Backoff and retry configuration
      class BackoffConfig < BaseConfig
        attribute :default_backoff_seconds, Arr.of(Int)
        attribute :max_backoff_seconds, Int
        attribute :backoff_multiplier, Flt
        attribute :jitter_enabled, Bool
        attribute :jitter_max_percentage, Flt
        attribute :reenqueue_delays, ReenqueueDelays
        attribute :default_reenqueue_delay, Int
        attribute :buffer_seconds, Int
      end

      # Task execution configuration
      class ExecutionConfig < BaseConfig
        attribute :processing_mode, Str
        attribute :max_concurrent_tasks, Int
        attribute :max_concurrent_steps, Int
        attribute :default_timeout_seconds, Int
        attribute :step_execution_timeout_seconds, Int
        attribute :environment, Str
        attribute :max_discovery_attempts, Int
        attribute :step_batch_size, Int
        attribute :max_retries, Int
        attribute :max_workflow_steps, Int
        attribute :connection_timeout_seconds, Int

        # Get step execution timeout in seconds
        def step_execution_timeout
          step_execution_timeout_seconds
        end

        # Get default task timeout in seconds
        def default_timeout
          default_timeout_seconds
        end
      end

      # Task reenqueue configuration
      class ReenqueueConfig < BaseConfig
        attribute :has_ready_steps, Int
        attribute :waiting_for_dependencies, Int
        attribute :processing, Int
      end

      # Event processing configuration
      class EventsConfig < BaseConfig
        attribute :batch_size, Int
        attribute :enabled, Bool
        attribute :batch_timeout_ms, Int
      end

      # Caching configuration
      class CacheConfig < BaseConfig
        attribute :enabled, Bool
        attribute :ttl_seconds, Int
        attribute :max_size, Int
      end

      # Cache entry configuration for query cache sub-components
      class CacheEntryConfig < BaseConfig
        attribute :ttl_seconds, Int
        attribute :max_entries, Int
      end

      # Query cache configuration (matching actual Rust structure)
      class QueryCacheConfig < BaseConfig
        attribute :enabled, Bool
        attribute :active_workers, CacheEntryConfig
        attribute :worker_health, CacheEntryConfig
        attribute :task_metadata, CacheEntryConfig
        attribute :handler_metadata, CacheEntryConfig
        attribute :cleanup_interval_seconds, Int
        attribute :memory_pressure_threshold, Flt
      end

      # PGMQ (PostgreSQL Message Queue) configuration
      class PgmqConfig < BaseConfig
        attribute :poll_interval_ms, Int
        attribute :visibility_timeout_seconds, Int
        attribute :batch_size, Int
        attribute :max_retries, Int
        attribute :default_namespaces, Arr.of(Str)
        attribute :queue_naming_pattern, Str
        attribute :max_batch_size, Int
        attribute :shutdown_timeout_seconds, Int

        # Get poll interval in milliseconds
        def poll_interval
          poll_interval_ms
        end

        # Get visibility timeout in seconds
        def visibility_timeout
          visibility_timeout_seconds
        end

        # Generate queue name for a namespace
        def queue_name_for_namespace(namespace)
          queue_naming_pattern.gsub('{namespace}', namespace)
        end
      end

      # Queue settings configuration
      class QueueSettings < BaseConfig
        attribute :visibility_timeout_seconds, Int
        attribute :message_retention_seconds, Int
        attribute :dead_letter_queue_enabled, Bool
        attribute :max_receive_count, Int
      end

      # Queue configuration for orchestration
      class QueueConfig < BaseConfig
        attribute :task_requests, Str
        attribute :task_processing, Str
        attribute :batch_results, Str
        attribute :step_results, Str
        attribute :worker_queues, Hsh.map(Sym, Str).default({}.freeze)
        attribute :settings, QueueSettings
      end

      # Embedded orchestrator configuration
      class EmbeddedOrchestratorConfig < BaseConfig
        attribute :auto_start, Bool
        attribute :namespaces, Arr.of(Str)
        attribute :shutdown_timeout_seconds, Int

        # Get shutdown timeout in seconds
        def shutdown_timeout
          shutdown_timeout_seconds
        end
      end

      # Orchestration system configuration
      class OrchestrationConfig < BaseConfig
        attribute :mode, Str
        attribute :task_requests_queue_name, Str
        attribute :tasks_per_cycle, Int
        attribute :cycle_interval_ms, Int
        attribute :task_request_polling_interval_ms, Int
        attribute :task_request_visibility_timeout_seconds, Int
        attribute :task_request_batch_size, Int
        attribute :active_namespaces, Arr.of(Str)
        attribute :max_concurrent_orchestrators, Int
        attribute :enable_performance_logging, Bool
        attribute :default_claim_timeout_seconds, Int
        attribute :queues, QueueConfig
        attribute :embedded_orchestrator, EmbeddedOrchestratorConfig
        attribute :enable_heartbeat, Bool
        attribute :heartbeat_interval_ms, Int

        # Get cycle interval in milliseconds
        def cycle_interval
          cycle_interval_ms
        end

        # Get task request polling interval in milliseconds
        def task_request_polling_interval
          task_request_polling_interval_ms
        end

        # Get heartbeat interval in milliseconds
        def heartbeat_interval
          heartbeat_interval_ms
        end

        # Get task request visibility timeout in seconds
        def task_request_visibility_timeout
          task_request_visibility_timeout_seconds
        end

        # Get default claim timeout in seconds
        def default_claim_timeout
          default_claim_timeout_seconds
        end
      end

      # Circuit breaker component configuration
      class CircuitBreakerComponentConfig < BaseConfig
        attribute :failure_threshold, Int
        attribute :timeout_seconds, Int
        attribute :success_threshold, Int
      end

      # Circuit breaker global settings
      class CircuitBreakerGlobalSettings < BaseConfig
        attribute :max_circuit_breakers, Int
        attribute :metrics_collection_interval_seconds, Int
        attribute :auto_create_enabled, Bool
        attribute :min_state_transition_interval_seconds, Flt
      end

      # Circuit breaker configuration
      class CircuitBreakerConfig < BaseConfig
        attribute :enabled, Bool
        attribute :global_settings, CircuitBreakerGlobalSettings
        attribute :default_config, CircuitBreakerComponentConfig
        attribute :component_configs, Hsh.map(Sym, CircuitBreakerComponentConfig)

        # Get configuration for a specific component
        def config_for_component(component_name)
          component_configs[component_name] || default_config
        end
      end

      # Executor instance configuration
      class ExecutorInstanceConfig < BaseConfig
        attribute :min_executors, Int
        attribute :max_executors, Int
        attribute :polling_interval_ms, Int
        attribute :batch_size, Int
        attribute :processing_timeout_ms, Int
        attribute :max_retries, Int
        attribute :circuit_breaker_enabled, Bool
        attribute :circuit_breaker_threshold, Int
      end

      # Executor coordinator configuration
      class ExecutorCoordinatorConfig < BaseConfig
        attribute :auto_scaling_enabled, Bool
        attribute :target_utilization, Flt
        attribute :scaling_interval_seconds, Int
        attribute :health_check_interval_seconds, Int
        attribute :scaling_cooldown_seconds, Int
        attribute :max_db_pool_usage, Flt
      end

      # Orchestration executor pools configuration (TAS-34)
      class ExecutorPoolsConfig < BaseConfig
        attribute :coordinator, ExecutorCoordinatorConfig
        attribute :task_request_processor, ExecutorInstanceConfig
        attribute :task_claimer, ExecutorInstanceConfig
        attribute :step_enqueuer, ExecutorInstanceConfig
        attribute :step_result_processor, ExecutorInstanceConfig
        attribute :task_finalizer, ExecutorInstanceConfig
      end

      # Root configuration structure matching Rust TaskerConfig
      class TaskerConfig < BaseConfig
        attribute :auth, AuthConfig
        attribute :database, DatabaseConfig
        attribute :telemetry, TelemetryConfig
        attribute :engine, EngineConfig
        attribute :task_templates, TaskTemplatesConfig
        attribute :health, HealthConfig
        attribute :dependency_graph, DependencyGraphConfig
        attribute :system, SystemConfig
        attribute :backoff, BackoffConfig
        attribute :execution, ExecutionConfig
        attribute :reenqueue, ReenqueueConfig
        attribute :events, EventsConfig
        attribute :cache, CacheConfig
        attribute :query_cache, QueryCacheConfig
        attribute :pgmq, PgmqConfig
        attribute :orchestration, OrchestrationConfig
        attribute :circuit_breakers, CircuitBreakerConfig
        attribute :executor_pools, ExecutorPoolsConfig.optional

        # Get database URL for the current environment
        def database_url
          database.database_url(execution.environment)
        end

        # Check if running in test environment
        def test_environment?
          execution.environment == 'test'
        end

        # Check if running in development environment
        def development_environment?
          execution.environment == 'development'
        end

        # Check if running in production environment
        def production_environment?
          execution.environment == 'production'
        end

        # Get maximum dependency depth
        def max_dependency_depth
          dependency_graph.max_depth
        end

        # Get maximum workflow steps
        def max_workflow_steps
          execution.max_workflow_steps
        end

        # Get system version string
        def system_version
          system.version
        end

        # Get connection timeout in seconds
        def connection_timeout
          execution.connection_timeout_seconds
        end

        # Get maximum retries
        def max_retries
          execution.max_retries
        end

        # Get maximum recursion depth
        def max_recursion_depth
          system.max_recursion_depth
        end

        # Get PGMQ shutdown timeout in seconds
        def pgmq_shutdown_timeout
          pgmq.shutdown_timeout_seconds
        end

        # Get PGMQ maximum batch size
        def pgmq_max_batch_size
          pgmq.max_batch_size
        end

        # Get executor pools configuration with fallback to defaults if not configured
        def executor_pools_config
          executor_pools || default_executor_pools_config
        end

        # Check if executor pools are explicitly configured
        def has_executor_pools_config?
          !executor_pools.nil?
        end

        private

        # Default executor pools configuration
        def default_executor_pools_config
          ExecutorPoolsConfig.new(
            coordinator: ExecutorCoordinatorConfig.new(
              auto_scaling_enabled: true,
              target_utilization: 0.75,
              scaling_interval_seconds: 30,
              health_check_interval_seconds: 10,
              scaling_cooldown_seconds: 60,
              max_db_pool_usage: 0.85
            ),
            task_request_processor: ExecutorInstanceConfig.new(
              min_executors: 1,
              max_executors: 5,
              polling_interval_ms: 100,
              batch_size: 10,
              processing_timeout_ms: 30_000,
              max_retries: 3,
              circuit_breaker_enabled: true,
              circuit_breaker_threshold: 5
            ),
            task_claimer: ExecutorInstanceConfig.new(
              min_executors: 2,
              max_executors: 10,
              polling_interval_ms: 50,
              batch_size: 20,
              processing_timeout_ms: 30_000,
              max_retries: 3,
              circuit_breaker_enabled: true,
              circuit_breaker_threshold: 3
            ),
            step_enqueuer: ExecutorInstanceConfig.new(
              min_executors: 2,
              max_executors: 8,
              polling_interval_ms: 50,
              batch_size: 50,
              processing_timeout_ms: 30_000,
              max_retries: 3,
              circuit_breaker_enabled: true,
              circuit_breaker_threshold: 5
            ),
            step_result_processor: ExecutorInstanceConfig.new(
              min_executors: 2,
              max_executors: 10,
              polling_interval_ms: 100,
              batch_size: 20,
              processing_timeout_ms: 30_000,
              max_retries: 3,
              circuit_breaker_enabled: true,
              circuit_breaker_threshold: 3
            ),
            task_finalizer: ExecutorInstanceConfig.new(
              min_executors: 1,
              max_executors: 4,
              polling_interval_ms: 200,
              batch_size: 10,
              processing_timeout_ms: 30_000,
              max_retries: 3,
              circuit_breaker_enabled: true,
              circuit_breaker_threshold: 5
            )
          )
        end
      end
    end
  end
end
