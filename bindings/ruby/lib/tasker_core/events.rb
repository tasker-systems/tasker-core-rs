# frozen_string_literal: true

require 'dry-events'
require 'set'

module TaskerCore
  # Event system that bridges Rust orchestration events (source of truth) to Ruby
  # dry-events for ergonomic Ruby subscribers.
  #
  # Architecture:
  # - **Rust Orchestration Layer**: Source of truth for all events
  # - **Ruby Event Bridge**: Subscribes to Rust events and re-publishes via dry-events
  # - **Ruby Subscribers**: Use familiar Rails/dry-events patterns
  #
  # This maintains full compatibility with Rails engine event patterns while
  # leveraging Rust's high-performance event publishing.
  module Events

    # ========================================================================
    # RUBY EVENT BUS (receives events from Rust)
    # ========================================================================

    # Ruby event bus that receives events from Rust and republishes via dry-events
    # This provides Ruby ergonomics while Rust remains the source of truth
    class RubyEventBus
      include Dry::Events::Publisher[:tasker_core]

      def initialize
        # Register events dynamically as they come from Rust
        # This allows for dynamic event discovery without pre-registration
        @registered_events = Set.new
      end

      # Publish event from Rust to Ruby subscribers
      def publish_from_rust(event_type, payload)
        # Register event if we haven't seen it before
        unless @registered_events.include?(event_type)
          register_event(event_type)
          @registered_events.add(event_type)
        end

        # Publish to Ruby subscribers
        publish(event_type, payload)
      end

      # Get list of events that have been seen from Rust
      def discovered_events
        @registered_events.to_a.sort
      end
    end

    # Global event bus instance - single source for all Ruby subscribers
    BUS = RubyEventBus.new

    # ========================================================================
    # BASE SUBSCRIBER (mirrors Rails engine pattern)
    # ========================================================================

    # Base class for event subscribers that mirrors Rails engine patterns
    # Provides safe payload access, metrics extraction, and automatic method routing
    class BaseSubscriber
      include Dry::Events::Subscriber[:tasker_core]

      # Declarative subscription - mirrors Rails engine pattern
      def self.subscribe_to(*event_types)
        @subscribed_events = event_types

        # Subscribe to events and route to handler methods
        event_types.each do |event_type|
          BUS.subscribe(event_type) do |event|
            begin
              new.handle_event(event_type, event)
            rescue StandardError => e
              handle_subscriber_error(event_type, event, e)
            end
          end
        end
      end

      # Get events this subscriber listens to
      def self.subscribed_events
        @subscribed_events || []
      end

      # Handle incoming event by routing to appropriate method
      def handle_event(event_type, event_payload)
        method_name = "handle_#{event_type.tr('.', '_')}"

        if respond_to?(method_name, true)
          send(method_name, event_payload)
        else
          handle_unrouted_event(event_type, event_payload)
        end
      end

      # Override in subclasses to handle events that don't have specific methods
      def handle_unrouted_event(event_type, event_payload)
        # Default: do nothing
      end

      # Safe payload access with fallback values (mirrors Rails pattern)
      def safe_get(payload, key, default = nil)
        payload&.dig(key) || payload&.dig(key.to_s) || default
      end

      # Extract timing metrics from event payload
      def extract_timing_metrics(payload)
        {
          duration: safe_get(payload, :duration, 0.0),
          started_at: safe_get(payload, :started_at),
          completed_at: safe_get(payload, :completed_at),
          execution_duration: safe_get(payload, :execution_duration, 0.0)
        }
      end

      # Extract error information from event payload
      def extract_error_metrics(payload)
        error_info = safe_get(payload, :error, {})

        {
          error_type: safe_get(error_info, :type, 'unknown'),
          error_message: safe_get(error_info, :message),
          error_category: safe_get(error_info, :category, 'unknown'),
          error_code: safe_get(error_info, :code),
          permanent: safe_get(error_info, :permanent, false),
          retry_after: safe_get(error_info, :retry_after)
        }
      end

      # Get task and step IDs from payload
      def extract_identifiers(payload)
        {
          task_id: safe_get(payload, :task_id),
          step_id: safe_get(payload, :step_id),
          attempt_number: safe_get(payload, :attempt_number, 1)
        }
      end

      private

      # Handle errors in subscribers
      def self.handle_subscriber_error(event_type, event_payload, error)
        logger = defined?(Rails) ? Rails.logger : Logger.new($stderr)
        logger.error("Subscriber error handling #{event_type}: #{error.message}")
        logger.error(error.backtrace.join("\n")) if error.backtrace
      end
    end

    # ========================================================================
    # RUST EVENT BRIDGE (source of truth integration)
    # ========================================================================

    # Bridge that connects Rust orchestration events to Ruby subscribers
    # Rust is the authoritative source of events; Ruby provides ergonomic subscription
    class RustEventBridge
      attr_reader :rust_integration

      def initialize(rust_integration)
        @rust_integration = rust_integration
        @running = false
        @event_callbacks = {}
        @logger = defined?(Rails) ? Rails.logger : Logger.new($stdout)
      end

      # Start the bridge - connects to Rust event stream
      def start
        return if @running

        @running = true
        @logger.info("Starting Rust event bridge")

        # Register with Rust layer to receive events
        # TODO: Implement actual FFI callback registration
        # For now, this is a placeholder for the FFI integration
        register_with_rust_event_system

        @logger.info("Rust event bridge started")
      end

      # Stop the bridge
      def stop
        return unless @running

        @running = false
        @logger.info("Stopping Rust event bridge")

        # Unregister from Rust layer
        # TODO: Implement actual FFI unregistration

        @logger.info("Rust event bridge stopped")
      end

      # Handle event received from Rust orchestration layer
      # This is the main entry point called by Rust FFI
      def handle_rust_event(event_type, event_payload_json)
        return unless @running

        begin
          # Parse JSON payload from Rust
          event_payload = JSON.parse(event_payload_json, symbolize_names: true)

          # Add metadata to payload
          enriched_payload = enrich_event_payload(event_payload, event_type)

          # Publish to Ruby event bus
          BUS.publish_from_rust(event_type, enriched_payload)

          # Call any direct callbacks
          @event_callbacks[event_type]&.each do |callback|
            callback.call(enriched_payload)
          rescue StandardError => e
            @logger.error("Error in event callback for #{event_type}: #{e.message}")
          end

        rescue JSON::ParserError => e
          @logger.error("Failed to parse event payload from Rust: #{e.message}")
        rescue StandardError => e
          @logger.error("Error handling Rust event #{event_type}: #{e.message}")
        end
      end

      # Register direct callback for specific event type
      # Use this for low-level integration; prefer BaseSubscriber for normal use
      def register_callback(event_type, &callback)
        @event_callbacks[event_type] ||= []
        @event_callbacks[event_type] << callback
      end

      # Check if bridge is running
      def running?
        @running
      end

      # Get statistics about received events
      def statistics
        {
          running: @running,
          discovered_events: BUS.discovered_events.count,
          event_types: BUS.discovered_events,
          callback_registrations: @event_callbacks.transform_values(&:count)
        }
      end

      private

      # Register with Rust event system via FFI
      def register_with_rust_event_system
        # TODO: Implement actual FFI registration
        # This will involve calling into Rust to register this Ruby object
        # as the event callback handler for the orchestration system
        #
        # Example of what this might look like:
        # TaskerCore.register_event_callback(self)

        @logger.debug("Registered Ruby event bridge with Rust orchestration system")
      end

      # Enrich event payload with Ruby-specific metadata
      def enrich_event_payload(payload, event_type)
        payload.merge(
          ruby_received_at: Time.now.iso8601,
          ruby_event_type: event_type,
          source: 'rust_orchestration'
        )
      end
    end

    # ========================================================================
    # BUILT-IN EVENT SUBSCRIBERS (using Rails engine patterns)
    # ========================================================================

    # Performance monitoring subscriber that tracks slow operations
    # Mirrors Rails engine performance monitoring patterns
    class PerformanceMonitor < BaseSubscriber
      subscribe_to 'step.completed', 'task.completed', 'performance.bottleneck_detected'

      def initialize(logger: nil, slow_step_threshold: 30.0, slow_task_threshold: 300.0)
        @logger = logger || default_logger
        @slow_step_threshold = slow_step_threshold
        @slow_task_threshold = slow_task_threshold
      end

      # Handle completed steps and check for performance issues
      def handle_step_completed(event)
        timing = extract_timing_metrics(event)
        identifiers = extract_identifiers(event)

        duration = timing[:duration] || timing[:execution_duration] || 0.0

        if duration > @slow_step_threshold
          @logger.warn("Slow step detected: step_id=#{identifiers[:step_id]} duration=#{duration}s")

          # Note: Since Rust is source of truth, we don't publish events back
          # Instead, we could send feedback to Rust or just log/monitor
          log_slow_step(identifiers[:step_id], duration, event)
        end
      end

      # Handle completed tasks and check for performance issues
      def handle_task_completed(event)
        timing = extract_timing_metrics(event)
        identifiers = extract_identifiers(event)

        duration = timing[:duration] || timing[:execution_duration] || 0.0

        if duration > @slow_task_threshold
          @logger.warn("Slow task detected: task_id=#{identifiers[:task_id]} duration=#{duration}s")

          log_slow_task(identifiers[:task_id], duration, event)
        end
      end

      # Handle bottleneck detection from Rust orchestration
      def handle_performance_bottleneck_detected(event)
        description = safe_get(event, :description, 'Unknown bottleneck')
        step_ids = safe_get(event, :step_ids, [])

        @logger.warn("Performance bottleneck detected: #{description}")
        @logger.warn("Affected steps: #{step_ids.join(', ')}") if step_ids.any?

        # Could integrate with monitoring systems here
        log_bottleneck(event)
      end

      private

      def log_slow_step(step_id, duration, event_data)
        # Integration point for external monitoring systems
        # Could send to DataDog, NewRelic, etc.
        @logger.debug("Performance metrics - slow step: #{step_id}, duration: #{duration}, threshold: #{@slow_step_threshold}")
      end

      def log_slow_task(task_id, duration, event_data)
        # Integration point for external monitoring systems
        @logger.debug("Performance metrics - slow task: #{task_id}, duration: #{duration}, threshold: #{@slow_task_threshold}")
      end

      def log_bottleneck(event)
        # Integration point for external monitoring systems
        @logger.debug("Performance bottleneck: #{event}")
      end

      def default_logger
        defined?(Rails) ? Rails.logger : Logger.new($stdout)
      end
    end

    # Error tracking subscriber that monitors failure patterns
    # Mirrors Rails engine error tracking patterns
    class ErrorTracker < BaseSubscriber
      subscribe_to 'step.failed', 'task.failed', 'api.request_failed', 'handler.failed'

      def initialize(logger: nil, error_threshold: 10)
        @logger = logger || default_logger
        @error_counts = Hash.new(0)
        @error_threshold = error_threshold
        @window_start = Time.now
      end

      # Handle step failures
      def handle_step_failed(event)
        identifiers = extract_identifiers(event)
        error_info = extract_error_metrics(event)

        error_key = "step_#{identifiers[:step_id]}"
        @error_counts[error_key] += 1

        @logger.error("Step failed: step_id=#{identifiers[:step_id]} error=#{error_info[:error_message]}")

        check_error_threshold(error_key, identifiers)
      end

      # Handle task failures
      def handle_task_failed(event)
        identifiers = extract_identifiers(event)
        error_info = extract_error_metrics(event)

        error_key = "task_#{identifiers[:task_id]}"
        @error_counts[error_key] += 1

        @logger.error("Task failed: task_id=#{identifiers[:task_id]} error=#{error_info[:error_message]}")

        check_error_threshold(error_key, identifiers)
      end

      # Handle API request failures
      def handle_api_request_failed(event)
        url = safe_get(event, :url, 'unknown')
        service = safe_get(event, :service, 'unknown')
        error_info = extract_error_metrics(event)

        api_key = "api_#{url}_#{service}"
        @error_counts[api_key] += 1

        @logger.error("API request failed: #{url} service=#{service} error=#{error_info[:error_message]}")

        check_api_error_threshold(api_key, url, service)
      end

      # Handle handler failures
      def handle_handler_failed(event)
        handler_name = safe_get(event, :handler_name, 'unknown')
        error_info = extract_error_metrics(event)

        @logger.error("Handler failed: #{handler_name} error=#{error_info[:error_message]}")
      end

      # Get error statistics for monitoring
      def error_statistics
        {
          total_errors: @error_counts.values.sum,
          error_counts: @error_counts.dup,
          window_start: @window_start,
          threshold: @error_threshold
        }
      end

      private

      def check_error_threshold(error_key, identifiers)
        if @error_counts[error_key] >= @error_threshold
          @logger.warn("Error threshold exceeded: #{error_key} count=#{@error_counts[error_key]}")

          # Could trigger alerts or circuit breakers here
          # Since Rust is source of truth, we would send feedback to Rust layer
          alert_high_error_rate(error_key, identifiers)
        end
      end

      def check_api_error_threshold(api_key, url, service)
        if @error_counts[api_key] >= @error_threshold
          @logger.warn("API error threshold exceeded: #{url} count=#{@error_counts[api_key]}")

          alert_api_issues(api_key, url, service)
        end
      end

      def alert_high_error_rate(error_key, identifiers)
        # Integration point for alerting systems
        @logger.debug("High error rate alert: #{error_key}, identifiers: #{identifiers}")
      end

      def alert_api_issues(api_key, url, service)
        # Integration point for API monitoring systems
        @logger.debug("API issues alert: #{api_key}, url: #{url}, service: #{service}")
      end

      def default_logger
        defined?(Rails) ? Rails.logger : Logger.new($stdout)
      end
    end

    # ========================================================================
    # CONVENIENCE METHODS AND MODULE API
    # ========================================================================

    # Subscribe to events using Ruby event bus (receives from Rust)
    # This is the primary API for Ruby subscribers
    def self.subscribe(event_type, &block)
      BUS.subscribe(event_type, &block)
    end

    # Get list of events discovered from Rust
    def self.discovered_events
      BUS.discovered_events
    end

    # Get bridge statistics
    def self.bridge_statistics
      @rust_bridge&.statistics || { running: false }
    end

    # Check if bridge is running
    def self.bridge_running?
      @rust_bridge&.running? || false
    end

    # Initialize the event system with Rust integration
    # This is the main setup method called by RailsIntegration
    def self.initialize_subscribers(logger: nil)
      # Initialize built-in subscribers (they auto-register via class definition)
      @performance_monitor ||= PerformanceMonitor.new(logger: logger)
      @error_tracker ||= ErrorTracker.new(logger: logger)

      logger&.info("TaskerCore event subscribers initialized")
    end

    # Create and start Rust event bridge
    # This connects Ruby subscribers to Rust events (source of truth)
    def self.create_rust_bridge(rust_integration)
      @rust_bridge ||= RustEventBridge.new(rust_integration)
      @rust_bridge.start
      @rust_bridge
    end

    # Stop the event system
    def self.shutdown
      @rust_bridge&.stop
      @rust_bridge = nil
    end

    # ========================================================================
    # BACKWARDS COMPATIBILITY (deprecated methods)
    # ========================================================================

    # Legacy method - now delegates to main subscribe method
    # @deprecated Use Events.subscribe instead
    def self.subscribe_orchestration(event_type, &block)
      warn "[DEPRECATED] Events.subscribe_orchestration is deprecated. Use Events.subscribe instead."
      subscribe(event_type, &block)
    end

    # Legacy method - now delegates to main subscribe method
    # @deprecated Use Events.subscribe instead
    def self.subscribe_step_handler(event_type, &block)
      warn "[DEPRECATED] Events.subscribe_step_handler is deprecated. Use Events.subscribe instead."
      subscribe(event_type, &block)
    end

    # Publishing events is no longer supported from Ruby side
    # Events come from Rust orchestration layer
    # @deprecated Events are now published by Rust orchestration layer
    def self.publish_orchestration(event_type, data = {})
      warn "[DEPRECATED] Ruby-side event publishing is deprecated. Events come from Rust orchestration layer."
      # Could implement sending back to Rust if needed for custom events
    end

    # Publishing events is no longer supported from Ruby side
    # Events come from Rust orchestration layer
    # @deprecated Events are now published by Rust orchestration layer
    def self.publish_step_handler(event_type, data = {})
      warn "[DEPRECATED] Ruby-side event publishing is deprecated. Events come from Rust orchestration layer."
      # Could implement sending back to Rust if needed for custom events
    end
  end
end
