# frozen_string_literal: true

module TaskerCore
  module Examples
    module CheckpointYield
      # TAS-125: Checkpoint Yield Analyzer Handler
      #
      # Creates a single batch for testing checkpoint yielding.
      # Configuration from task context:
      #   - total_items: Number of items to process (default: 100)
      #
      # Returns batch_processing_outcome with a single batch covering all items.
      class AnalyzerHandler < StepHandler::Batchable
        def call(context)
          # Get configuration from task context
          total_items = context.task.context['total_items'] || 100

          # Get worker template name from step definition initialization
          worker_template_name = context.step_definition.handler.initialization['worker_template_name'] ||
                                 'checkpoint_yield_batch'

          if total_items <= 0
            # No items to process - return no_batches outcome
            return no_batches_outcome(reason: 'no_items_to_process')
          end

          # Create a single batch for all items (checkpoint yielding handles chunking)
          # Using 1 worker = 1 batch covering all items
          cursor_configs = create_cursor_configs(total_items, 1)

          # Return batch analyzer success with batch configuration
          create_batches_outcome(
            worker_template_name: worker_template_name,
            cursor_configs: cursor_configs,
            total_items: total_items,
            metadata: {
              'test_type' => 'checkpoint_yield',
              'items_per_batch' => total_items
            }
          )
        end
      end
    end
  end
end
