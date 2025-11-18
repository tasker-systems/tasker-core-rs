# frozen_string_literal: true

module TaskerCore
  module BatchProcessing
    module StepHandlers
      # CsvResultsAggregatorHandler - Deferred convergence step that aggregates batch worker results
      #
      # This handler:
      # 1. Detects batch processing scenario (NoBatches vs WithBatches)
      # 2. Handles NoBatches scenario with zero metrics
      # 3. Aggregates results from all batch workers in WithBatches scenario
      # 4. Calculates overall inventory metrics
      #
      # Expected dependency results:
      #   - analyze_csv: Analyzer step result (contains batch_processing_outcome)
      #   - process_csv_batch_*: One or more batch worker results
      #
      # Aggregation logic:
      #   - Sum: processed_count, total_inventory_value
      #   - Merge: category_counts (hash merge with addition)
      #   - Max: max_price, max_price_product
      #   - Weighted average: overall_average_rating
      #
      # Returns:
      #   Success with aggregated metrics and worker_count
      class CsvResultsAggregatorHandler < StepHandler::Batchable
        def call(_task, sequence, _step)
          # Detect batch processing scenario
          scenario = detect_aggregation_scenario(sequence, 'analyze_csv', 'process_csv_batch_')

          # Aggregate batch worker results (handles both NoBatches and WithBatches scenarios)
          aggregate_batch_worker_results(
            scenario,
            zero_metrics: {
              'total_processed' => 0,
              'total_inventory_value' => 0.0,
              'category_counts' => {},
              'max_price' => 0.0,
              'max_price_product' => nil,
              'overall_average_rating' => 0.0
            }
          ) do |batch_results|
            # Custom aggregation logic for inventory metrics
            aggregate_batch_results(batch_results)
          end
        end

        private

        # Aggregate results from all batch workers
        #
        # @param batch_results [Hash] Hash of batch worker results (worker_name => result_hash)
        # @return [Hash] Aggregated metrics
        def aggregate_batch_results(batch_results)
          total_processed = 0
          total_inventory_value = 0.0
          category_counts = Hash.new(0)
          max_price = 0.0
          max_price_product = nil
          total_weighted_rating = 0.0

          batch_results.each_value do |result|
            # Sum processed counts
            processed = result['processed_count'] || 0
            total_processed += processed

            # Sum inventory values
            total_inventory_value += result['total_inventory_value'] || 0.0

            # Merge category counts
            result['category_counts']&.each do |category, count|
              category_counts[category] += count
            end

            # Track maximum price product across all batches
            batch_max_price = result['max_price'] || 0.0
            if batch_max_price > max_price
              max_price = batch_max_price
              max_price_product = result['max_price_product']
            end

            # Accumulate weighted rating (rating * count for proper averaging)
            batch_avg_rating = result['average_rating'] || 0.0
            total_weighted_rating += (batch_avg_rating * processed)
          end

          # Calculate overall average rating (weighted by processed counts)
          overall_average_rating = if total_processed.positive?
                                     (total_weighted_rating / total_processed).round(2)
                                   else
                                     0.0
                                   end

          {
            'total_processed' => total_processed,
            'total_inventory_value' => total_inventory_value.round(2),
            'category_counts' => category_counts,
            'max_price' => max_price,
            'max_price_product' => max_price_product,
            'overall_average_rating' => overall_average_rating
          }
        end
      end
    end
  end
end
