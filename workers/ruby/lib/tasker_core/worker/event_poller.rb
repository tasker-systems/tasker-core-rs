# frozen_string_literal: true

module TaskerCore
  module Worker
    # EventPoller polls for step execution events from Rust via FFI
    # This solves the cross-thread communication issue by running in the main Ruby thread
    class EventPoller
      include Singleton

      attr_reader :logger, :active, :polling_thread

      POLL_INTERVAL = 0.01 # 10ms polling interval

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