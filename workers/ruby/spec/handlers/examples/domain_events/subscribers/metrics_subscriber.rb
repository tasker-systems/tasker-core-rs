# frozen_string_literal: true

# TAS-65: Example metrics subscriber for fast/in-process domain events
#
# Demonstrates how to create a subscriber that collects metrics from events.
# Maintains counters for various event types, useful for monitoring and alerting.
#
# @example Register in bootstrap
#   registry = TaskerCore::DomainEvents::SubscriberRegistry.instance
#   registry.register(DomainEvents::Subscribers::MetricsSubscriber)
#   registry.start_all!
#
# @example Query metrics
#   metrics = DomainEvents::Subscribers::MetricsSubscriber
#   puts "Total events: #{metrics.events_received}"
#   puts "Success events: #{metrics.success_events}"
#   puts "Failure events: #{metrics.failure_events}"
#   puts "By namespace: #{metrics.events_by_namespace}"
#
module DomainEvents
  module Subscribers
    class MetricsSubscriber < TaskerCore::DomainEvents::BaseSubscriber
      # Subscribe to all events
      subscribes_to '*'

      class << self
        attr_accessor :events_received, :success_events, :failure_events,
                      :events_by_namespace, :events_by_name, :last_event_at

        # Initialize counters (called at class load)
        def reset_counters!
          @mutex = Mutex.new
          @events_received = 0
          @success_events = 0
          @failure_events = 0
          @events_by_namespace = Hash.new(0)
          @events_by_name = Hash.new(0)
          @last_event_at = nil
        end

        # Thread-safe increment
        def increment(counter_name, by: 1)
          @mutex.synchronize do
            current = instance_variable_get("@#{counter_name}")
            instance_variable_set("@#{counter_name}", current + by)
          end
        end

        # Thread-safe hash increment
        def increment_hash(hash_name, key, by: 1)
          @mutex.synchronize do
            hash = instance_variable_get("@#{hash_name}")
            hash[key] += by
          end
        end

        # Thread-safe setter
        def set(attr_name, value)
          @mutex.synchronize do
            instance_variable_set("@#{attr_name}", value)
          end
        end

        # Get event count for a specific namespace
        def namespace_count(namespace)
          @mutex.synchronize { @events_by_namespace[namespace] }
        end

        # Get event count for a specific event name
        def event_name_count(event_name)
          @mutex.synchronize { @events_by_name[event_name] }
        end

        # Generate a summary report
        def summary
          @mutex.synchronize do
            {
              events_received: @events_received,
              success_events: @success_events,
              failure_events: @failure_events,
              by_namespace: @events_by_namespace.dup,
              by_name: @events_by_name.dup,
              last_event_at: @last_event_at
            }
          end
        end
      end

      # Initialize counters on class load
      reset_counters!

      # Handle any domain event by updating metrics
      #
      # @param event [Hash] The domain event
      def handle(event)
        event_name = event[:event_name]
        metadata = event[:metadata] || {}
        execution_result = event[:execution_result] || {}

        # Increment total count
        self.class.increment(:events_received)

        # Track success/failure
        success = execution_result[:success]
        if success
          self.class.increment(:success_events)
        else
          self.class.increment(:failure_events)
        end

        # Track by namespace
        namespace = metadata[:namespace] || 'unknown'
        self.class.increment_hash(:events_by_namespace, namespace)

        # Track by event name
        self.class.increment_hash(:events_by_name, event_name)

        # Update last event timestamp
        self.class.set(:last_event_at, Time.now)

        logger.debug "MetricsSubscriber: Updated metrics for #{event_name} " \
                     "(total: #{self.class.events_received})"
      end
    end

    # Namespace-filtered metrics subscriber
    #
    # Only counts events from specific namespaces.
    # Useful for per-domain metrics collection.
    #
    # @example Creating namespace-specific metrics
    #   class PaymentMetricsSubscriber < NamespaceMetricsSubscriber
    #     subscribes_to 'payment.*'
    #     track_namespaces 'payments', 'billing'
    #   end
    #
    class NamespaceMetricsSubscriber < TaskerCore::DomainEvents::BaseSubscriber
      class << self
        attr_accessor :tracked_namespaces, :namespace_counts

        def track_namespaces(*namespaces)
          @tracked_namespaces = namespaces.flatten.map(&:to_s)
          @namespace_counts = Hash.new(0)
        end

        def reset!
          @namespace_counts = Hash.new(0)
        end

        def count_for(namespace)
          @namespace_counts[namespace.to_s]
        end

        def total_count
          @namespace_counts.values.sum
        end
      end

      # Default: track all namespaces
      track_namespaces '*'

      def handle(event)
        metadata = event[:metadata] || {}
        namespace = metadata[:namespace]&.to_s

        # Check if this namespace is tracked
        return unless track_namespace?(namespace)

        self.class.namespace_counts[namespace] += 1

        logger.debug "NamespaceMetricsSubscriber: #{namespace} count now " \
                     "#{self.class.namespace_counts[namespace]}"
      end

      private

      def track_namespace?(namespace)
        return true if self.class.tracked_namespaces.include?('*')

        self.class.tracked_namespaces.include?(namespace)
      end
    end
  end
end
