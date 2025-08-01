# frozen_string_literal: true
#
require 'tasker_core/execution'
require 'tasker_core/execution/worker_manager'
require 'tasker_core/execution/command_listener'
require 'tasker_core/execution/batch_execution_handler'
require 'tasker_core/embedded_server'

module TaskerCore
  module OrchestrationHelpers
    def logger
      TaskerCore::Logging::Logger.instance
    end

    def start_embedded_server
      # Start embedded server for command processing using configured values
      config = TaskerCore::Config.instance.effective_config

      # Get configured values - fail fast if missing
      if config.nil? || config.dig("command_backplane", "core").nil?
        raise "Missing command_backplane.core configuration in test setup"
      end

      core_host = config.dig("command_backplane", "core", "host")
      core_port = config.dig("command_backplane", "core", "port")

      if core_host.nil? || core_port.nil?
        raise "Missing required command_backplane.core configuration (host: #{core_host.inspect}, port: #{core_port.inspect})"
      end

      server_config = {
        bind_address: "#{core_host}:#{core_port}",  # Use configured address and port
        command_queue_size: 100,
        connection_timeout_ms: 5000,
        graceful_shutdown_timeout_ms: 2000,
        max_connections: 10,
        background: true
      }

      @embedded_server = TaskerCore::EmbeddedServer.new(server_config)
      @embedded_server.start(block_until_ready: true, ready_timeout: 10)
    end

    def stop_worker_manager
      if @worker_manager
        begin
          @worker_manager.stop_heartbeat
          @worker_manager.stop
        rescue StandardError => e
          puts "⚠️ Warning: Failed to cleanup worker manager: #{e.message}"
        end
      end
    end

    def stop_embedded_server
      if @embedded_server&.running?
        @embedded_server.stop(timeout: 5)
      end
    end

    def start_worker_manager
      logger.info "🔧 Registering worker with explicit task handler information..."

      begin
        # Load YAML configuration directly (can't use let declarations in before(:all))
        config_path = File.expand_path('handlers/examples/order_fulfillment/config/order_fulfillment_handler.yaml', __dir__)
        loaded_task_config = YAML.load_file(config_path)

        # Extract task handler information from YAML configuration
        supported_tasks = [{
          namespace: loaded_task_config['namespace_name'],
          handler_name: loaded_task_config['name'],
          version: loaded_task_config['version'],
          handler_class: loaded_task_config['task_handler_class'],
          description: loaded_task_config['description'] || "Auto-registered from integration test",
          supported_step_types: loaded_task_config['step_templates']&.map { |step| step['name'] } || [],
          handler_config: loaded_task_config,
          priority: 100,
          timeout_ms: 5000,  # Reduced from 30000 to 5000 for faster feedback
          supports_retries: true
        }]

        logger.debug "📋 Task handler configuration loaded: #{supported_tasks.first[:namespace]}/#{supported_tasks.first[:handler_name]}"
        logger.debug "🎯 DEBUG: supported_step_types = #{supported_tasks.first[:supported_step_types]}"
        logger.debug "🚫 SKIPPING FFI registration - using command pattern instead"

        worker_id = "integration_test_worker_#{rand(10000..99999)}"
        @worker_manager = TaskerCore::Execution::WorkerManager.new(
          worker_id: worker_id,
          supported_namespaces: [loaded_task_config['namespace_name']],
          heartbeat_interval: 5,  # Reduced from 30 to 5 seconds for faster testing
          custom_capabilities: {
            'integration_test' => true,
            'order_fulfillment_capable' => true,
            'supports_all_step_types' => true,
            'explicit_task_registration' => true,
            'manager_type' => 'rust_backed',
            'supports_execute_batch' => true,
            'ruby_worker' => true,
            'command_listener_running' => true
          },
          supported_tasks: supported_tasks
        )

        logger.debug "🎯 DEBUG: About to start worker - this should send RegisterWorker command with task handler info"
        logger.debug "🎯 DEBUG: Worker ID = #{worker_id}"
        logger.debug "🎯 DEBUG: Supported namespaces = #{@worker_manager.supported_namespaces}"

        # Start the worker - this sends RegisterWorker command via TCP to Rust
        @worker_manager.start

        logger.debug "✅ Worker registered successfully via command pattern (not FFI)"
        logger.debug "💓 Worker heartbeat started, worker is now active"
        logger.debug "🎯 DEBUG: Worker manager running = #{@worker_manager.running?}"

        logger.debug "✅ Direct TCP CommandListener will be created by WorkerManager - no tracking hooks needed"

      rescue StandardError => e
        logger.error "❌ Failed to register worker with task handlers: #{e.message}"
        logger.error "Backtrace: #{e.backtrace.first(3).join(', ')}"
        raise
      end
    end
  end
end
