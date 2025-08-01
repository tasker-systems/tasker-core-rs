# frozen_string_literal: true

require 'singleton'


module TaskerCore
  module Internal
    # Ruby-level wrapper for the unified orchestration system
    # This provides a singleton pattern at the Ruby level to avoid
    # re-initializing the orchestration system on every call
    class OrchestrationManager
    include Singleton

    attr_reader :initialized_at, :status, :logger, :base_task_handler

    def initialize
      @initialized = false
      @status = 'not_initialized'
      @initialized_at = nil
      @orchestration_system = nil
      @base_task_handler = nil  # 🎯 Memoized BaseTaskHandler
      @orchestration_handle = nil  # 🎯 NEW: Handle-based FFI architecture
      @command_client = nil  # 🎯 PERSISTENT: Cached CommandClient for connection reuse
      @command_client_mutex = Mutex.new  # Thread-safe access to cached client
      @command_listener = nil  # 🎯 PERSISTENT: Cached CommandListener for worker server
      @command_listener_mutex = Mutex.new  # Thread-safe access to cached listener
      @worker_manager = nil  # 🎯 PERSISTENT: Cached WorkerManager for worker lifecycle
      @worker_manager_mutex = Mutex.new  # Thread-safe access to cached worker manager
      @logger = TaskerCore::Logging::Logger.instance
    end

    # Get the orchestration system (handle-based initialization)
    def orchestration_system
      # Use handle-based approach - no direct system initialization needed
      if orchestration_handle && !@initialized
        logger.debug "Initializing orchestration system with TCP architecture"
        @initialized = true
        @status = 'initialized'
        @initialized_at = defined?(Rails) ? Time.current : Time.now
        @orchestration_system ||= { 'handle_based' => true, 'architecture' => 'tcp' }

        # NOTE: No batch orchestrator or ZeroMQ integration needed with TCP architecture
        # TCP executor handles all orchestration internally via embedded server
        logger.debug "TCP orchestration system initialized"
      end

      unless @orchestration_system
        raise TaskerCore::Errors::OrchestrationError, 'Orchestration system not initialized'
      end

      @orchestration_system
    end

    # Check if the orchestration system is initialized
    def initialized?
      # Trigger initialization if not already done
      orchestration_system
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

      # Reset cached command client and disconnect if connected
      @command_client_mutex.synchronize do
        if @command_client&.connected?
          begin
            @command_client.disconnect
          rescue StandardError => e
            @logger.warn "Failed to disconnect cached command client during reset: #{e.message}"
          end
        end
        @command_client = nil
      end

      # Reset cached command listener and stop if running
      @command_listener_mutex.synchronize do
        if @command_listener&.running?
          begin
            @command_listener.stop
          rescue StandardError => e
            @logger.warn "Failed to stop cached command listener during reset: #{e.message}"
          end
        end
        @command_listener = nil
      end

      # Reset cached worker manager and stop if running
      @worker_manager_mutex.synchronize do
        if @worker_manager&.running?
          begin
            @worker_manager.stop
          rescue StandardError => e
            @logger.warn "Failed to stop cached worker manager during reset: #{e.message}"
          end
        end
        @worker_manager = nil
      end
      # NOTE: No batch orchestrator to stop with TCP architecture
    end

    def logger
      TaskerCore::Logging::Logger.instance
    end

    # Get or create the orchestration handle (SINGLE INITIALIZATION)
    def orchestration_handle
      @orchestration_handle ||= begin
        logger.info "🎯 HANDLE ARCHITECTURE: Creating orchestration handle with persistent references"
        TaskerCore::OrchestrationHandle.new
      rescue StandardError => e
        error_msg = "Failed to create orchestration handle: #{e.class.name}: #{e.message}\n#{e.backtrace.first(5).join("\n")}"
        logger.error error_msg
        raise TaskerCore::Errors::OrchestrationError, error_msg
      end
    end

    # Get handle info for debugging
    def handle_info
      handle = orchestration_handle
      return { status: 'no_handle' } unless handle

      handle.info
    end

    # TCP integration status - replaces ZeroMQ status
    def tcp_integration_status
      handle = orchestration_handle
      return { enabled: false, reason: 'no_handle' } unless handle

      # Check TCP executor enabled status with error handling
      tcp_enabled = begin
        handle.is_tcp_executor_enabled
      rescue StandardError => e
        logger.debug "TCP executor status check failed: #{e.message}"
        false
      end

      return { enabled: false, reason: 'tcp_executor_disabled' } unless tcp_enabled

      # Get TCP executor config with error handling
      tcp_config = begin
        handle.tcp_executor_config
      rescue StandardError => e
        logger.debug "TCP executor config retrieval failed: #{e.message}"
        {}
      end

      {
        enabled: true,
        running: true, # TCP executor manages its own lifecycle
        architecture: 'tcp',
        tcp_config: tcp_config
      }
    rescue StandardError => e
      { enabled: false, error: e.message }
    end

    # 🎯 SHARED FFI ARCHITECTURE INTEGRATION (Phase 3)

    # Get the singleton CommandClient for this Ruby process
    #
    # This ensures exactly ONE CommandClient exists for the entire process lifetime,
    # with internal mutex-protected connection lifecycle management.
    #
    # @param host [String] Server host (optional, uses config default)
    # @param port [Integer] Server port (optional, uses config default)
    # @param timeout [Integer] Connection timeout (optional, uses config default)
    # @return [TaskerCore::Execution::CommandClient] Singleton command client
    def create_command_client(host: nil, port: nil, timeout: nil)
      orchestration_system # Ensure initialized

      @command_client_mutex.synchronize do
        # Return existing singleton if it exists and configuration matches
        if @command_client && client_config_matches?(@command_client, host, port, timeout)
          logger.debug "🔄 SINGLETON: Reusing existing CommandClient (auto-reconnects as needed)"
          return @command_client
        end

        # Disconnect existing client if configuration changed
        if @command_client
          logger.info "🔧 SINGLETON: Configuration changed - replacing CommandClient"
          begin
            @command_client.disconnect if @command_client.connected?
          rescue StandardError => e
            logger.warn "Failed to disconnect existing client: #{e.message}"
          end
        end

        # Create new singleton client with internal connection management
        begin
          logger.info "🎯 SINGLETON: Creating process-wide CommandClient singleton"
          @command_client = TaskerCore::Execution::CommandClient.new(
            host: host,
            port: port,
            timeout: timeout
          )

          logger.info "✅ SINGLETON: CommandClient created (will auto-connect on first command)"
          @command_client
        rescue StandardError => e
          @command_client = nil
          error_msg = "Failed to create singleton CommandClient: #{e.class.name}: #{e.message}"
          logger.error error_msg
          raise TaskerCore::Errors::OrchestrationError, error_msg
        end
      end
    end

    # Create singleton CommandListener for Ruby worker server
    #
    # @param host [String] Host to bind to (optional, uses config default)
    # @param port [Integer] Port to bind to (optional, uses config default)
    # @param timeout [Integer] Connection timeout (optional, uses config default)
    # @param max_connections [Integer] Maximum connections (optional, uses default)
    # @return [TaskerCore::Execution::CommandListener] Singleton command listener
    def create_command_listener(host: nil, port: nil, timeout: nil, max_connections: nil)
      orchestration_system # Ensure initialized

      @command_listener_mutex.synchronize do
        # Return existing singleton if it exists and configuration matches
        if @command_listener && listener_config_matches?(@command_listener, host, port, timeout, max_connections)
          logger.debug "🔄 SINGLETON: Reusing existing CommandListener (auto-manages lifecycle)"
          return @command_listener
        end

        # Stop existing listener if configuration changed
        if @command_listener
          logger.info "🔧 SINGLETON: Configuration changed - replacing CommandListener"
          begin
            @command_listener.stop if @command_listener.running?
          rescue StandardError => e
            logger.warn "Failed to stop existing listener: #{e.message}"
          end
        end

        # Create new singleton listener with internal server management
        begin
          logger.info "🎯 SINGLETON: Creating process-wide CommandListener singleton"
          @command_listener = TaskerCore::Execution::CommandListener.new(
            host: host,
            port: port,
            timeout: timeout,
            max_connections: max_connections
          )

          logger.info "✅ SINGLETON: CommandListener created (call start() to begin accepting commands)"
          @command_listener
        rescue StandardError => e
          @command_listener = nil
          error_msg = "Failed to create singleton CommandListener: #{e.class.name}: #{e.message}"
          logger.error error_msg
          raise TaskerCore::Errors::OrchestrationError, error_msg
        end
      end
    end

    # Create singleton WorkerManager for Ruby worker lifecycle
    #
    # @param worker_id [String] Unique worker identifier
    # @param mode [Symbol] Worker mode (:worker, :server, :hybrid)
    # @param supported_tasks [Array<Hash>] Explicit task template definitions (Phase 2)
    # @param supported_namespaces [Array<String>] Legacy namespace support (auto-discovered if nil)
    # @param max_concurrent_steps [Integer] Maximum concurrent steps (optional, uses default)
    # @param heartbeat_interval [Integer] Heartbeat interval in seconds (optional, uses default)
    # @param server_host [String] Server host (optional, uses config default)
    # @param server_port [Integer] Server port (optional, uses config default)
    # @param bind_port [Integer] Port to bind for server mode (optional)
    # @return [TaskerCore::Execution::WorkerManager] Singleton worker manager
    def create_worker_manager(worker_id:, mode: :worker, supported_tasks: nil,
                             supported_namespaces: nil, max_concurrent_steps: nil,
                             heartbeat_interval: nil, server_host: nil, server_port: nil,
                             bind_port: nil)
      orchestration_system # Ensure initialized

      @worker_manager_mutex.synchronize do
        # Return existing singleton if it exists and configuration matches
        if @worker_manager && worker_manager_config_matches?(@worker_manager, worker_id, mode,
                                                           supported_tasks, supported_namespaces,
                                                           max_concurrent_steps, heartbeat_interval,
                                                           server_host, server_port, bind_port)
          logger.debug "🔄 SINGLETON: Reusing existing WorkerManager (auto-manages lifecycle)"
          return @worker_manager
        end

        # Stop existing worker manager if configuration changed
        if @worker_manager
          logger.info "🔧 SINGLETON: Configuration changed - replacing WorkerManager"
          begin
            @worker_manager.stop if @worker_manager.running?
          rescue StandardError => e
            logger.warn "Failed to stop existing worker manager: #{e.message}"
          end
        end

        # Create new singleton worker manager with internal lifecycle management
        begin
          logger.info "🎯 SINGLETON: Creating process-wide WorkerManager singleton"
          @worker_manager = TaskerCore::Execution::WorkerManager.new(
            worker_id: worker_id,
            mode: mode,
            supported_tasks: supported_tasks,
            supported_namespaces: supported_namespaces,
            max_concurrent_steps: max_concurrent_steps,
            heartbeat_interval: heartbeat_interval,
            server_host: server_host,
            server_port: server_port,
            bind_port: bind_port
          )

          logger.info "✅ SINGLETON: WorkerManager created (call start() to begin worker lifecycle)"
          @worker_manager
        rescue StandardError => e
          @worker_manager = nil
          error_msg = "Failed to create singleton WorkerManager: #{e.class.name}: #{e.message}"
          logger.error error_msg
          raise TaskerCore::Errors::OrchestrationError, error_msg
        end
      end
    end

    # Get command architecture status and configuration
    #
    # @return [Hash] Command architecture information
    def command_architecture_status
      orchestration_system # Ensure initialized

      begin
        tcp_status = tcp_integration_status

        {
          architecture: 'rust_backed_commands',
          tcp_integration: tcp_status,
          rust_ffi_available: defined?(TaskerCore::CommandClient),
          worker_manager_available: defined?(TaskerCore::WorkerManager),
          command_listener_available: defined?(TaskerCore::CommandListener),
          components: {
            command_client: 'TaskerCore::Execution::CommandClient (Rust-backed)',
            worker_manager: 'TaskerCore::Execution::WorkerManager (Rust-backed)',
            orchestration_handle: 'TaskerCore::OrchestrationHandle (Handle-based FFI)'
          }
        }
      rescue StandardError => e
        {
          architecture: 'error',
          error: e.message,
          components: {}
        }
      end
    end

    # Auto-discover and register a Ruby worker with Phase 3 SharedWorkerManager
    #
    # This method provides a high-level interface for Ruby applications to easily
    # register workers with the Rust orchestration system using the new SharedWorkerManager
    # architecture with enhanced task template support (Phase 2).
    #
    # @param worker_id [String] Unique worker identifier
    # @param supported_tasks [Array<Hash>] Explicit task template definitions (Phase 2)
    # @param custom_capabilities [Hash] Additional worker capabilities (optional)
    # @param mode [Symbol] Worker mode (:worker, :server, :hybrid) (optional, defaults to :worker)
    # @return [TaskerCore::Execution::SharedWorkerManager] Configured and started shared worker manager
    # @raise [TaskerCore::Errors::OrchestrationError] if worker registration fails
    def register_ruby_worker(worker_id:, supported_tasks: nil, custom_capabilities: {}, mode: :worker)
      orchestration_system # Ensure initialized

      begin
        logger.info "🎯 Auto-registering Ruby worker with SharedWorkerManager (Phase 3): #{worker_id}"

        # Create shared worker manager with Phase 2 task template support
        worker_manager = create_shared_worker_manager(
          worker_id: worker_id,
          mode: mode,
          supported_tasks: supported_tasks,
          custom_capabilities: custom_capabilities.merge(
            'auto_registered' => true,
            'orchestration_manager' => true,
            'ruby_integration' => 'shared_ffi',
            'phase3_enhanced' => true
          )
        )

        # Initialize based on mode
        case mode.to_sym
        when :worker, :hybrid
          # Initialize as worker and register
          success = worker_manager.initialize_as_worker
          unless success
            raise TaskerCore::Errors::OrchestrationError, "Failed to initialize worker #{worker_id}"
          end

          # Register with enhanced task template support
          registration_response = worker_manager.register_worker
          logger.info "✅ Worker registered: #{registration_response}"

          # Start automatic heartbeat
          heartbeat_success = worker_manager.start_heartbeat
          unless heartbeat_success
            logger.warn "⚠️ Failed to start automatic heartbeat for worker #{worker_id}"
          end

        when :server
          # Initialize as server
          success = worker_manager.initialize_as_server
          unless success
            raise TaskerCore::Errors::OrchestrationError, "Failed to initialize server #{worker_id}"
          end

        else
          raise TaskerCore::Errors::OrchestrationError, "Unsupported worker mode: #{mode}"
        end

        logger.info "✅ Ruby worker #{worker_id} registered and started successfully with SharedWorkerManager"
        worker_manager
      rescue StandardError => e
        error_msg = "Failed to register Ruby worker #{worker_id}: #{e.class.name}: #{e.message}"
        logger.error error_msg
        raise TaskerCore::Errors::OrchestrationError, error_msg
      end
    end


    # Check if the cached command client matches the requested configuration
    #
    # @param client [TaskerCore::Execution::CommandClient] The cached client to check
    # @param host [String, nil] Requested host
    # @param port [Integer, nil] Requested port
    # @param timeout [Integer, nil] Requested timeout
    # @return [Boolean] true if configuration matches
    def client_config_matches?(client, host, port, timeout)
      # Get default values from config for comparison
      config = TaskerCore::Config.instance.effective_config

      # Require configuration - no hardcoded fallbacks
      if config.nil? || config.dig("command_backplane", "core").nil?
        logger.error "Missing command_backplane.core configuration for client config matching"
        return false
      end

      default_host = config.dig("command_backplane", "core", "host")
      default_port = config.dig("command_backplane", "core", "port")
      default_timeout = config.dig("command_backplane", "core", "timeout") || 30  # Only timeout has a default

      # Use defaults if parameters are nil
      requested_host = host || default_host
      requested_port = port || default_port
      requested_timeout = timeout || default_timeout

      # Compare with client's current configuration
      client.host == requested_host &&
      client.port == requested_port &&
      client.timeout == requested_timeout
    end

    # Check if CommandListener configuration matches requested parameters
    def listener_config_matches?(listener, host, port, timeout, max_connections)
      # Get default values from config for comparison
      config = TaskerCore::Config.instance.effective_config

      # Require configuration - no hardcoded fallbacks
      if config.nil? || config.dig("command_backplane", "worker").nil?
        logger.error "Missing command_backplane.worker configuration for listener config matching"
        return false
      end

      default_host = config.dig("command_backplane", "worker", "host")
      default_port = config.dig("command_backplane", "worker", "port")
      default_timeout = config.dig("command_backplane", "worker", "timeout") || 30  # Only timeout has a default
      default_max_connections = 10  # This is a reasonable default for connections

      # Use defaults if parameters are nil
      requested_host = host || default_host
      requested_port = port || default_port
      requested_timeout = timeout || default_timeout
      requested_max_connections = max_connections || default_max_connections

      # Compare with listener's current configuration
      listener.host == requested_host &&
      listener.port == requested_port &&
      listener.timeout == requested_timeout &&
      listener.max_connections == requested_max_connections
    end

    # Check if WorkerManager configuration matches requested parameters
    def worker_manager_config_matches?(worker_manager, worker_id, mode, supported_tasks,
                                     supported_namespaces, max_concurrent_steps,
                                     heartbeat_interval, server_host, server_port, bind_port)
      # Get default values from config for comparison
      config = TaskerCore::Config.instance.effective_config

      # Require configuration - no hardcoded fallbacks for command backplane
      if config.nil? || config.dig("command_backplane").nil?
        logger.error "Missing command_backplane configuration for worker manager config matching"
        return false
      end

      default_max_concurrent_steps = config.dig("execution", "max_concurrent_steps") || 1000
      default_heartbeat_interval = 30
      default_server_host = config.dig("command_backplane", "core", "host")
      default_server_port = config.dig("command_backplane", "core", "port")
      default_bind_port = config.dig("command_backplane", "worker", "port")

      # Use defaults if parameters are nil
      requested_worker_id = worker_id
      requested_mode = mode || :worker
      requested_supported_tasks = supported_tasks
      requested_supported_namespaces = supported_namespaces
      requested_max_concurrent_steps = max_concurrent_steps || default_max_concurrent_steps
      requested_heartbeat_interval = heartbeat_interval || default_heartbeat_interval
      requested_server_host = server_host || default_server_host
      requested_server_port = server_port || default_server_port
      requested_bind_port = bind_port || default_bind_port

      # Compare with worker manager's current configuration
      worker_manager.worker_id == requested_worker_id &&
      worker_manager.mode == requested_mode &&
      worker_manager.supported_tasks == requested_supported_tasks &&
      worker_manager.supported_namespaces == requested_supported_namespaces &&
      worker_manager.max_concurrent_steps == requested_max_concurrent_steps &&
      worker_manager.heartbeat_interval == requested_heartbeat_interval &&
      worker_manager.server_host == requested_server_host &&
      worker_manager.server_port == requested_server_port &&
      worker_manager.bind_port == requested_bind_port
    end
    end
  end
end
