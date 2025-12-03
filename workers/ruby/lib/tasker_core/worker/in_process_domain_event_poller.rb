# frozen_string_literal: true

require 'json'

module TaskerCore
  module Worker
    # TAS-65 Phase 4.2: In-Process Domain Event Poller
    #
    # Polls for fast domain events (delivery_mode: fast) from the Rust in-process
    # event bus via FFI. Ruby handlers can subscribe to specific event patterns
    # for integration purposes (Sentry, DataDog, Slack, etc.).
    #
    # This follows the same polling pattern as EventPoller but for domain events
    # rather than step execution events. Events flow through a broadcast channel
    # from Rust to Ruby.
    #
    # Event Flow:
    # 1. Step handler (Rust or Ruby) publishes domain event with delivery_mode: fast
    # 2. EventRouter routes to InProcessEventBus
    # 3. InProcessEventBus broadcasts to all subscribers (including FFI channel)
    # 4. This poller calls poll_in_process_events via FFI
    # 5. Events are dispatched to registered Ruby handlers based on pattern matching
    #
    # @example Starting the poller
    #   poller = TaskerCore::Worker::InProcessDomainEventPoller.instance
    #   poller.start!
    #
    # @example Subscribing to specific event patterns
    #   poller.subscribe('payment.*') do |event|
    #     puts "Payment event: #{event[:event_name]}"
    #     # Forward to Sentry, DataDog, etc.
    #   end
    #
    # @example Subscribing to all events
    #   poller.subscribe('*') do |event|
    #     MyMetricsCollector.record(event)
    #   end
    #
    # @example Stopping the poller
    #   poller.stop!
    class InProcessDomainEventPoller
      include Singleton

      attr_reader :logger, :active, :polling_thread

      # Polling interval in seconds (50ms - longer than step events since these are async notifications)
      POLL_INTERVAL = 0.05

      # Maximum events to poll per iteration
      MAX_EVENTS_PER_POLL = 10

      def initialize
        @logger = TaskerCore::Logger.instance
        @active = false
        @polling_thread = nil
        @subscribers = {} # pattern => array of handlers
        @mutex = Mutex.new
      end

      # Subscribe a handler to an event pattern
      #
      # @param pattern [String] Event pattern (exact, wildcard 'payment.*', or global '*')
      # @yield [event] Block called when matching event is received
      # @yieldparam event [Hash] Domain event data
      # @return [void]
      #
      # @example
      #   poller.subscribe('payment.processed') { |event| puts event[:event_name] }
      #   poller.subscribe('order.*') { |event| notify_slack(event) }
      #   poller.subscribe('*') { |event| log_all_events(event) }
      def subscribe(pattern, &handler)
        raise ArgumentError, 'Block required for subscription' unless block_given?
        raise ArgumentError, 'Pattern cannot be empty' if pattern.nil? || pattern.empty?

        @mutex.synchronize do
          @subscribers[pattern] ||= []
          @subscribers[pattern] << handler
        end

        logger.info "InProcessDomainEventPoller: Subscribed to pattern '#{pattern}'"
      end

      # Unsubscribe all handlers for a pattern
      #
      # @param pattern [String] Event pattern to unsubscribe
      # @return [void]
      def unsubscribe(pattern)
        @mutex.synchronize do
          @subscribers.delete(pattern)
        end

        logger.info "InProcessDomainEventPoller: Unsubscribed from pattern '#{pattern}'"
      end

      # Clear all subscriptions
      def clear_subscriptions!
        @mutex.synchronize do
          @subscribers.clear
        end

        logger.info 'InProcessDomainEventPoller: Cleared all subscriptions'
      end

      # Get count of registered subscribers
      def subscriber_count
        @mutex.synchronize do
          @subscribers.values.flatten.size
        end
      end

      # Start polling for in-process domain events
      def start!
        return if @active

        @active = true
        logger.info 'Starting InProcessDomainEventPoller - polling for fast domain events from Rust'

        @polling_thread = Thread.new do
          poll_events_loop
        end

        logger.info '✅ InProcessDomainEventPoller started successfully'
      end

      # Stop polling
      def stop!
        return unless @active

        logger.info 'Stopping InProcessDomainEventPoller...'
        @active = false

        if @polling_thread&.alive?
          @polling_thread.join(5.0) # Wait up to 5 seconds for thread to finish
          @polling_thread.kill if @polling_thread.alive? # Force kill if still running
        end

        @polling_thread = nil
        logger.info '✅ InProcessDomainEventPoller stopped'
      end

      # Check if poller is active
      def active?
        @active
      end

      # Get statistics about the poller
      def stats
        ffi_stats = begin
          TaskerCore::FFI.in_process_event_stats
        rescue StandardError => e
          logger.warn "Failed to get FFI stats: #{e.message}"
          { enabled: false, status: 'unknown' }
        end

        {
          active: @active,
          subscriber_count: subscriber_count,
          patterns: @subscribers.keys,
          ffi: ffi_stats
        }
      end

      private

      # Main polling loop - runs in dedicated thread
      def poll_events_loop
        logger.debug 'InProcessDomainEventPoller: Starting poll loop'

        while @active
          begin
            # Poll for events from Rust via FFI
            events = TaskerCore::FFI.poll_in_process_events(MAX_EVENTS_PER_POLL)

            if events.nil? || events.empty?
              # No events available, sleep briefly
              sleep(POLL_INTERVAL)
              next
            end

            # Process each event through pattern matching
            events.each do |event_data|
              dispatch_event(event_data)
            end
          rescue StandardError => e
            logger.error "InProcessDomainEventPoller error: #{e.message}"
            logger.error e.backtrace.first(5).join("\n")

            # Sleep longer on error to avoid tight error loops
            sleep(POLL_INTERVAL * 10) if @active
          end
        end

        logger.debug 'InProcessDomainEventPoller: Poll loop terminated'
      end

      # Dispatch event to matching subscribers
      def dispatch_event(event_data)
        event_name = event_data[:event_name] || event_data['event_name']
        return unless event_name

        logger.debug "InProcessDomainEventPoller: Dispatching event '#{event_name}'"

        # Parse business_payload if it's a JSON string
        if event_data[:business_payload].is_a?(String)
          begin
            event_data[:business_payload] = JSON.parse(event_data[:business_payload])
          rescue JSON::ParserError => e
            logger.warn "Failed to parse business_payload JSON: #{e.message}"
          end
        end

        # Parse error if it's a JSON string
        if event_data.dig(:execution_result, :error).is_a?(String)
          begin
            event_data[:execution_result][:error] = JSON.parse(event_data[:execution_result][:error])
          rescue JSON::ParserError => e
            logger.warn "Failed to parse error JSON: #{e.message}"
          end
        end

        # Find and invoke matching handlers
        handlers = find_matching_handlers(event_name)

        if handlers.empty?
          logger.debug "InProcessDomainEventPoller: No handlers for event '#{event_name}'"
          return
        end

        handlers.each do |handler|
          invoke_handler_safely(handler, event_data, event_name)
        end
      end

      # Find handlers that match the event name
      def find_matching_handlers(event_name)
        handlers = []

        @mutex.synchronize do
          @subscribers.each do |pattern, pattern_handlers|
            handlers.concat(pattern_handlers) if matches_pattern?(event_name, pattern)
          end
        end

        handlers
      end

      # Check if event name matches pattern
      def matches_pattern?(event_name, pattern)
        return true if pattern == '*' # Global wildcard

        if pattern.end_with?('.*')
          # Wildcard pattern: 'payment.*' matches 'payment.processed', 'payment.failed', etc.
          prefix = pattern[0..-3] # Remove '.*'
          event_name.start_with?("#{prefix}.")
        else
          # Exact match
          event_name == pattern
        end
      end

      # Safely invoke a handler (fire-and-forget semantics)
      def invoke_handler_safely(handler, event_data, event_name)
        handler.call(event_data)
      rescue StandardError => e
        # Fire-and-forget: log errors but don't propagate
        logger.warn "InProcessDomainEventPoller: Handler failed for '#{event_name}': #{e.message}"
        logger.warn e.backtrace.first(3).join("\n")
      end
    end
  end
end
