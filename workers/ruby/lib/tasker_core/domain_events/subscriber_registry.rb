# frozen_string_literal: true

require 'singleton'

module TaskerCore
  module DomainEvents
    # TAS-65: Registry for domain event subscribers
    #
    # Manages the lifecycle of domain event subscribers. Subscribers are registered
    # at bootstrap time and started/stopped together with the worker.
    #
    # @example Registering subscribers at bootstrap
    #   registry = TaskerCore::DomainEvents::SubscriberRegistry.instance
    #
    #   # Register subscriber classes (instantiation deferred)
    #   registry.register(PaymentEventSubscriber)
    #   registry.register(MetricsSubscriber)
    #
    #   # Or register instances directly
    #   registry.register_instance(CustomSubscriber.new(some_config))
    #
    #   # Start all subscribers
    #   registry.start_all!
    #
    # @example Stopping all subscribers
    #   registry.stop_all!
    #
    class SubscriberRegistry
      include Singleton

      attr_reader :logger, :subscribers

      def initialize
        @logger = TaskerCore::Logger.instance
        @subscribers = []
        @started = false
      end

      # Register a subscriber class
      #
      # The class will be instantiated when start_all! is called.
      #
      # @param subscriber_class [Class] A subclass of BaseSubscriber
      # @return [void]
      def register(subscriber_class)
        unless subscriber_class < BaseSubscriber
          raise ArgumentError, "#{subscriber_class} must be a subclass of BaseSubscriber"
        end

        logger.info "SubscriberRegistry: Registered #{subscriber_class.name}"
        @subscriber_classes ||= []
        @subscriber_classes << subscriber_class
      end

      # Register a subscriber instance directly
      #
      # Use this when your subscriber needs custom initialization.
      #
      # @param subscriber [BaseSubscriber] A subscriber instance
      # @return [void]
      def register_instance(subscriber)
        unless subscriber.is_a?(BaseSubscriber)
          raise ArgumentError, "Expected BaseSubscriber, got #{subscriber.class}"
        end

        logger.info "SubscriberRegistry: Registered instance of #{subscriber.class.name}"
        @subscribers << subscriber
      end

      # Start all registered subscribers
      #
      # Instantiates registered classes and starts all subscribers.
      #
      # @return [void]
      def start_all!
        return if @started

        # Instantiate registered classes
        (@subscriber_classes || []).each do |klass|
          @subscribers << klass.new
        end

        # Start all subscribers
        @subscribers.each do |subscriber|
          begin
            subscriber.start!
          rescue StandardError => e
            logger.error "Failed to start #{subscriber.class.name}: #{e.message}"
          end
        end

        @started = true
        logger.info "SubscriberRegistry: Started #{@subscribers.size} subscribers"
      end

      # Stop all subscribers
      #
      # @return [void]
      def stop_all!
        return unless @started

        @subscribers.each do |subscriber|
          begin
            subscriber.stop!
          rescue StandardError => e
            logger.error "Failed to stop #{subscriber.class.name}: #{e.message}"
          end
        end

        @started = false
        logger.info 'SubscriberRegistry: Stopped all subscribers'
      end

      # Check if subscribers have been started
      #
      # @return [Boolean]
      def started?
        @started
      end

      # Get count of registered subscribers
      #
      # @return [Integer]
      def count
        @subscribers.size + (@subscriber_classes&.size || 0)
      end

      # Get subscriber statistics
      #
      # @return [Hash]
      def stats
        {
          started: @started,
          subscriber_count: @subscribers.size,
          active_count: @subscribers.count(&:active?),
          subscribers: @subscribers.map do |s|
            {
              class: s.class.name,
              active: s.active?,
              patterns: s.class.patterns
            }
          end
        }
      end

      # Reset the registry (for testing)
      #
      # @note This stops all subscribers first
      def reset!
        stop_all! if @started
        @subscribers.clear
        @subscriber_classes&.clear
        @started = false
        logger.info 'SubscriberRegistry: Reset'
      end
    end
  end
end
