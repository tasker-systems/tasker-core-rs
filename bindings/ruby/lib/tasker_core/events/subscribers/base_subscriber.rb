# frozen_string_literal: true

module TaskerCore
  module Events
    module Subscribers
      # BaseSubscriber provides a clean foundation for creating custom event subscribers
      #
      # This class mirrors the Rails engine BaseSubscriber patterns and provides:
      # - Declarative subscription registration via class methods
      # - Automatic method routing from event names to handler methods
      # - Defensive payload handling with safe accessors
      # - Easy integration with the TaskerCore event system
      # - Metrics collection helper methods for common patterns
      #
      # ## Usage
      #
      #   class OrderNotificationSubscriber < TaskerCore::Events::Subscribers::BaseSubscriber
      #     subscribe_to 'task.completed', 'step.failed'
      #
      #     def handle_task_completed(event)
      #       OrderMailer.completion_email(event[:task_id]).deliver_later
      #     end
      #
      #     def handle_step_failed(event)
      #       AlertService.notify("Step failed: #{event[:step_name]}")
      #     end
      #   end
      #
      #   # Register the subscriber
      #   OrderNotificationSubscriber.subscribe(TaskerCore::Events::Publisher.instance)
      class BaseSubscriber
        class << self
          # Get or initialize the list of events this class subscribes to
          def subscribed_events
            @subscribed_events ||= []
          end

          # Get or set the event filter for this class
          def event_filter(filter_proc = nil)
            if filter_proc
              @event_filter = filter_proc
            else
              @event_filter
            end
          end

          # Declarative method to register events this subscriber cares about
          #
          # @param events [Array<Symbol, String>] Event names to subscribe to
          # @return [void]
          #
          # Example:
          #   subscribe_to 'task.completed', 'step.failed'
          #   subscribe_to 'order.created', 'payment.processed'
          def subscribe_to(*events)
            # Accumulate events instead of replacing them
            @subscribed_events = (subscribed_events + events.map(&:to_s)).uniq
          end

          # Set a filter for events (optional)
          #
          # @param filter_proc [Proc] A proc that returns true for events to process
          # @return [void]
          #
          # Example:
          #   filter_events { |event_name| event_name.include?('order') }
          def filter_events(&filter_proc)
            @event_filter = filter_proc
          end

          # Subscribe this subscriber to a publisher
          #
          # @param publisher [TaskerCore::Events::Publisher] The event publisher
          # @return [BaseSubscriber] The subscriber instance
          def subscribe(publisher)
            subscriber = new
            subscriber.subscribe_to_publisher(publisher)
            subscriber
          end
        end

        def initialize(name: nil, events: nil, config: {})
          @subscription_name = name
          @subscription_config = config
          @logger = TaskerCore::Logging::Logger.new

          # If events are provided via constructor (from YAML), add them to subscribed events
          return if events.blank?

          current_events = self.class.subscribed_events || []
          self.class.instance_variable_set(:@subscribed_events, (current_events + Array(events)).uniq)
        end

        # Subscribe to all events defined by the class
        #
        # @param publisher [TaskerCore::Events::Publisher] The event publisher
        # @return [void]
        def subscribe_to_publisher(publisher)
          event_subscriptions.each do |event_constant, handler_method|
            # Apply filtering if defined
            next unless should_handle_event?(event_constant)

            # Subscribe to the event with automatic method routing
            # This will fail fast if the event doesn't exist, which is the correct behavior
            publisher.subscribe(event_constant) do |event|
              send(handler_method, event)
            rescue StandardError => e
              handle_subscriber_error(event_constant, event, e)
            end

            @logger.debug { "#{self.class.name}: Subscribed to #{event_constant} -> #{handler_method}" }
          end
        end

        protected

        # Check if an event is a custom event (not a system constant)
        #
        # @param event_constant [String] The event constant or name
        # @return [Boolean] Whether it's a custom event
        def custom_event?(event_constant)
          event_str = event_constant.to_s
          event_str.exclude?('TaskerCore::Constants::') && event_str.include?('.')
        end

        # Get event subscriptions mapping for this subscriber
        #
        # This method maps event constants to handler methods using naming conventions.
        # Override this method to customize the mapping.
        #
        # @return [Hash] Mapping of event constants to handler method symbols
        def event_subscriptions
          @event_subscriptions ||= build_event_subscriptions
        end

        # Build event subscriptions using explicit constants
        #
        # @return [Hash] Mapping of event constants to handler method symbols
        def build_event_subscriptions
          subscriptions = {}

          (self.class.subscribed_events || []).each do |event_name|
            # Convert developer-friendly name to internal constant
            internal_constant = resolve_internal_event_constant(event_name)

            # Generate handler method name: order.processed -> handle_order_processed
            handler_method = generate_handler_method_name(event_name)

            # Only add if the handler method exists
            if respond_to?(handler_method, true)
              subscriptions[internal_constant] = handler_method
            else
              @logger.warn("#{self.class.name}: Handler method #{handler_method} not found for event #{event_name}")
            end
          end

          subscriptions
        end

        # Resolve developer-friendly event name to internal constant
        #
        # This handles the transparent namespace mapping:
        # - "order.processed" -> "custom.order.processed" (for custom events)
        # - "task.completed" -> "task.completed" (for system events)
        #
        # @param event_name [String] The developer-friendly event name
        # @return [String] The internal event constant
        def resolve_internal_event_constant(event_name)
          event_str = event_name.to_s

          # Check if it's already an internal constant (starts with system prefixes)
          system_prefixes = %w[task. step. workflow. observability.]
          if system_prefixes.any? { |prefix| event_str.start_with?(prefix) }
            return event_str # It's a system event, use as-is
          end

          # Check if it's already prefixed with custom.
          if event_str.start_with?('custom.')
            return event_str # Already internal format
          end

          # Assume it's a custom event and add the prefix
          "custom.#{event_str}"
        end

        # Generate handler method name from event name
        #
        # @param event_name [String] The event name (should be consistent format)
        # @return [Symbol] The handler method name
        def generate_handler_method_name(event_name)
          # Convert dots to underscores and prefix with handle_
          # Examples:
          #   'task.completed' -> :handle_task_completed
          #   'step.failed' -> :handle_step_failed
          #   'custom.event' -> :handle_custom_event
          clean_name = event_name.to_s.tr('.', '_').underscore
          :"handle_#{clean_name}"
        end

        # Check if this subscriber should handle the given event
        #
        # @param event_constant [String] The event constant
        # @return [Boolean] Whether to handle this event
        def should_handle_event?(event_constant)
          # Apply class-level filter if defined
          return false if self.class.event_filter && !self.class.event_filter.call(event_constant)

          # Apply instance-level filtering (override in subclasses)
          should_process_event?(event_constant)
        end

        # Instance-level event filtering (override in subclasses)
        #
        # @param _event_constant [String] The event constant (unused in base implementation)
        # @return [Boolean] Whether to handle this event
        def should_process_event?(_event_constant)
          true # Process all events by default
        end

        # Safe accessor for event payload keys with fallback values
        #
        # @param event [Hash, Dry::Events::Event] The event payload or event object
        # @param key [Symbol, String] The key to access
        # @param default [Object] The default value if key is missing
        # @return [Object] The value or default
        def safe_get(event, key, default = nil)
          return default if event.nil?

          # Handle Dry::Events::Event objects
          if event.respond_to?(:payload)
            payload = event.payload
            return payload.fetch(key.to_sym) { payload.fetch(key.to_s, default) }
          end

          # Handle plain hash events
          event.fetch(key.to_sym) do
            event.fetch(key.to_s, default)
          end
        end

        # Extract core attributes common to most events
        #
        # @param event [Hash, Dry::Events::Event] The event payload or event object
        # @return [Hash] Core attributes
        def extract_core_attributes(event)
          {
            task_id: safe_get(event, :task_id, 'unknown'),
            step_id: safe_get(event, :step_id),
            event_timestamp: safe_get(event, :timestamp, Time.current).to_s
          }.compact
        end

        # Extract step-specific attributes
        #
        # @param event [Hash, Dry::Events::Event] The event payload or event object
        # @return [Hash] Step attributes
        def extract_step_attributes(event)
          extract_core_attributes(event).merge(
            step_id: safe_get(event, :step_id),
            step_name: safe_get(event, :step_name, 'unknown_step')
          ).compact
        end

        # ===============================
        # METRICS COLLECTION HELPERS
        # ===============================
        # These methods make it easy to extract common metrics data from events

        # Extract timing metrics from completion events
        #
        # @param event [Hash, Dry::Events::Event] The event payload
        # @return [Hash] Timing metrics with default values
        #
        # Example:
        #   timing = extract_timing_metrics(event)
        #   StatsD.histogram('tasker.task.duration', timing[:execution_duration])
        #   StatsD.gauge('tasker.task.step_count', timing[:step_count])
        def extract_timing_metrics(event)
          {
            execution_duration: safe_get(event, :execution_duration, 0.0).to_f,
            started_at: safe_get(event, :started_at),
            completed_at: safe_get(event, :completed_at),
            step_count: safe_get(event, :total_steps, 0).to_i,
            completed_steps: safe_get(event, :completed_steps, 0).to_i,
            failed_steps: safe_get(event, :failed_steps, 0).to_i
          }
        end

        # Extract error metrics from failure events
        #
        # @param event [Hash, Dry::Events::Event] The event payload
        # @return [Hash] Error metrics with categorization
        #
        # Example:
        #   error = extract_error_metrics(event)
        #   StatsD.increment('tasker.errors', tags: ["error_type:#{error[:error_type]}"])
        def extract_error_metrics(event)
          {
            error_message: safe_get(event, :error_message, 'unknown_error'),
            error_class: safe_get(event, :exception_class, 'UnknownError'),
            error_type: categorize_error(safe_get(event, :exception_class)),
            attempt_number: safe_get(event, :attempt_number, 1).to_i,
            is_retryable: safe_get(event, :retryable, false),
            final_failure: safe_get(event, :attempt_number, 1).to_i >= safe_get(event, :retry_limit, 1).to_i
          }
        end

        # Extract performance metrics for operational monitoring
        #
        # @param event [Hash, Dry::Events::Event] The event payload
        # @return [Hash] Performance metrics
        #
        # Example:
        #   perf = extract_performance_metrics(event)
        #   StatsD.histogram('tasker.memory_usage', perf[:memory_usage])
        def extract_performance_metrics(event)
          {
            memory_usage: safe_get(event, :memory_usage, 0).to_i,
            cpu_time: safe_get(event, :cpu_time, 0.0).to_f,
            queue_time: safe_get(event, :queue_time, 0.0).to_f,
            processing_time: safe_get(event, :processing_time, 0.0).to_f,
            retry_delay: safe_get(event, :retry_delay, 0.0).to_f
          }
        end

        # Extract business metrics tags for categorization
        #
        # @param event [Hash, Dry::Events::Event] The event payload
        # @return [Array<String>] Array of tag strings for metrics systems
        #
        # Example:
        #   tags = extract_metric_tags(event)
        #   StatsD.increment('tasker.task.completed', tags: tags)
        def extract_metric_tags(event)
          MetricTagsExtractor.extract(event)
        end

        # Service class to extract and build metric tags from events
        # Reduces complexity by organizing tag building logic
        class MetricTagsExtractor
          class << self
            # Extract all metric tags from event
            #
            # @param event [Hash, Dry::Events::Event] The event payload
            # @return [Array<String>] Array of tag strings
            def extract(event)
              tags = []

              tags.concat(extract_entity_tags(event))
              tags.concat(extract_environment_tags(event))
              tags.concat(extract_status_tags(event))
              tags.concat(extract_business_tags(event))

              tags.compact
            end

            private

            # Extract core entity tags (task, step names)
            #
            # @param event [Hash, Dry::Events::Event] The event payload
            # @return [Array<String>] Entity tags
            def extract_entity_tags(event)
              tags = []

              task_name = safe_get_from_event(event, :task_name)
              tags << "task:#{task_name}" if task_name && task_name != 'unknown'

              step_name = safe_get_from_event(event, :step_name)
              tags << "step:#{step_name}" if step_name && step_name != 'unknown_step'

              tags
            end

            # Extract environment and source tags
            #
            # @param event [Hash, Dry::Events::Event] The event payload
            # @return [Array<String>] Environment tags
            def extract_environment_tags(event)
              tags = []

              tags << "environment:#{defined?(Rails) ? Rails.env : 'unknown'}"

              source_system = safe_get_from_event(event, :source_system)
              tags << "source:#{source_system}" if source_system

              tags
            end

            # Extract status and retry tags
            #
            # @param event [Hash, Dry::Events::Event] The event payload
            # @return [Array<String>] Status tags
            def extract_status_tags(event)
              tags = []

              retryable = safe_get_from_event(event, :retryable)
              tags << "retryable:#{retryable}" if retryable

              attempt_number = safe_get_from_event(event, :attempt_number)
              tags << "attempt:#{attempt_number}" if attempt_number

              tags
            end

            # Extract business context tags
            #
            # @param event [Hash, Dry::Events::Event] The event payload
            # @return [Array<String>] Business tags
            def extract_business_tags(event)
              tags = []

              priority = safe_get_from_event(event, :priority)
              tags << "priority:#{priority}" if priority

              tags
            end

            # Safely get value from event (reuse BaseSubscriber logic)
            #
            # @param event [Hash, Dry::Events::Event] The event payload
            # @param key [Symbol] The key to extract
            # @param default [Object] Default value
            # @return [Object] Extracted value
            def safe_get_from_event(event, key, default = nil)
              return default if event.nil?

              # Handle Dry::Events::Event object
              if event.respond_to?(:payload) && event.payload.respond_to?(:fetch)
                payload = event.payload
                return payload.fetch(key.to_sym) { payload.fetch(key.to_s, default) }
              end

              # Handle plain hash events
              event.fetch(key.to_sym) do
                event.fetch(key.to_s, default)
              end
            end
          end
        end

        # Create metric name with consistent naming convention
        #
        # @param base_name [String] The base metric name
        # @param event_type [String] The event type (completed, failed, etc.)
        # @return [String] Standardized metric name
        #
        # Example:
        #   metric_name = build_metric_name('tasker.task', 'completed')
        #   # => 'tasker.task.completed'
        def build_metric_name(base_name, event_type)
          "#{base_name}.#{event_type}".squeeze('.')
        end

        # Extract numeric value safely for metrics
        #
        # @param event [Hash, Dry::Events::Event] The event payload
        # @param key [Symbol, String] The key to extract
        # @param default [Numeric] The default value
        # @return [Numeric] The numeric value
        #
        # Example:
        #   duration = extract_numeric_metric(event, :execution_duration, 0.0)
        #   StatsD.histogram('task.duration', duration)
        def extract_numeric_metric(event, key, default = 0)
          value = safe_get(event, key, default)

          # Handle nil values and non-numeric types
          return default if value.nil? || !value.respond_to?(:to_f)

          # Try to convert to float
          converted = value.to_f

          # Check if this was a valid numeric conversion
          # For strings that can't convert, to_f returns 0.0
          return default if converted == 0.0 && value.is_a?(String) && value !~ /\A\s*0*(\\.0*)?\s*\z/

          converted
        end

        private

        # Categorize error types for metrics tagging
        #
        # @param error_class [String] The exception class name
        # @return [String] Categorized error type
        def categorize_error(error_class)
          ErrorCategorizer.categorize(error_class)
        end

        # Handle errors in subscribers
        def handle_subscriber_error(event_type, _event_payload, error)
          @logger.error("Subscriber error handling #{event_type}: #{error.message}")
          @logger.error(error.backtrace.join("\n")) if error.backtrace
        end

        # Service class to categorize error types for metrics
        # Reduces complexity by organizing error classification logic
        class ErrorCategorizer
          class << self
            # Categorize error based on class name
            #
            # @param error_class [String] The exception class name
            # @return [String] Categorized error type
            def categorize(error_class)
              return 'unknown' if error_class.blank?

              error_string = error_class.to_s

              classify_error_type(error_string)
            end

            private

            # Classify error type using pattern matching
            #
            # @param error_string [String] The error class as string
            # @return [String] Classified error type
            def classify_error_type(error_string)
              ErrorTypeClassifier.classify(error_string)
            end

            # Service class to classify error types using pattern matching
            # Reduces complexity by organizing error type classification logic
            class ErrorTypeClassifier
              class << self
                # Classify error type from error string
                #
                # @param error_string [String] The error class as string
                # @return [String] Classified error type
                def classify(error_string)
                  ERROR_TYPE_MATCHERS.each do |type, matcher|
                    return type if matcher.call(error_string)
                  end

                  'other'
                end

                # Map of error types to their matching logic
                ERROR_TYPE_MATCHERS = {
                  'timeout' => ->(str) { timeout_error?(str) },
                  'network' => ->(str) { network_error?(str) },
                  'not_found' => ->(str) { not_found_error?(str) },
                  'auth' => ->(str) { auth_error?(str) },
                  'rate_limit' => ->(str) { rate_limit_error?(str) },
                  'client_error' => ->(str) { client_error?(str) },
                  'server_error' => ->(str) { server_error?(str) },
                  'runtime' => ->(str) { runtime_error?(str) }
                }.freeze

                # Check if error is timeout-related
                #
                # @param error_string [String] The error class as string
                # @return [Boolean] True if timeout error
                def self.timeout_error?(error_string)
                  error_string.match?(/Timeout/i) || error_string.match?(/TimeoutError/i)
                end

                # Check if error is network-related
                #
                # @param error_string [String] The error class as string
                # @return [Boolean] True if network error
                def self.network_error?(error_string)
                  error_string.match?(/Connection/i) || error_string.match?(/Network/i)
                end

                # Check if error is not found-related
                #
                # @param error_string [String] The error class as string
                # @return [Boolean] True if not found error
                def self.not_found_error?(error_string)
                  error_string.match?(/NotFound/i) || error_string.match?(/Missing/i) || error_string.include?('404')
                end

                # Check if error is authentication-related
                #
                # @param error_string [String] The error class as string
                # @return [Boolean] True if auth error
                def self.auth_error?(error_string)
                  error_string.match?(/Unauthorized/i) || error_string.match?(/Forbidden/i) ||
                    error_string.include?('401') || error_string.include?('403')
                end

                # Check if error is rate limit-related
                #
                # @param error_string [String] The error class as string
                # @return [Boolean] True if rate limit error
                def self.rate_limit_error?(error_string)
                  error_string.match?(/RateLimit/i) || error_string.match?(/TooManyRequests/i) ||
                    error_string.include?('429')
                end

                # Check if error is client error-related
                #
                # @param error_string [String] The error class as string
                # @return [Boolean] True if client error
                def self.client_error?(error_string)
                  error_string.match?(/BadRequest/i) || error_string.match?(/Invalid/i) ||
                    error_string.include?('400')
                end

                # Check if error is server error-related
                #
                # @param error_string [String] The error class as string
                # @return [Boolean] True if server error
                def self.server_error?(error_string)
                  error_string.match?(/ServerError/i) || error_string.match?(/InternalError/i) ||
                    error_string.include?('500')
                end

                # Check if error is runtime-related
                #
                # @param error_string [String] The error class as string
                # @return [Boolean] True if runtime error
                def self.runtime_error?(error_string)
                  error_string.match?(/StandardError/i) || error_string.match?(/RuntimeError/i)
                end
              end
            end
          end
        end
      end
    end
  end
end
