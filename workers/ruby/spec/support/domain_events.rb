# frozen_string_literal: true

# TAS-65: Test helpers for domain event system
#
# Provides utilities for testing domain event publishers and subscribers.
#
# @example Using MockEventPublisher
#   describe MyHandler do
#     let(:mock_publisher) { TaskerCore::TestHelpers::MockEventPublisher.new }
#
#     before do
#       TaskerCore::DomainEvents::PublisherRegistry.instance.register(mock_publisher)
#     end
#
#     it 'publishes payment.processed event' do
#       # ... execute handler ...
#       expect(mock_publisher.published_events).to include(
#         have_attributes(event_name: 'payment.processed')
#       )
#     end
#   end
#
# @example Using EventCapture
#   describe 'event flow' do
#     let(:capture) { TaskerCore::TestHelpers::EventCapture.new }
#
#     before do
#       capture.subscribe_to('payment.*')
#     end
#
#     it 'captures events matching pattern' do
#       # ... trigger events ...
#       expect(capture.events.size).to eq(2)
#       expect(capture.events_by_name('payment.processed')).to have(1).item
#     end
#   end

module TaskerCore
  module TestHelpers
    # Mock publisher for testing domain event publication
    #
    # Captures all events published through it for later assertions.
    # Can be configured to simulate failures.
    #
    # TAS-96: Updated to support both old direct-call pattern and new publish(ctx) pattern.
    class MockEventPublisher < DomainEvents::BasePublisher
      attr_reader :published_events, :should_fail, :failure_error

      def initialize(name: 'MockEventPublisher')
        super()
        @publisher_name = name
        @published_events = []
        @should_fail = false
        @failure_error = nil
      end

      def name
        @publisher_name
      end

      # Configure the publisher to fail
      def simulate_failure!(error = StandardError.new('Simulated failure'))
        @should_fail = true
        @failure_error = error
      end

      # Clear simulated failure
      def clear_failure!
        @should_fail = false
        @failure_error = nil
      end

      # Clear all captured events
      def clear!
        @published_events.clear
      end

      # Get events by name
      def events_by_name(name)
        @published_events.select { |e| e[:event_name] == name }
      end

      # Get events matching a pattern (supports wildcards)
      def events_matching(pattern)
        regex = pattern.gsub('.', '\\.').gsub('*', '.*')
        @published_events.select { |e| e[:event_name].match?(/^#{regex}$/) }
      end

      # Override transform_payload to capture the full event
      def transform_payload(step_result, _event_declaration, _step_context = nil)
        step_result[:result] || {}
      end

      # TAS-96: Updated signature to match BasePublisher (event_name, payload, metadata)
      # This captures events when using the new publish(ctx) flow
      def before_publish(event_name, payload, metadata)
        raise @failure_error if @should_fail

        @published_events << {
          event_name: event_name,
          payload: payload,
          metadata: metadata,
          published_at: Time.now
        }
      end

      # Legacy capture method for direct testing
      # @deprecated Use publish(ctx) flow instead
      def capture_event(step_result, event_declaration, step_context = nil)
        raise @failure_error if @should_fail

        @published_events << {
          event_name: event_declaration[:name],
          delivery_mode: event_declaration[:delivery_mode],
          step_result: step_result,
          step_context: step_context,
          published_at: Time.now
        }

        true
      end
    end

    # Event capture utility for testing subscriber behavior
    #
    # Subscribes to patterns and captures all received events.
    class EventCapture < DomainEvents::BaseSubscriber
      attr_reader :events

      def initialize
        super
        @events = []
        @mutex = Mutex.new
      end

      # Subscribe to a pattern
      def subscribe_to(pattern)
        @patterns ||= []
        @patterns << pattern
        self.class.subscribes_to(*@patterns)
        start! unless active?
      end

      # Handle captured events
      def handle(event)
        @mutex.synchronize do
          @events << event.merge(captured_at: Time.now)
        end
      end

      # Clear captured events
      def clear!
        @mutex.synchronize { @events.clear }
      end

      # Get events by name
      def events_by_name(name)
        @mutex.synchronize do
          @events.select { |e| e[:event_name] == name }
        end
      end

      # Wait for events (useful for async testing)
      def wait_for_events(count:, timeout: 5)
        deadline = Time.now + timeout
        loop do
          return true if @events.size >= count
          return false if Time.now > deadline

          sleep 0.05
        end
      end
    end

    # Factory for creating test events
    module EventFactory
      extend self

      # Create a minimal step execution result
      def step_result(success: true, result: {}, step_uuid: nil)
        {
          step_uuid: step_uuid || SecureRandom.uuid,
          success: success,
          result: result,
          status: success ? 'completed' : 'error',
          error: success ? nil : { message: 'Test error' },
          metadata: {
            execution_time_ms: rand(10..500),
            handler_version: '1.0',
            retryable: true,
            completed_at: Time.now.iso8601
          }
        }
      end

      # Create an event declaration (mimics YAML configuration)
      def event_declaration(name:, delivery_mode: 'fast', publisher: nil)
        decl = {
          name: name,
          delivery_mode: delivery_mode.to_s
        }
        decl[:publisher] = publisher if publisher
        decl
      end

      # Create a minimal step context
      def step_context(
        task_uuid: nil,
        step_uuid: nil,
        step_name: 'test_step',
        namespace: 'test'
      )
        {
          task_uuid: task_uuid || SecureRandom.uuid,
          step_uuid: step_uuid || SecureRandom.uuid,
          step_name: step_name,
          namespace: namespace,
          correlation_id: SecureRandom.uuid,
          workflow_step: {
            workflow_step_uuid: step_uuid || SecureRandom.uuid,
            name: step_name,
            retryable: true
          },
          task: {
            task_uuid: task_uuid || SecureRandom.uuid,
            namespace_name: namespace
          }
        }
      end

      # Create a complete domain event
      def domain_event(
        event_name:,
        business_payload: {},
        namespace: 'test',
        task_uuid: nil,
        step_uuid: nil
      )
        {
          event_name: event_name,
          business_payload: business_payload,
          metadata: {
            task_uuid: task_uuid || SecureRandom.uuid,
            step_uuid: step_uuid || SecureRandom.uuid,
            namespace: namespace,
            correlation_id: SecureRandom.uuid,
            fired_at: Time.now.iso8601,
            fired_by: 'test'
          }
        }
      end
    end
  end
end

# RSpec integration if RSpec is available
if defined?(RSpec)
  RSpec.configure do |config|
    # Include test helpers in all specs
    config.include TaskerCore::TestHelpers

    # Reset domain event registries before each test
    config.before do
      if defined?(TaskerCore::DomainEvents::PublisherRegistry)
        TaskerCore::DomainEvents::PublisherRegistry.instance.reset!
      end
      if defined?(TaskerCore::DomainEvents::SubscriberRegistry)
        TaskerCore::DomainEvents::SubscriberRegistry.instance.reset!
      end
    end
  end
end
