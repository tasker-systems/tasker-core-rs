# frozen_string_literal: true

module TaskerCore
  module Examples
    module CheckpointYield
      # TAS-125: Checkpoint Yield Worker Handler
      #
      # Processes items with checkpoint yielding to test the TAS-125
      # checkpoint persistence and re-dispatch mechanism.
      #
      # Configuration from task context:
      #   - items_per_checkpoint: Items before checkpoint yield (default: 25)
      #   - fail_after_items: Fail after processing this many items (optional)
      #   - fail_on_attempt: Only fail on this attempt number (default: 1)
      #   - permanent_failure: If true, fail with non-retryable error (default: false)
      #
      # Checkpoint behavior:
      #   - After processing items_per_checkpoint items, calls checkpoint_yield()
      #   - Checkpoint persists cursor position and accumulated results
      #   - On resume, continues from checkpoint cursor with accumulated results
      class WorkerHandler < StepHandler::Batchable
        def call(context)
          # Get batch worker context first
          batch_ctx = get_batch_context(context)
          return batch_worker_failure('No batch inputs found') unless batch_ctx

          # Check for no-op placeholder (must use batch context, not execution context)
          no_op_result = handle_no_op_worker(batch_ctx)
          return no_op_result if no_op_result

          # Get configuration from task context
          items_per_checkpoint = context.task.context['items_per_checkpoint'] ||
                                 context.step_definition.handler.initialization['items_per_checkpoint'] ||
                                 25
          fail_after_items = context.task.context['fail_after_items']
          fail_on_attempt = context.task.context['fail_on_attempt'] || 1
          permanent_failure = context.task.context['permanent_failure'] || false

          # Determine starting position
          if batch_ctx.has_checkpoint?
            # Resume from checkpoint
            start_cursor = batch_ctx.checkpoint_cursor
            accumulated = batch_ctx.accumulated_results || { 'running_total' => 0, 'item_ids' => [] }
            total_processed = batch_ctx.checkpoint_items_processed
          else
            # Fresh start
            start_cursor = batch_ctx.start_cursor
            accumulated = { 'running_total' => 0, 'item_ids' => [] }
            total_processed = 0
          end

          end_cursor = batch_ctx.end_cursor
          # Use context.retry_count which safely returns 0 if attempts is nil
          current_attempt = context.retry_count + 1 # 1-indexed

          # Process items in chunks
          current_cursor = start_cursor
          items_in_chunk = 0

          while current_cursor < end_cursor
            # Check for failure injection
            if fail_after_items && total_processed >= fail_after_items && current_attempt == fail_on_attempt
              return inject_failure(total_processed, current_cursor, permanent_failure)
            end

            # Process one item
            item_result = process_item(current_cursor)
            accumulated['running_total'] += item_result[:value]
            accumulated['item_ids'] << item_result[:id]

            current_cursor += 1
            items_in_chunk += 1
            total_processed += 1

            # Check if we should yield a checkpoint
            if items_in_chunk >= items_per_checkpoint && current_cursor < end_cursor
              return checkpoint_yield(
                cursor: current_cursor,
                items_processed: total_processed,
                accumulated_results: accumulated
              )
            end
          end

          # All items processed - return success
          # Use results: (not batch_metadata:) to match batch_worker_success signature
          batch_worker_success(
            items_processed: total_processed,
            items_succeeded: total_processed,
            items_failed: 0,
            results: accumulated.merge(
              'final_cursor' => current_cursor,
              'checkpoints_used' => total_processed / items_per_checkpoint
            )
          )
        end

        private

        def process_item(cursor)
          # Simple processing - just compute a value based on cursor
          {
            id: "item_#{cursor.to_s.rjust(4, '0')}",
            value: cursor + 1 # 1-indexed value
          }
        end

        def inject_failure(items_processed, cursor, permanent)
          if permanent
            TaskerCore::Types::StepHandlerCallResult.error(
              message: "Injected permanent failure after #{items_processed} items",
              error_type: 'PermanentError',
              retryable: false,
              metadata: {
                items_processed: items_processed,
                cursor_at_failure: cursor,
                failure_type: 'permanent'
              }
            )
          else
            TaskerCore::Types::StepHandlerCallResult.error(
              message: "Injected transient failure after #{items_processed} items",
              error_type: 'RetryableError',
              retryable: true,
              metadata: {
                items_processed: items_processed,
                cursor_at_failure: cursor,
                failure_type: 'transient'
              }
            )
          end
        end
      end
    end
  end
end
