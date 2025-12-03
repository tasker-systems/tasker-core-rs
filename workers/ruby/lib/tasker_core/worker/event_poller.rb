# frozen_string_literal: true

module TaskerCore
  module Worker
    # EventPoller polls for step execution events from Rust via FFI
    #
    # Solves the cross-thread communication issue between Rust and Ruby by
    # actively polling the Rust worker for pending events. Events are forwarded
    # to EventBridge for normal processing.
    #
    # The poller runs in a dedicated background thread, continuously checking
    # for new events from the Rust worker at a configurable interval (default 10ms).
    # This approach allows Ruby to control the thread context and safely process
    # events without running into Ruby's Global Interpreter Lock (GIL) limitations.
    #
    # @example Starting the poller
    #   poller = TaskerCore::Worker::EventPoller.instance
    #   poller.start!
    #   # => Spawns polling thread with 10ms interval
    #
    #   # Check if poller started successfully
    #   if poller.active?
    #     puts "Event poller is running"
    #   end
    #
    # @example Checking poller status
    #   poller.active?
    #   # => true/false
    #
    #   if poller.active? && poller.polling_thread&.alive?
    #     puts "Poller thread is healthy"
    #   end
    #
    # @example Stopping the poller
    #   poller.stop!
    #   # => Gracefully stops polling thread (max 5 second wait)
    #
    #   # Poller will wait for current poll to complete
    #   # and then cleanly shutdown
    #
    # Threading Model:
    # - **Main Thread**: Ruby application, handler execution
    # - **Polling Thread**: Dedicated background thread for event polling
    # - **Rust Threads**: Rust worker runtime (separate from Ruby)
    #
    # The polling thread:
    # 1. Calls `TaskerCore::FFI.poll_step_events` to check for events
    # 2. If event found, forwards to EventBridge for processing
    # 3. If no event, sleeps for POLL_INTERVAL
    # 4. On error, logs and sleeps for 10x POLL_INTERVAL
    #
    # Why Polling is Necessary:
    #
    # Ruby's GIL prevents Rust from directly calling Ruby methods from Rust threads.
    # Direct FFI callbacks from Rust to Ruby would either:
    # - Cause segfaults (unsafe thread access)
    # - Block indefinitely (GIL contention)
    # - Require complex unsafe code
    #
    # Polling allows:
    # - Ruby to control thread context
    # - Safe event processing in Ruby thread
    # - Simple, reliable cross-language communication
    # - Predictable performance characteristics
    #
    # Performance Characteristics:
    # - **Poll Interval**: 10ms (0.01 seconds) - configurable via POLL_INTERVAL
    # - **Max Latency**: ~10ms from event generation to processing start
    # - **CPU Usage**: Minimal (yields during sleep)
    # - **Error Recovery**: Automatic with exponential backoff (10x interval)
    #
    # Event Flow:
    # 1. Rust worker detects step ready for execution
    # 2. Rust queues event in internal event queue
    # 3. EventPoller calls poll_step_events via FFI
    # 4. Rust returns next event (or nil if queue empty)
    # 5. EventPoller forwards event to EventBridge
    # 6. EventBridge publishes to Ruby subscribers
    # 7. StepExecutionSubscriber executes handler
    #
    # @see TaskerCore::Worker::EventBridge For event processing
    # @see TaskerCore::FFI For Rust FFI operations
    # @see POLL_INTERVAL For polling frequency configuration
    class EventPoller
      include Singleton

      attr_reader :logger, :active, :polling_thread

      # Polling interval in seconds (10ms)
      # Lower values reduce latency but increase CPU usage
      # Higher values reduce CPU usage but increase latency
      POLL_INTERVAL = 0.01

      def initialize
        @logger = TaskerCore::Logger.instance
        @active = false
        @polling_thread = nil
      end

      # Start polling for events from Rust
      def start!
        return if @active

        @active = true
        logger.info 'Starting EventPoller - polling for step execution events from Rust'

        @polling_thread = Thread.new do
          poll_events_loop
        end

        logger.info '✅ EventPoller started successfully'
      end

      # Stop polling
      def stop!
        return unless @active

        logger.info 'Stopping EventPoller...'
        @active = false

        if @polling_thread&.alive?
          @polling_thread.join(5.0) # Wait up to 5 seconds for thread to finish
          @polling_thread.kill if @polling_thread.alive? # Force kill if still running
        end

        @polling_thread = nil
        logger.info '✅ EventPoller stopped'
      end

      # Check if poller is active
      def active?
        @active
      end

      private

      # Main polling loop - runs in dedicated thread
      def poll_events_loop
        logger.debug 'EventPoller: Starting poll loop'

        while @active
          begin
            # Poll for next event from Rust via FFI
            event_data = TaskerCore::FFI.poll_step_events

            if event_data.nil?
              # No events available, sleep briefly
              sleep(POLL_INTERVAL)
              next
            end

            # Process the event through EventBridge
            process_event(event_data)
          rescue StandardError => e
            logger.error "EventPoller error: #{e.message}"
            logger.error e.backtrace.join("\n")

            # Sleep longer on error to avoid tight error loops
            sleep(POLL_INTERVAL * 10) if @active
          end
        end

        logger.debug 'EventPoller: Poll loop terminated'
      end

      # Process a polled event through the EventBridge
      def process_event(event_data)
        logger.debug "EventPoller: Processing event from Rust: #{event_data}"

        # Forward to EventBridge for normal processing
        EventBridge.instance.publish_step_execution(event_data)

        logger.debug 'EventPoller: Event forwarded to EventBridge successfully'
      rescue StandardError => e
        logger.error "EventPoller: Failed to process event: #{e.message}"
        logger.error e.backtrace.join("\n")
        raise
      end
    end
  end
end
