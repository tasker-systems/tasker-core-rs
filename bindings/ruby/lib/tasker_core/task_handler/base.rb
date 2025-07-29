# frozen_string_literal: true

require 'yaml'
require 'logger'
require_relative '../orchestration/orchestration_manager'

module TaskerCore
  module TaskHandler
    class Base
      # Ruby wrapper that sends commands to Rust orchestrator
      # This class is now extremely simple:
      # - initialize_task: Send InitializeTask command to Rust
      # - handle(task_id): Send TryTaskIfReady command to Rust
      # No FFI, no registration, no complex logic - just command delegation

      attr_reader :logger, :task_config

      def initialize(config: {}, task_config_path: nil, task_config: nil)
        @logger = TaskerCore::Logging::Logger.instance
        @task_config = task_config || (task_config_path ? load_task_config_from_path(task_config_path) : {})
        
        # Register this handler's configuration with the TaskHandlerRegistry
        register_handler_configuration if @task_config && !@task_config.empty?
      end

      # Main task processing method - Rails engine signature: handle(task_id)
      # Send TryTaskIfReady command to Rust orchestrator and return response directly
      def handle(task_id)
        raise TaskerCore::ValidationError, 'task_id is required' unless task_id
        raise TaskerCore::ValidationError, 'task_id must be an integer' unless task_id.is_a?(Integer)

        logger.info "Sending TryTaskIfReady command for task #{task_id}"
        
        # Get command client and send command
        orchestration_manager = TaskerCore::Internal::OrchestrationManager.instance
        command_client = orchestration_manager.create_command_client
        
        # Send command and return response directly
        command_client.try_task_if_ready(task_id)
      end

      # Initialize a new task with workflow steps
      # Send InitializeTask command to Rust orchestrator and return response directly
      def initialize_task(task_request)
        logger.info "Sending InitializeTask command"
        
        # Ensure task_request has required fields with defaults
        task_request = task_request.dup
        task_request['version'] ||= '0.1.0'  # Default version if not provided
        task_request['namespace'] ||= 'default'  # Default namespace if not provided
        task_request['status'] ||= 'PENDING'  # Default status for new tasks
        task_request['complete'] ||= false  # Default complete state
        task_request['tags'] ||= []  # Default empty tags array
        task_request['bypass_steps'] ||= []  # Default empty bypass_steps array
        task_request['requested_at'] ||= Time.now.utc.strftime('%Y-%m-%dT%H:%M:%S')  # Default to current time in ISO format
        
        # Get command client and send command
        orchestration_manager = TaskerCore::Internal::OrchestrationManager.instance
        command_client = orchestration_manager.create_command_client
        
        # Send InitializeTask command and return response directly
        command_client.initialize_task(task_request)
      end

      private

      def load_task_config_from_path(path)
        return {} unless path && File.exist?(path)
        YAML.load_file(path)
      rescue StandardError => e
        logger.warn "Error loading task configuration: #{e.message}"
        {}
      end

      # Register this handler's configuration with the TaskHandlerRegistry
      def register_handler_configuration
        return unless @task_config

        # Extract required registration information
        namespace = @task_config['namespace_name']
        task_name = @task_config['name']
        version = @task_config['version']
        
        unless namespace && task_name && version
          logger.warn "TaskHandler configuration missing required fields (namespace_name, name, version): #{@task_config.keys}"
          return
        end

        logger.info "Registering TaskHandler: #{namespace}/#{task_name}/#{version}"

        begin
          # Get the orchestration manager to access the TaskHandlerRegistry
          orchestration_manager = TaskerCore::Internal::OrchestrationManager.instance
          orchestration_handle = orchestration_manager.orchestration_handle

          # Convert Ruby task_config to format expected by Rust registry
          # The entire task_config goes in config_schema to preserve step_templates
          handler_metadata = {
            'namespace' => namespace,
            'name' => task_name,
            'version' => version,
            'handler_class' => @task_config['task_handler_class'] || self.class.name,
            'config_schema' => @task_config  # Pass entire config including step_templates
          }

          # Register with the Rust TaskHandlerRegistry
          orchestration_handle.register_handler(handler_metadata)
          logger.info "✅ Successfully registered TaskHandler: #{namespace}/#{task_name}/#{version}"

        rescue StandardError => e
          logger.error "Failed to register TaskHandler configuration: #{e.message}"
          logger.debug e.backtrace.join("\n")
        end
      end
    end
  end
end