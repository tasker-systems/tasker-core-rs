# frozen_string_literal: true

require 'dry-events'
require 'singleton'
require 'logger'
require_relative '../constants'
require_relative 'concerns/event_based_transitions'

module TaskerCore
  module Events
    # Core event infrastructure for the TaskerCore system
    #
    # This publisher provides standalone event infrastructure using dry-events,
    # mirroring the functionality of the Tasker Rails engine Publisher but
    # operating independently within the TaskerCore Ruby bindings.
    #
    # Events are statically defined in TaskerCore::Constants and registered here.
    # State machine mappings and metadata are loaded from the constants.
    #
    # ## Usage
    #
    #   # Publishing events
    #   TaskerCore::Events::Publisher.instance.publish('task.completed', {
    #     task_id: '123',
    #     execution_duration: 5.2
    #   })
    #
    #   # Subscribing to events
    #   publisher = TaskerCore::Events::Publisher.instance
    #   publisher.subscribe('task.completed') do |event|
    #     puts "Task completed: #{event[:task_id]}"
    #   end
    #
    # ## Integration with Rust
    #
    # This publisher is designed to work seamlessly with the Rust FFI layer,
    # allowing Rust orchestration events to be published through the same
    # system that Rails handlers use.
    class Publisher
      include Dry::Events::Publisher[:tasker_core]
      include TaskerCore::Events::Concerns::EventBasedTransitions
      include Singleton

      attr_reader :logger

      def initialize
        # Register all static events from constants
        @logger = TaskerCore::Logging::Logger.new
        register_static_events
      end

      # Core publish method with automatic timestamp enhancement
      #
      # This is the primary method used for publishing events. It ensures
      # that all events have a timestamp and provides a consistent interface
      # for both Ruby and Rust event sources.
      #
      # @param event_name [String] The event name
      # @param payload [Hash] The event payload
      # @return [void]
      def publish(event_name, payload = {})
        # Ensure timestamp is always present in the payload
        enhanced_payload = ensure_timestamp(payload)

        logger.debug { "Publishing event: #{event_name} with payload keys: #{enhanced_payload.keys}" }

        # Call the parent publish method from dry-events
        super(event_name, enhanced_payload)
      rescue StandardError => e
        logger.error { "Failed to publish event #{event_name}: #{e.message}" }
        raise
      end

      # Request a task transition through Rust orchestration
      #
      # This sends a transition request to Rust, which will validate and publish
      # the appropriate success or error events back to Ruby.
      #
      # @param task_id [String, Integer] Task identifier
      # @param from_state [String, nil] Current task state
      # @param to_state [String] Target task state
      # @param payload [Hash] Additional context for the transition
      # @return [Boolean] True if request was sent
      def publish_task_transition(task_id, from_state, to_state, payload = {})
        request_transition('task', task_id.to_s, from_state, to_state, payload)
      end

      # Request a step transition through Rust orchestration
      #
      # This sends a transition request to Rust, which will validate and publish
      # the appropriate success or error events back to Ruby.
      #
      # @param step_id [String, Integer] Step identifier
      # @param task_id [String, Integer] Parent task identifier
      # @param from_state [String, nil] Current step state
      # @param to_state [String] Target step state
      # @param payload [Hash] Additional context for the transition
      # @return [Boolean] True if request was sent
      def publish_step_transition(step_id, task_id, from_state, to_state, payload = {})
        context = payload.merge(task_id: task_id.to_s)
        request_transition('step', step_id.to_s, from_state, to_state, context)
      end

      # Get the number of registered events
      #
      # @return [Integer] Count of registered events
      def registered_event_count
        @registered_event_count ||= 0
      end

      # Get list of all registered event names
      #
      # @return [Array<String>] List of registered event names
      def registered_events
        @registered_events ||= []
      end

      # Check if an event is registered
      #
      # @param event_name [String] Event name to check
      # @return [Boolean] True if event is registered
      def event_registered?(event_name)
        registered_events.include?(event_name)
      end

      private

      # Register all events from static constants
      #
      # This mirrors the Rails engine approach but uses TaskerCore constants
      def register_static_events
        logger.info('TaskerCore: Registering events from static constants')

        event_count = 0
        @registered_events = []

        # Register Task Events
        TaskerCore::Constants::TaskEvents.constants.each do |const_name|
          event_constant = TaskerCore::Constants::TaskEvents.const_get(const_name)
          register_event(event_constant)
          @registered_events << event_constant
          event_count += 1
        end

        # Register Step Events
        TaskerCore::Constants::StepEvents.constants.each do |const_name|
          event_constant = TaskerCore::Constants::StepEvents.const_get(const_name)
          register_event(event_constant)
          @registered_events << event_constant
          event_count += 1
        end

        # Register Workflow Events
        TaskerCore::Constants::WorkflowEvents.constants.each do |const_name|
          event_constant = TaskerCore::Constants::WorkflowEvents.const_get(const_name)
          register_event(event_constant)
          @registered_events << event_constant
          event_count += 1
        end

        # Register Observability Events
        register_observability_events
        event_count += count_observability_events

        # Register Test Events (for testing environments)
        register_test_events
        event_count += count_test_events

        @registered_event_count = event_count
        logger.info("TaskerCore: Successfully registered #{event_count} events from static constants")
      end

      # Register nested observability events
      def register_observability_events
        # Task observability events
        TaskerCore::Constants::ObservabilityEvents::Task.constants.each do |const_name|
          event_constant = TaskerCore::Constants::ObservabilityEvents::Task.const_get(const_name)
          register_event(event_constant)
          @registered_events << event_constant
        end

        # Step observability events
        TaskerCore::Constants::ObservabilityEvents::Step.constants.each do |const_name|
          event_constant = TaskerCore::Constants::ObservabilityEvents::Step.const_get(const_name)
          register_event(event_constant)
          @registered_events << event_constant
        end
      end

      # Register test events for testing environments
      def register_test_events
        TaskerCore::Constants::TestEvents.constants.each do |const_name|
          event_constant = TaskerCore::Constants::TestEvents.const_get(const_name)
          register_event(event_constant)
          @registered_events << event_constant
        end
      end

      # Count observability events for logging
      def count_observability_events
        task_count = TaskerCore::Constants::ObservabilityEvents::Task.constants.size
        step_count = TaskerCore::Constants::ObservabilityEvents::Step.constants.size
        task_count + step_count
      end

      # Count test events for logging
      def count_test_events
        TaskerCore::Constants::TestEvents.constants.size
      end

      # Ensure payload has a timestamp
      #
      # @param payload [Hash] Original payload
      # @return [Hash] Payload with timestamp
      def ensure_timestamp(payload)
        enhanced_payload = payload.dup

        # Add timestamp if not present
        unless enhanced_payload.key?(:timestamp) || enhanced_payload.key?('timestamp')
          enhanced_payload[:timestamp] = Time.now
        end

        enhanced_payload
      end
    end
  end
end
