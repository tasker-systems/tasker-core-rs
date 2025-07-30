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
        task_request = TaskerCore::Types::TaskRequest.new(task_request.deep_symbolize_keys)

        # Get command client and send command
        orchestration_manager = TaskerCore::Internal::OrchestrationManager.instance
        command_client = orchestration_manager.create_command_client

        # Send InitializeTask command and return response directly
        command_client.initialize_task(task_request.to_ffi_hash)
      end

      private

      def load_task_config_from_path(path)
        return {} unless path && File.exist?(path)
        YAML.load_file(path)
      rescue StandardError => e
        logger.warn "Error loading task configuration: #{e.message}"
        {}
      end
    end
  end
end
