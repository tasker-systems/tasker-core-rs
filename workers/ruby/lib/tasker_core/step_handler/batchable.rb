# frozen_string_literal: true

require_relative 'base'
require_relative 'mixins'

module TaskerCore
  module StepHandler
    # Batchable step handler for batch processing handlers
    #
    # ## TAS-112: Composition Pattern (DEPRECATED CLASS)
    #
    # This class is provided for backward compatibility. For new code, use the mixin pattern:
    #
    # ```ruby
    # class CsvBatchProcessorHandler < TaskerCore::StepHandler::Base
    #   include TaskerCore::StepHandler::Mixins::Batchable
    #
    #   def call(context)
    #     batch_ctx = get_batch_context(context)
    #
    #     # Handle no-op placeholder
    #     no_op_result = handle_no_op_worker(batch_ctx)
    #     return no_op_result if no_op_result
    #
    #     # Handler-specific processing...
    #   end
    # end
    # ```
    #
    # ## TAS-112: 0-Indexed Cursors (BREAKING CHANGE)
    #
    # As of TAS-112, cursor indexing is 0-based to match Python, TypeScript, and Rust.
    # Previously Ruby used 1-based indexing.
    #
    # ## IMPORTANT: Outcome Helper Methods Return Success Objects
    #
    # The outcome helper methods return fully-wrapped Success objects:
    #
    # ```ruby
    # def call(context)
    #   if dataset_empty?
    #     return no_batches_outcome(reason: 'empty_dataset')  # Returns Success
    #   end
    # end
    # ```
    class Batchable < Base
      include Mixins::Batchable
    end
  end
end
