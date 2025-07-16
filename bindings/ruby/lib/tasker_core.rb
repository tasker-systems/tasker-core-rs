# frozen_string_literal: true

require_relative 'tasker_core/version'
require 'json'
require 'faraday'
require 'dry-events'
require 'dry-struct'
require 'dry-types'
require 'dry-validation'

begin
  # Load the compiled Rust extension first (provides base classes)
  require_relative 'tasker_core/tasker_core_rb'
rescue LoadError => e
  raise LoadError, <<~MSG

    ❌ Failed to load tasker-core-rb native extension!

    This usually means the Rust extension hasn't been compiled yet.

    To compile the extension:
      cd #{File.dirname(__FILE__)}/../..
      rake compile

    Or if you're using this gem in a Rails application:
      bundle exec rake tasker_core:compile

    Original error: #{e.message}

  MSG
end

# Load Ruby modules after Rust extension (they depend on Rust base classes)
# require_relative 'tasker_core/types'  # Temporarily disabled due to dry-types version issues
require_relative 'tasker_core/events'
require_relative 'tasker_core/step_handler/base'  # Now enabled - BaseStepHandler available from Rust
require_relative 'tasker_core/step_handler/api'   # Now enabled - depends on base
require_relative 'tasker_core/task_handler'       # Task handler base class

module TaskerCore
  # Base error hierarchy
  class Error < StandardError; end
  class OrchestrationError < Error; end
  class DatabaseError < Error; end
  class StateTransitionError < Error; end
  class ValidationError < Error; end
  class TimeoutError < Error; end
  class FFIError < Error; end

  # Step handler error classification (mirrors Rails engine design)
  class ProceduralError < Error; end

  # Retryable errors - temporary failures that should be retried with backoff
  class RetryableError < ProceduralError
    attr_reader :retry_after, :context, :error_category # Suggested retry delay in seconds (from server)        # Additional context for monitoring/debugging # Category for grouping (network, rate_limit, etc.)

    def initialize(message, retry_after: nil, context: {}, error_category: nil)
      super(message)
      @retry_after = retry_after
      @context = context || {}
      @error_category = error_category
    end

    # Check if this error suggests skipping standard exponential backoff
    def skip_backoff?
      retry_after&.positive?
    end

    # Get effective retry delay (server-suggested or calculated)
    def effective_retry_delay(attempt_number = 1)
      return retry_after if retry_after&.positive?

      # Exponential backoff: 2^attempt with max of 300 seconds (5 minutes)
      [2**attempt_number, 300].min
    end
  end

  # Permanent errors - failures that should NOT be retried
  class PermanentError < ProceduralError
    attr_reader :error_code, :context, :error_category # Machine-readable error code for categorization        # Additional context for monitoring/debugging # Category for grouping (validation, auth, etc.)

    def initialize(message, error_code: nil, context: {}, error_category: nil)
      super(message)
      @error_code = error_code
      @context = context || {}
      @error_category = error_category
    end

    # Common permanent error categories
    def validation_error?
      error_category == 'validation' || error_code&.start_with?('VALIDATION_')
    end

    def authorization_error?
      error_category == 'authorization' || error_code&.start_with?('AUTH_')
    end

    def business_logic_error?
      error_category == 'business_logic' || error_code&.start_with?('BUSINESS_')
    end
  end

  # Error classification utilities (mirrors Rails engine ResponseProcessor)
  module ErrorClassification
    # HTTP status codes that indicate temporary failures (should retry with backoff)
    BACKOFF_ERROR_CODES = [429, 503].freeze

    # HTTP status codes that indicate permanent failures (should NOT retry)
    PERMANENT_ERROR_CODES = [400, 401, 403, 404, 422].freeze

    # Success status codes
    SUCCESS_CODES = (200..226)

    # Classify HTTP response and create appropriate error
    def self.from_http_response(status_code, message, response_body = nil, headers = {})
      context = {
        status_code: status_code,
        response_body: response_body,
        headers: headers
      }

      case status_code
      when *SUCCESS_CODES
        nil # No error for success codes
      when *BACKOFF_ERROR_CODES
        # Rate limiting or service unavailable - retry with backoff
        retry_after = extract_retry_after(headers)
        category = status_code == 429 ? 'rate_limit' : 'service_unavailable'
        RetryableError.new(
          message,
          retry_after: retry_after,
          context: context,
          error_category: category
        )
      when *PERMANENT_ERROR_CODES
        # Client errors - don't retry
        category = classify_permanent_error(status_code)
        error_code = extract_error_code(status_code, response_body)
        PermanentError.new(
          message,
          error_code: error_code,
          context: context,
          error_category: category
        )
      when 500..599
        # Other server errors - retry without forced backoff
        RetryableError.new(
          message,
          context: context,
          error_category: 'server_error'
        )
      else
        # Unknown status codes - treat as retryable
        RetryableError.new(
          message,
          context: context,
          error_category: 'unknown'
        )
      end
    end

    # Extract retry-after header value (in seconds)
    def self.extract_retry_after(headers)
      retry_after = headers['Retry-After'] || headers['retry-after']
      return nil unless retry_after

      # Handle both numeric (seconds) and HTTP date formats
      if retry_after.match?(/^\d+$/)
        retry_after.to_i
      else
        # Parse HTTP date and calculate seconds from now
        begin
          future_time = Time.parse(retry_after)
          [future_time - Time.now, 0].max.to_i
        rescue StandardError
          nil
        end
      end
    end

    # Classify permanent error category by status code
    def self.classify_permanent_error(status_code)
      case status_code
      when 400 then 'validation'
      when 401, 403 then 'authorization'
      when 404 then 'not_found'
      when 422 then 'validation'
      else 'client_error'
      end
    end

    # Extract machine-readable error code from response
    def self.extract_error_code(status_code, response_body)
      # Try to parse JSON response for error codes
      if response_body&.strip&.start_with?('{')
        begin
          parsed = JSON.parse(response_body)
          return parsed['error_code'] || parsed['code'] || parsed['error']
        rescue JSON::ParserError
          # Fall through to status-based codes
        end
      end

      # Fallback to status-based error codes
      "HTTP_#{status_code}"
    end
  end

  # High-performance operations delegated to Rust
  class RustOperations
    def initialize(database_url)
      @database_url = database_url
    end

    # Get real-time task execution context using Rust SQL functions
    # @param task_id [Integer] The task ID
    # @return [TaskerCore::TaskExecutionContext] Real-time execution context
    def get_task_execution_context(task_id)
      # Delegates to the high-performance Rust implementation
      # that uses optimized SQL functions for real-time analysis
      TaskerCore.get_task_execution_context(task_id, @database_url)
    rescue TaskerCore::DatabaseError => e
      raise TaskerCore::OrchestrationError, "Failed to get task execution context: #{e.message}"
    end

    # Discover viable steps using high-performance dependency resolution
    # @param task_id [Integer] The task ID
    # @return [Array<TaskerCore::ViableStep>] Array of viable steps ready for execution
    def discover_viable_steps(task_id)
      # Uses Rust's optimized dependency resolution algorithms
      # that are 10-100x faster than Ruby/SQL approaches
      TaskerCore.discover_viable_steps(task_id, @database_url)
    rescue TaskerCore::DatabaseError => e
      raise TaskerCore::OrchestrationError, "Failed to discover viable steps: #{e.message}"
    end

    # Get system health metrics
    # @return [TaskerCore::SystemHealth] Current system health statistics
    def get_system_health
      # Real-time health metrics computed efficiently in Rust
      TaskerCore.get_system_health(@database_url)
    rescue TaskerCore::DatabaseError => e
      raise TaskerCore::OrchestrationError, "Failed to get system health: #{e.message}"
    end

    # Get analytics for performance monitoring
    # @param time_range_hours [Integer] Time range for analytics
    # @return [TaskerCore::AnalyticsMetrics] Performance analytics with structured data
    def get_analytics(time_range_hours = 24)
      # High-performance analytics computed in Rust with structured results
      TaskerCore.get_analytics_metrics(@database_url, time_range_hours)
    rescue TaskerCore::DatabaseError => e
      raise TaskerCore::OrchestrationError, "Failed to get analytics: #{e.message}"
    end

    # Analyze task dependencies for optimization insights
    # @param task_id [Integer] The task ID
    # @return [TaskerCore::DependencyAnalysis] Comprehensive dependency analysis
    def analyze_dependencies(task_id)
      # High-performance DAG analysis with optimization recommendations
      TaskerCore.analyze_dependencies(task_id, @database_url)
    rescue TaskerCore::DatabaseError => e
      raise TaskerCore::OrchestrationError, "Failed to analyze dependencies: #{e.message}"
    end

    # Batch update step states efficiently
    # @param updates [Array<Array>] Array of [step_id, new_state, context_data] tuples
    # @return [Integer] Number of steps successfully updated
    def batch_update_step_states(updates)
      # Efficient batch database operations performed in Rust
      TaskerCore.batch_update_step_states(updates, @database_url)
    rescue TaskerCore::DatabaseError => e
      raise TaskerCore::OrchestrationError, "Failed to batch update step states: #{e.message}"
    end
  end

  # Rails integration helper that provides access to Rust performance operations
  # Rails integration helper providing both handler foundation and performance operations
  class RailsIntegration
    attr_reader :rust_ops, :event_bridge

    def initialize(database_url = nil, logger: nil)
      @database_url = database_url || default_database_url
      @logger = logger || default_logger
      @rust_ops = RustOperations.new(@database_url)

      # Initialize event system
      initialize_event_system
    end

    # ========================================================================
    # HANDLER FOUNDATION
    # ========================================================================

    # Get Rust-based step handler foundation for inheritance
    # This returns the actual Rust BaseStepHandler that provides orchestration
    def step_handler_foundation
      # Return actual Rust BaseStepHandler from FFI layer
      TaskerCore::BaseStepHandler
    end

    # Get enhanced API step handler foundation
    # This extends the Rust foundation with HTTP-specific functionality
    def api_step_handler_foundation
      TaskerCore::StepHandler::Api
    end

    # Check if Rust handler foundation is available
    def handler_migration_available?
      # Check for actual Rust handlers from FFI layer
      defined?(TaskerCore::BaseStepHandler) && defined?(TaskerCore::BaseTaskHandler)
    end

    # Create step handler instance with Rust integration
    # @param handler_class [Class] Step handler class to instantiate
    # @param config [Hash] Handler configuration
    # @return [TaskerCore::StepHandler::Base] Handler instance
    def create_step_handler(handler_class, config: {})
      handler_class.new(
        config: config,
        logger: @logger,
        rust_integration: self
      )
    end

    # ========================================================================
    # EVENT SYSTEM
    # ========================================================================

    # Initialize event system with Rust bridge
    def initialize_event_system
      # Initialize built-in subscribers for monitoring
      TaskerCore::Events.initialize_subscribers(logger: @logger)

      # Create bridge to Rust event system
      @event_bridge = TaskerCore::Events.create_rust_bridge(self)

      @logger.info('TaskerCore event system initialized')
    end

    # Publish orchestration event
    def publish_orchestration_event(event_type, data = {})
      TaskerCore::Events.publish_orchestration(event_type, data)
    end

    # Publish step handler event
    def publish_step_handler_event(event_type, data = {})
      TaskerCore::Events.publish_step_handler(event_type, data)
    end

    # Subscribe to orchestration events
    def subscribe_to_orchestration(event_type, &)
      TaskerCore::Events.subscribe_orchestration(event_type, &)
    end

    # Subscribe to step handler events
    def subscribe_to_step_handler(event_type, &)
      TaskerCore::Events.subscribe_step_handler(event_type, &)
    end

    # Subscribe to Rust events (pattern-based)
    def subscribe_to_rust_events(event_pattern, &)
      @event_bridge.subscribe_to_rust_events(event_pattern, &)
    end

    # ========================================================================
    # TYPE SYSTEM INTEGRATION
    # ========================================================================

    # Validate and convert data using type system
    # @param data [Hash] Raw data to validate
    # @param type_class [Class] TaskerCore::Types class to use
    # @return [TaskerCore::Types::BaseStruct] Validated and typed data
    def validate_and_convert(data, type_class)
      type_class.new(data)
    rescue Dry::Struct::Error => e
      raise TaskerCore::ValidationError, "Type validation failed: #{e.message}"
    end

    # Create step context from raw data
    def create_step_context(raw_data)
      validate_and_convert(raw_data, TaskerCore::Types::StepContext)
    end

    # Create task context from raw data
    def create_task_context(raw_data)
      validate_and_convert(raw_data, TaskerCore::Types::TaskContext)
    end

    # ========================================================================
    # PERFORMANCE OPERATIONS (enhanced with type safety)
    # ========================================================================

    # Get task execution context with type validation
    def get_task_execution_context(task_id)
      raw_result = @rust_ops.get_task_execution_context(task_id)
      validate_and_convert(raw_result.to_h, TaskerCore::Types::TaskExecutionContext)
    end

    # Discover viable steps with type validation
    def discover_viable_steps(task_id)
      raw_results = @rust_ops.discover_viable_steps(task_id)
      raw_results.map { |raw| validate_and_convert(raw.to_h, TaskerCore::Types::ViableStep) }
    end

    # Get system health with type validation
    def get_system_health
      raw_result = @rust_ops.get_system_health
      validate_and_convert(raw_result.to_h, TaskerCore::Types::SystemHealth)
    end

    # Get analytics with type validation
    def get_analytics(time_range_hours = 24)
      raw_result = @rust_ops.get_analytics(time_range_hours)
      validate_and_convert(raw_result.to_h, TaskerCore::Types::AnalyticsMetrics)
    end

    # Analyze dependencies with type validation
    def analyze_dependencies(task_id)
      raw_result = @rust_ops.analyze_dependencies(task_id)
      validate_and_convert(raw_result.to_h, TaskerCore::Types::DependencyAnalysis)
    end

    # Batch update step states with validation
    def batch_update_step_states(updates)
      # Validate input using schema
      validated = TaskerCore::Types::BatchUpdateSchema.call({ updates: updates })

      if validated.failure?
        raise TaskerCore::ValidationError, "Batch update validation failed: #{validated.errors.to_h}"
      end

      # Convert to format expected by Rust layer
      rust_updates = validated.to_h[:updates].map do |update|
        [update[:step_id], update[:new_state], update[:context_data]]
      end

      @rust_ops.batch_update_step_states(rust_updates)
    end

    # ========================================================================
    # SHUTDOWN AND CLEANUP
    # ========================================================================

    # Graceful shutdown
    def shutdown
      @logger.info('Shutting down TaskerCore integration...')

      # Stop event bridge
      @event_bridge&.stop

      # Clean up any other resources

      @logger.info('TaskerCore integration shutdown complete')
    end

    private

    def default_database_url
      if defined?(Rails)
        # Extract from Rails database configuration
        config = Rails.application.config.database_configuration[Rails.env]
        if config
          build_postgres_url(config)
        else
          ENV['DATABASE_URL'] || 'postgresql://localhost/tasker_development'
        end
      else
        ENV['DATABASE_URL'] || 'postgresql://localhost/tasker_development'
      end
    end

    def build_postgres_url(config)
      host = config['host'] || 'localhost'
      port = config['port'] || 5432
      database = config['database']
      username = config['username']
      password = config['password']

      url = 'postgresql://'
      url += "#{username}:#{password}@" if username && password
      url += "#{host}:#{port}/#{database}"
      url
    end

    def default_logger
      if defined?(Rails)
        Rails.logger
      else
        require 'logger'
        Logger.new($stdout).tap do |log|
          log.level = Logger::INFO
          log.formatter = proc { |severity, datetime, _progname, msg|
            "[#{datetime}] #{severity} TaskerCore: #{msg}\n"
          }
        end
      end
    end
  end
end
