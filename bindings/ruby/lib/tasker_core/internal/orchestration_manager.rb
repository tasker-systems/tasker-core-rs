# frozen_string_literal: true

require 'singleton'

module TaskerCore
  module Internal
    # Ruby-level wrapper for the unified orchestration system
    # This provides a singleton pattern at the Ruby level to avoid
    # re-initializing the orchestration system on every call
    class OrchestrationManager
    include Singleton

    attr_reader :initialized_at, :status, :logger

    def initialize
      @initialized = false
      @status = 'not_initialized'
      @initialized_at = nil
      @orchestration_system = nil
      @base_task_handler = nil  # 🎯 Memoized BaseTaskHandler
      @orchestration_handle = nil  # 🎯 NEW: Handle-based FFI architecture
      @logger = TaskerCore::Logging::Logger.instance
    end

    # Get the orchestration system (handle-based initialization)
    def orchestration_system
      # Use handle-based approach - no direct system initialization needed
      handle = orchestration_handle
      if handle
        @initialized = true
        @status = 'initialized'
        @initialized_at = defined?(Rails) ? Time.current : Time.now
        @orchestration_system ||= { 'handle_based' => true }
      else
        @status = 'failed'
      end
      @orchestration_system
    end

    # Check if the orchestration system is initialized
    def initialized?
      @initialized && @orchestration_system
    end

    # Get orchestration system status information
    def info
      {
        initialized: @initialized,
        status: @status,
        initialized_at: @initialized_at,
        architecture: @orchestration_system&.dig('architecture'),
        pool_source: @orchestration_system&.dig('pool_source')
      }
    end

    # Reset the orchestration system (for testing)
    def reset!
      @initialized = false
      @status = 'reset'
      @initialized_at = nil
      @orchestration_system = nil
      @base_task_handler = nil  # Reset memoized handler too
      @orchestration_handle = nil  # Reset handle too
    end

    # Delegate common orchestration operations - use handle-based versions

    def register_handler(handler_data)
      register_handler_with_handle(handler_data)
    end

    def find_handler(task_request)
      find_handler_with_handle(task_request)
    end

    def list_handlers(namespace = nil)
      orchestration_system # Ensure initialized
      # TODO: Implement handle-based list_handlers once available
      [] # Return empty for now
    end

    def handler_exists?(handler_key)
      orchestration_system # Ensure initialized
      # Check if we can find the handler instead
      result = find_handler(handler_key)
      result && result['found']
    end

    # Get workflow steps for a task with dependency information
    def get_workflow_steps_for_task(task_id)
      orchestration_system # Ensure initialized
      
      begin
        # Use handle-based approach to get workflow steps
        handle = orchestration_handle
        return nil unless handle
        
        # Call Rust FFI method to get steps with dependencies
        handle.get_task_workflow_steps(task_id)
      rescue StandardError => e
        logger.error "Failed to get workflow steps for task #{task_id}: #{e.message}"
        nil
      end
    end

    # Get step dependencies for a specific step
    def get_step_dependencies(step_id)
      orchestration_system # Ensure initialized
      
      begin
        handle = orchestration_handle
        return nil unless handle
        
        # Call Rust FFI method to get step dependencies
        handle.get_step_dependencies(step_id)
      rescue StandardError => e
        logger.error "Failed to get dependencies for step #{step_id}: #{e.message}"
        nil
      end
    end

    # Get task metadata (namespace, name, version) for a task
    def get_task_metadata(task_id)
      orchestration_system # Ensure initialized
      
      begin
        handle = orchestration_handle
        return nil unless handle
        
        # Call Rust FFI method to get task metadata
        handle.get_task_metadata(task_id)
      rescue StandardError => e
        logger.error "Failed to get metadata for task #{task_id}: #{e.message}"
        nil
      end
    end

    # 🎯 THREAD-SAFE FIX: Create stateless BaseTaskHandler singleton
    # The handler itself is stateless - configuration is passed per-call via handle(task_id, config)
    # This ensures thread safety and correct per-task configuration handling
    def get_base_task_handler(task_config_hash = nil)
      orchestration_system # Ensure initialized

      # Create a stateless singleton handler that receives config per-call
      # The config parameter here is ignored - it's only kept for API compatibility
      @stateless_base_task_handler ||= begin
        logger.info "✅ Creating stateless BaseTaskHandler singleton (config passed per-call)"
        # BaseTaskHandler constructor takes no arguments
        TaskerCore::BaseTaskHandler.new
      rescue StandardError => e
        error_msg = "Failed to create BaseTaskHandler: #{e.message}"
        logger.error error_msg
        nil
      end
    end

    # 🎯 STEP PROCESSING: Get BaseTaskHandler for step processing via orchestration system
    # Ruby step handlers use the same BaseTaskHandler pattern as task handlers for consistency
    # This provides access to the process_step method for individual step execution
    def get_step_processor
      orchestration_system # Ensure initialized

      # Reuse the same BaseTaskHandler singleton for step processing
      # This ensures consistent orchestration system integration
      @step_processor ||= begin
        logger.info "✅ Getting BaseTaskHandler for step processing"
        get_base_task_handler
      rescue StandardError => e
        error_msg = "Failed to get step processor: #{e.message}"
        logger.error error_msg
        nil
      end
    end

    # 🎯 SHARED STEP EXECUTION: Create shared RubyStepHandler bridge
    # The RubyStepHandler should be a shared FFI bridge component, not per-step instances
    # This provides a single bridge for all Ruby step handler execution
    def get_shared_step_handler
      orchestration_system # Ensure initialized

      # Create a shared step execution bridge that handles FFI delegation
      # This is instantiated once and used by all Ruby step handlers
      @shared_step_handler ||= begin
        logger.info "✅ Creating shared RubyStepHandler FFI bridge"
        # RubyStepHandler acts as shared bridge - no per-step configuration needed
        handle = orchestration_handle
        if handle
          # Get step executor from orchestration system
          step_executor = handle.step_executor if handle.respond_to?(:step_executor)
          TaskerCore::RubyStepHandler.new("SharedBridge", "shared", step_executor)
        else
          logger.error "Failed to get orchestration handle for step handler bridge"
          nil
        end
      rescue StandardError => e
        error_msg = "Failed to create shared step handler: #{e.message}"
        logger.error error_msg
        nil
      end
    end

    # 🎯 NEW: Handle-based architecture methods for optimal FFI performance

    # Get or create the orchestration handle (SINGLE INITIALIZATION)
    def orchestration_handle
      @orchestration_handle ||= begin
        logger.info "🎯 HANDLE ARCHITECTURE: Creating orchestration handle with persistent references"
        TaskerCore::OrchestrationHandle.new
      rescue StandardError => e
        error_msg = "Failed to create orchestration handle: #{e.message}"
        logger.error error_msg
        nil
      end
    end

    # Handle-based factory operations (NO global lookups!)
    def create_test_task_with_handle(options)
      handle = orchestration_handle
      return nil unless handle

      handle.create_test_task(options)
    end

    def create_test_workflow_step_with_handle(options)
      handle = orchestration_handle
      return nil unless handle

      handle.create_test_workflow_step(options)
    end

    def create_test_foundation_with_handle(options)
      handle = orchestration_handle
      return nil unless handle

      handle.create_test_foundation(options)
    end

    # Handle-based orchestration operations (NO global lookups!)
    def register_handler_with_handle(handler_data)
      handle = orchestration_handle
      return nil unless handle

      handle.register_handler(handler_data)
    end

    def find_handler_with_handle(task_request)
      handle = orchestration_handle
      return nil unless handle

      handle.find_handler(task_request)
    end

    # Get handle info for debugging
    def handle_info
      handle = orchestration_handle
      return { status: 'no_handle' } unless handle

      handle.info
    end

    # 🎯 PHASE 2: Performance operations with handle delegation (NO global lookups!)

    def analyze_dependencies_with_handle(task_id)
      handle = orchestration_handle
      return nil unless handle

      TaskerCore.analyze_dependencies_with_handle(handle, task_id)
    end

    def get_task_execution_context_with_handle(task_id)
      handle = orchestration_handle
      return nil unless handle

      TaskerCore::Performance.get_task_execution_context_with_handle(handle, task_id)
    end

    def discover_viable_steps_with_handle(task_id)
      handle = orchestration_handle
      return nil unless handle

      TaskerCore.discover_viable_steps_with_handle(handle, task_id)
    end

    def get_system_health_with_handle
      handle = orchestration_handle
      return nil unless handle

      TaskerCore::Performance.get_system_health_with_handle(handle)
    end

    def get_analytics_metrics_with_handle(time_range_hours = nil)
      handle = orchestration_handle
      return nil unless handle

      TaskerCore::Performance.get_analytics_metrics_with_handle(handle, time_range_hours)
    end

    # 🎯 PHASE 2: Event operations with handle delegation (NO global lookups!)

    def publish_simple_event_with_handle(event_data)
      handle = orchestration_handle
      return nil unless handle

      TaskerCore.publish_simple_event_with_handle(handle, event_data)
    end

    def publish_orchestration_event_with_handle(event_data)
      handle = orchestration_handle
      return nil unless handle

      TaskerCore.publish_orchestration_event_with_handle(handle, event_data)
    end

    def subscribe_to_events_with_handle(subscription_data)
      handle = orchestration_handle
      return nil unless handle

      TaskerCore.subscribe_to_events_with_handle(handle, subscription_data)
    end

    def get_event_stats_with_handle
      handle = orchestration_handle
      return nil unless handle

      TaskerCore::Events.get_event_stats_with_handle(handle)
    end

    def register_external_event_callback_with_handle(callback_data)
      handle = orchestration_handle
      return nil unless handle

      TaskerCore.register_external_event_callback_with_handle(handle, callback_data)
    end

    # 🎯 PHASE 2: Testing operations with handle delegation (NO global lookups!)

    def create_testing_framework_with_handle
      handle = orchestration_handle
      return nil unless handle

      TaskerCore::TestHelpers::TestingFramework.create_testing_framework_with_handle(handle)
    end

    def setup_test_environment_with_handle
      handle = orchestration_handle
      return nil unless handle

      TaskerCore::TestHelpers::TestingFramework.setup_test_environment_with_handle(handle)
    end

    def cleanup_test_environment_with_handle
      handle = orchestration_handle
      return nil unless handle

      TaskerCore::TestHelpers::TestingFramework.cleanup_test_environment_with_handle(handle)
    end

    # 🎯 ZEROMQ BATCH PROCESSING: Cross-language orchestration integration
    
    # Get or create BatchStepExecutionOrchestrator with shared ZMQ context
    def batch_step_orchestrator
      @batch_step_orchestrator ||= begin
        handle = orchestration_handle
        return nil unless handle
        
        # Check if ZeroMQ is enabled in Rust configuration
        return nil unless handle.is_zeromq_enabled
        
        # Get ZeroMQ configuration from Rust system
        zmq_config = handle.zeromq_config
        
        logger.info "🚀 Creating BatchStepExecutionOrchestrator with ZeroMQ config from Rust"
        
        # Create orchestrator with TCP endpoints from Rust configuration
        TaskerCore::Orchestration::BatchStepExecutionOrchestrator.new(
          step_sub_endpoint: zmq_config['batch_endpoint'],
          result_pub_endpoint: zmq_config['result_endpoint'],
          max_workers: 10, # Could be configurable
          handler_registry: self,
          zmq_context: nil # TCP endpoints don't require shared context
        )
      rescue StandardError => e
        logger.error "Failed to create BatchStepExecutionOrchestrator: #{e.message}"
        nil
      end
    end
    
    # Get ZMQ context for Ruby orchestration
    # Using TCP endpoints, we don't need to share the exact same context as Rust
    def get_shared_zmq_context
      handle = orchestration_handle
      return nil unless handle
      
      # For TCP communication, each side can have its own ZMQ context
      logger.debug "🔗 Creating Ruby ZMQ context for TCP communication with Rust"
      
      require 'ffi-rzmq'
      ZMQ::Context.new
    rescue StandardError => e
      logger.error "Failed to create ZMQ context: #{e.message}"
      nil
    end
    
    # Start ZeroMQ batch processing integration
    def start_zeromq_integration
      orchestrator = batch_step_orchestrator
      return false unless orchestrator
      
      logger.info "🎯 Starting ZeroMQ batch processing integration"
      orchestrator.start
      true
    rescue StandardError => e
      logger.error "Failed to start ZeroMQ integration: #{e.message}"
      false
    end
    
    # Stop ZeroMQ batch processing integration
    def stop_zeromq_integration
      return unless @batch_step_orchestrator
      
      logger.info "🛑 Stopping ZeroMQ batch processing integration"
      @batch_step_orchestrator.stop
      @batch_step_orchestrator = nil
    rescue StandardError => e
      logger.error "Failed to stop ZeroMQ integration: #{e.message}"
    end
    
    # Check if ZeroMQ integration is available and running
    def zeromq_integration_status
      handle = orchestration_handle
      return { enabled: false, reason: 'no_handle' } unless handle
      
      return { enabled: false, reason: 'zeromq_disabled' } unless handle.is_zeromq_enabled
      
      orchestrator_running = @batch_step_orchestrator&.stats&.dig(:running) || false
      
      {
        enabled: true,
        orchestrator_running: orchestrator_running,
        zeromq_config: handle.zeromq_config,
        orchestrator_stats: @batch_step_orchestrator&.stats
      }
    rescue StandardError => e
      { enabled: false, error: e.message }
    end

    # ========================================================================
    # RUBY TASK HANDLER REGISTRY (Ruby-Centric Architecture)
    # ========================================================================

    # Registry of Ruby TaskHandler instances for direct step execution
    def ruby_task_handlers
      @ruby_task_handlers ||= {}
    end

    # Register a Ruby TaskHandler instance for step execution
    # @param namespace [String] Task namespace
    # @param name [String] Task name  
    # @param version [String] Task version
    # @param handler [TaskerCore::TaskHandler::Base] TaskHandler instance with pre-instantiated step handlers
    def register_ruby_task_handler(namespace, name, version, handler)
      key = "#{namespace}/#{name}/#{version}"
      ruby_task_handlers[key] = handler
      @logger&.debug "📝 Registered Ruby TaskHandler: #{key} with #{handler.step_handlers&.size || 0} step handlers"
      true
    end

    # Get Ruby TaskHandler for a specific task configuration
    # @param namespace [String] Task namespace
    # @param name [String] Task name
    # @param version [String] Task version
    # @return [TaskerCore::TaskHandler::Base, nil] TaskHandler instance or nil if not found
    def get_ruby_task_handler(namespace, name, version)
      key = "#{namespace}/#{name}/#{version}"
      ruby_task_handlers[key]
    end

    # Get Ruby TaskHandler for a specific task_id by looking up task metadata
    # This is the method called from Rust FFI to find the appropriate handler
    # @param task_id [Integer] Task ID to find handler for
    # @return [TaskerCore::TaskHandler::Base, nil] TaskHandler instance or nil if not found
    def get_task_handler_for_task(task_id)
      # TODO: Implement task metadata lookup to find namespace/name/version for task_id
      # For now, return the first available TaskHandler as fallback
      # In production, this would query the database to get task metadata, then lookup handler
      
      if ruby_task_handlers.any?
        handler = ruby_task_handlers.values.first
        @logger&.debug "🔍 Found TaskHandler for task_id #{task_id}: #{handler.class.name}"
        handler
      else
        @logger&.warn "⚠️  No TaskHandler found for task_id #{task_id}"
        nil
      end
    end

    # List all registered Ruby TaskHandlers
    # @return [Hash] Hash of registered handlers by key
    def list_ruby_task_handlers
      ruby_task_handlers.keys.map do |key|
        {
          key: key,
          handler_class: ruby_task_handlers[key].class.name,
          step_handler_count: ruby_task_handlers[key].step_handlers&.size || 0
        }
      end
    end
    end
  end
end
