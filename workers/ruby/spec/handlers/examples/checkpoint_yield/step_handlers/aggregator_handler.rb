# frozen_string_literal: true

module TaskerCore
  module Examples
    module CheckpointYield
      # TAS-125: Checkpoint Yield Aggregator Handler
      #
      # Aggregates results from checkpoint yield batch workers.
      # Collects final output for E2E test verification.
      class AggregatorHandler < StepHandler::Batchable
        def call(context)
          # Detect aggregation scenario
          scenario = detect_aggregation_scenario(
            context.dependency_results,
            'analyze_items',
            'checkpoint_yield_batch_'
          )

          if scenario.no_batches?
            return no_batches_aggregation_result(
              'total_processed' => 0,
              'running_total' => 0,
              'test_passed' => true,
              'scenario' => 'no_batches'
            )
          end

          # Aggregate from batch results
          total_processed = 0
          running_total = 0
          all_item_ids = []
          checkpoints_used = 0

          scenario.batch_results.each_value do |result|
            next unless result

            total_processed += result['items_processed'] || 0
            # batch_worker_success stores custom data under 'results' key
            batch_results = result['results'] || {}
            running_total += batch_results['running_total'] || 0
            all_item_ids.concat(batch_results['item_ids'] || [])
            checkpoints_used += batch_results['checkpoints_used'] || 0
          end

          TaskerCore::Types::StepHandlerCallResult.success(
            result: {
              'total_processed' => total_processed,
              'running_total' => running_total,
              'item_count' => all_item_ids.length,
              'checkpoints_used' => checkpoints_used,
              'worker_count' => scenario.worker_count,
              'test_passed' => true,
              'scenario' => 'with_batches'
            }
          )
        end
      end
    end
  end
end
