# frozen_string_literal: true

module TaskerCore
  module DomainEvents
    # TAS-65: Base class for domain event subscribers
    #
    # Domain event subscribers handle business events published by steps.
    # They subscribe to event patterns and receive events via the in-process
    # event bus (for fast events) or can poll PGMQ (for durable events).
    #
    # @example Creating a subscriber for payment events
    #   class PaymentEventSubscriber < TaskerCore::DomainEvents::BaseSubscriber
    #     # Match all payment.* events
    #     subscribes_to 'payment.*'
    #
    #     def handle(event)
    #       case event[:event_name]
    #       when 'payment.processed'
    #         notify_accounting(event[:business_payload])
    #       when 'payment.failed'
    #         alert_support(event[:business_payload])
    #       end
    #     end
    #
    #     private
    #
    #     def notify_accounting(payload)
    #       # Send to accounting system
    #     end
    #
    #     def alert_support(payload)
    #       # Alert support team
    #     end
    #   end
    #
    # @example Creating a subscriber for metrics collection
    #   class MetricsSubscriber < TaskerCore::DomainEvents::BaseSubscriber
    #     # Match all events
    #     subscribes_to '*'
    #
    #     def handle(event)
    #       StatsD.increment("domain_events.#{event[:event_name].gsub('.', '_')}")
    #     end
    #   end
    #
    # @example Registering and starting subscribers
    #   # In bootstrap
    #   subscribers = [
    #     PaymentEventSubscriber.new,
    #     MetricsSubscriber.new
    #   ]
    #
    #   subscribers.each(&:start!)
    #
    class BaseSubscriber
      class << self
        # DSL method to declare event pattern subscriptions
        #
        # @param patterns [Array<String>] Event patterns to subscribe to
        def subscribes_to(*patterns)
          @patterns = patterns.flatten
        end

        # Get declared patterns
        #
        # @return [Array<String>]
        def patterns
          @patterns || ['*']
        end
      end

      attr_reader :logger, :active

      def initialize
        @logger = TaskerCore::Logger.instance
        @active = false
        @subscriptions = []
      end

      # Start listening for events
      def start!
        return if @active

        @active = true
        poller = TaskerCore::Worker::InProcessDomainEventPoller.instance

        self.class.patterns.each do |pattern|
          # Subscribe to the poller with this subscriber's handler
          poller.subscribe(pattern) do |event|
            handle_event_safely(event)
          end
          @subscriptions << pattern
          logger.info "#{self.class.name}: Subscribed to pattern '#{pattern}'"
        end

        logger.info "#{self.class.name}: Started with #{@subscriptions.size} subscriptions"
      end

      # Stop listening for events
      def stop!
        return unless @active

        @active = false
        poller = TaskerCore::Worker::InProcessDomainEventPoller.instance

        @subscriptions.each do |pattern|
          poller.unsubscribe(pattern)
        end
        @subscriptions.clear

        logger.info "#{self.class.name}: Stopped"
      end

      # Check if subscriber is active
      def active?
        @active
      end

      # Handle a domain event
      #
      # Subclasses MUST implement this method.
      #
      # @param event [Hash] The domain event
      #   - :event_name [String] The event name (e.g., "order.processed")
      #   - :business_payload [Hash] The business data from the step
      #   - :metadata [Hash] Event metadata (task_uuid, step_uuid, correlation_id, etc.)
      #   - :execution_result [Hash] The step execution result
      def handle(event)
        raise NotImplementedError, "#{self.class} must implement #handle"
      end

      # Hook called before handling an event
      #
      # Override for pre-processing, validation, or filtering.
      # Return false to skip handling this event.
      #
      # @param event [Hash] The domain event
      # @return [Boolean] true to continue handling, false to skip
      def before_handle(event) # rubocop:disable Lint/UnusedMethodArgument,Naming/PredicateMethod
        true
      end

      # Hook called after successful handling
      #
      # Override for post-processing, metrics, or cleanup.
      #
      # @param event [Hash] The domain event
      def after_handle(event)
        # Default: no-op
      end

      # Hook called if handling fails
      #
      # Override for custom error handling, alerts, or retry logic.
      # Note: Domain event handling uses fire-and-forget semantics - errors
      # are logged but not propagated.
      #
      # @param event [Hash] The domain event
      # @param error [Exception] The error that occurred
      def on_handle_error(event, error)
        logger.error "#{self.class.name}: Failed to handle #{event[:event_name]}: #{error.message}"
      end

      private

      # Safely handle an event with error capture
      def handle_event_safely(event)
        return unless @active
        return unless before_handle(event)

        handle(event)
        after_handle(event)
      rescue StandardError => e
        on_handle_error(event, e)
      end
    end
  end
end
