# frozen_string_literal: true

module TaskerCore
  module Worker
    # Subscribes to step execution events and routes to handlers
    class StepExecutionSubscriber
      attr_reader :logger, :handler_registry, :stats

      def initialize
        @logger = TaskerCore::Logger.instance
        @handler_registry = TaskerCore::Registry::HandlerRegistry.instance
        @stats = { processed: 0, succeeded: 0, failed: 0 }
        @active = true

        # Subscribe to step execution events
        TaskerCore::Worker::EventBridge.instance.subscribe_to_step_execution do |event|
          call(event)
        end
        logger.info 'Step execution subscriber initialized'
      end

      # Check if subscriber is active
      def active?
        @active
      end

      # Stop the subscriber
      def stop!
        @active = false
        logger.info 'Step execution subscriber stopped'
      end

      # Called by dry-events when step execution is requested
      def call(event)
        event_data = event.payload
        step_data = event_data[:task_sequence_step]

        logger.info 'Processing step execution request'
        logger.info("Event ID: #{event_data[:event_id]}")
        logger.info("Step: #{step_data.workflow_step.name}")
        logger.info("Handler: #{step_data.step_definition.handler.callable}")

        @stats[:processed] += 1

        begin
          # Resolve step handler from registry
          handler = @handler_registry.resolve_handler(step_data.step_definition.handler.callable)

          raise TaskerCore::Error, "No handler found for #{step_data.step_definition.handler.callable}" unless handler

          # Execute handler with step data
          result = handler.call(
            step_data.task,
            step_data.dependency_results,
            step_data.workflow_step
          )

          # Convert handler output to standardized result
          standardized_result = TaskerCore::Types::StepHandlerCallResult.from_handler_output(result)

          unless standardized_result.success?
            raise TaskerCore::Error, "Handler returned failure: #{standardized_result.message}"
          end

          # Publish successful completion
          publish_step_completion(
            event_data: event_data,
            success: true,
            result: standardized_result.result,
            metadata: {
              processed_at: Time.now.utc.iso8601,
              processed_by: 'ruby_worker',
              handler_class: step_data.step_definition.handler.callable,
              duration_ms: standardized_result.metadata&.dig('duration_ms')
            }.merge(standardized_result.metadata || {})
          )

          @stats[:succeeded] += 1
          logger.info('✅ Step execution completed successfully')
        rescue StandardError => e
          logger.error("💥 Step execution failed: #{e.message}")
          logger.error("💥 #{e.backtrace.first(5).join("\n💥 ")}")

          # Publish failure completion
          publish_step_completion(
            event_data: event_data,
            success: false,
            result: nil,
            error_message: e.message,
            metadata: {
              failed_at: Time.now.utc.iso8601,
              failed_by: 'ruby_worker',
              error_class: e.class.name,
              handler_class: step_data.step_definition.handler.callable
            }
          )

          @stats[:failed] += 1
        end
      end

      private

      def publish_step_completion(event_data:, success:, result: nil, error_message: nil, metadata: nil)
        TaskerCore::Worker::EventBridge.instance.publish_step_completion(
          {
            event_id: event_data[:event_id],
            task_uuid: event_data[:task_uuid],
            step_uuid: event_data[:step_uuid],
            success: success,
            result: result,
            metadata: metadata,
            error_message: error_message
          }
        )
      end
    end
  end
end
