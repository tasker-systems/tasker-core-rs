# frozen_string_literal: true

require_relative 'base'

module TaskerCore
  module StepHandler
    # Batchable step handler base class for batch processing handlers
    #
    # This class provides a marker for batch processing handlers and declares
    # the batch processing capabilities. It also provides helper methods to
    # reduce boilerplate in common batch processing patterns.
    #
    # ## IMPORTANT: Outcome Helper Methods Return Success Objects
    #
    # The outcome helper methods (no_batches_outcome, create_batches_outcome, etc.)
    # return fully-wrapped Success objects, NOT raw data hashes.
    #
    # ✅ CORRECT: Return helper result directly
    # ```ruby
    # def call(context)
    #   if dataset_empty?
    #     return no_batches_outcome(reason: 'empty_dataset')  # Returns Success
    #   end
    # end
    # ```
    #
    # ❌ INCORRECT: Double-wrapping (wrapping Success in Success)
    # ```ruby
    # def call(context)
    #   outcome = no_batches_outcome(reason: 'empty_dataset')  # Returns Success
    #   success(result: outcome)  # WRONG: Double wrapping!
    # end
    # ```
    #
    # ## Usage
    #
    # ```ruby
    # class CsvBatchProcessorHandler < TaskerCore::StepHandler::Batchable
    #   def call(context)
    #     # Extract batch context (cross-language standard)
    #     batch_ctx = get_batch_context(context)
    #
    #     # Handle no-op placeholder
    #     no_op_result = handle_no_op_worker(batch_ctx)
    #     return no_op_result if no_op_result
    #
    #     # Get dependency results
    #     csv_file = context.get_dependency_result('analyze_csv')&.dig('csv_file_path')
    #
    #     # Handler-specific processing...
    #   end
    # end
    # ```
    class Batchable < Base
      # Override capabilities to include batch-specific features
      def capabilities
        super + %w[batchable batch_processing parallel_execution cursor_based deferred_convergence]
      end

      protected

      # Category 1: Context Extraction Helpers

      # Cross-language standard: Extract batch context from step context
      #
      # @param context [TaskerCore::Types::StepContext] Step execution context
      # @return [BatchWorkerContext] Extracted batch context
      #
      # @example
      #   batch_ctx = get_batch_context(context)
      #   batch_id = batch_ctx.batch_id
      #   start = batch_ctx.start_cursor
      def get_batch_context(context)
        BatchProcessing::BatchWorkerContext.from_step_data(context.workflow_step)
      end

      # Extract cursor context from workflow step (legacy method)
      #
      # @deprecated Use {#get_batch_context} instead
      # @param step [WorkflowStepWrapper] Workflow step containing cursor configuration
      # @return [BatchWorkerContext] Extracted cursor context
      #
      # @example
      #   batch_ctx = extract_cursor_context(step)
      #   batch_id = batch_ctx.batch_id
      #   start = batch_ctx.start_cursor
      def extract_cursor_context(step)
        BatchProcessing::BatchWorkerContext.from_step_data(step)
      end

      # Detect batch aggregation scenario from dependency results
      #
      # @param sequence [DependencyResultsWrapper] Dependency results to analyze
      # @param analyzer_step_name [String] Name of the analyzer step
      # @param batch_worker_prefix [String] Prefix for batch worker step names
      # @return [BatchAggregationScenario] Detected scenario (NoBatches or WithBatches)
      #
      # @example
      #   scenario = detect_aggregation_scenario(sequence, 'analyze_csv', 'process_csv_batch_')
      #   if scenario.no_batches?
      #     return no_batches_aggregation_result
      #   end
      def detect_aggregation_scenario(sequence, analyzer_step_name, batch_worker_prefix)
        BatchProcessing::BatchAggregationScenario.detect(
          sequence,
          analyzer_step_name,
          batch_worker_prefix
        )
      end

      # Extract dependency result with safe navigation
      #
      # @param sequence [DependencyResultsWrapper] Dependency results
      # @param step_name [String] Name of the dependency step
      # @param keys [Array<String>] Optional keys to dig into result hash
      # @return [Object, nil] Extracted result or nil if not found
      #
      # @example
      #   # Get full result
      #   result = get_dependency_result(sequence, 'analyze_csv')
      #
      #   # Get nested field
      #   csv_path = get_dependency_result(sequence, 'analyze_csv', 'csv_file_path')
      def get_dependency_result(sequence, step_name, *keys)
        result = sequence.get_results(step_name)
        keys.empty? ? result : result&.dig(*keys)
      end

      # Category 2: No-Op Worker Handling

      # Handle no-op placeholder worker scenario
      #
      # Returns a success result if the worker is a no-op placeholder,
      # otherwise returns nil to allow normal processing to continue.
      #
      # @param context [BatchWorkerContext] Cursor context
      # @return [StepHandlerCallResult::Success, nil] Success result if no-op, nil otherwise
      #
      # @example
      #   context = extract_cursor_context(step)
      #   no_op_result = handle_no_op_worker(context)
      #   return no_op_result if no_op_result
      def handle_no_op_worker(context)
        return nil unless context.no_op?

        success(
          result: {
            'batch_id' => context.batch_id,
            'no_op' => true,
            'processed_count' => 0
          }
        )
      end

      # Category 3: Cursor Config Creation

      # Create standard cursor configurations for batch workers
      #
      # Divides total items into roughly equal ranges for each worker.
      # Supports optional customization via block.
      #
      # By default, this helper creates numeric cursors (1..N). However, cursors are
      # intentionally flexible and can be customized to use alphanumeric strings,
      # timestamps, UUIDs, or any comparable type that matches your data partitioning
      # strategy. Use the block parameter to customize cursor values for your specific
      # use case.
      #
      # ## Cursor Boundary Math
      #
      # The method divides total_items into worker_count roughly equal ranges using
      # ceiling division to ensure all items are covered:
      #
      # 1. items_per_worker = ceil(total_items / worker_count)
      # 2. For worker i (0-indexed):
      #    - start_cursor = (i * items_per_worker) + 1  (1-indexed)
      #    - end_cursor = min(start_cursor + items_per_worker, total_items + 1)  (exclusive)
      #    - batch_size = end_cursor - start_cursor
      #
      # Example: 1000 items, 3 workers
      #   - items_per_worker = ceil(1000/3) = 334
      #   - Worker 0: start=1, end=335, size=334
      #   - Worker 1: start=335, end=669, size=334
      #   - Worker 2: start=669, end=1001, size=332 (slightly smaller due to ceiling)
      #
      # Typically used with create_batches_outcome to send cursor configs to Rust orchestration.
      #
      # @param total_items [Integer] Total number of items to process
      # @param worker_count [Integer] Number of workers to create configs for (must be > 0)
      # @yield [config, index] Optional block to customize each config
      # @yieldparam config [Hash] Cursor config being created
      # @yieldparam index [Integer] Worker index (0-based)
      # @return [Array<Hash>] Array of cursor configurations
      #
      # @example Basic usage (numeric cursors)
      #   configs = create_cursor_configs(1000, 5)
      #   # => [
      #   #   { 'batch_id' => '001', 'start_cursor' => 1, 'end_cursor' => 201, 'batch_size' => 200 },
      #   #   { 'batch_id' => '002', 'start_cursor' => 201, 'end_cursor' => 401, 'batch_size' => 200 },
      #   #   ...
      #   # ]
      #
      # @example With metadata customization
      #   configs = create_cursor_configs(1000, 5) do |config, i|
      #     config['worker_name'] = "worker_#{i + 1}"
      #   end
      #
      # @example Alphanumeric cursors (alphabetical ranges)
      #   alphabet_ranges = [['A', 'F'], ['G', 'M'], ['N', 'S'], ['T', 'Z']]
      #   configs = create_cursor_configs(alphabet_ranges.size, alphabet_ranges.size) do |config, i|
      #     config['start_cursor'] = alphabet_ranges[i][0]
      #     config['end_cursor'] = alphabet_ranges[i][1]
      #     config.delete('batch_size')  # Not applicable for non-numeric cursors
      #   end
      #
      # @example UUID-based cursors (for UUID-partitioned datasets)
      #   uuid_ranges = [
      #     ['00000000-0000-0000-0000-000000000000', '3fffffff-ffff-ffff-ffff-ffffffffffff'],
      #     ['40000000-0000-0000-0000-000000000000', '7fffffff-ffff-ffff-ffff-ffffffffffff'],
      #     ['80000000-0000-0000-0000-000000000000', 'bfffffff-ffff-ffff-ffff-ffffffffffff'],
      #     ['c0000000-0000-0000-0000-000000000000', 'ffffffff-ffff-ffff-ffff-ffffffffffff']
      #   ]
      #   configs = create_cursor_configs(uuid_ranges.size, uuid_ranges.size) do |config, i|
      #     config['start_cursor'] = uuid_ranges[i][0]
      #     config['end_cursor'] = uuid_ranges[i][1]
      #     config.delete('batch_size')
      #   end
      #
      # @example Timestamp cursors (for time-partitioned datasets)
      #   time_ranges = [
      #     ['2024-01-01T00:00:00Z', '2024-01-08T00:00:00Z'],
      #     ['2024-01-08T00:00:00Z', '2024-01-15T00:00:00Z'],
      #     ['2024-01-15T00:00:00Z', '2024-01-22T00:00:00Z'],
      #     ['2024-01-22T00:00:00Z', '2024-01-31T23:59:59Z']
      #   ]
      #   configs = create_cursor_configs(time_ranges.size, time_ranges.size) do |config, i|
      #     config['start_cursor'] = time_ranges[i][0]
      #     config['end_cursor'] = time_ranges[i][1]
      #     config.delete('batch_size')
      #   end
      def create_cursor_configs(total_items, worker_count)
        raise ArgumentError, 'worker_count must be > 0' if worker_count <= 0

        items_per_worker = (total_items.to_f / worker_count).ceil

        (0...worker_count).map do |i|
          start_position = (i * items_per_worker) + 1
          end_position = [(start_position + items_per_worker), total_items + 1].min

          config = {
            'batch_id' => format('%03d', i + 1),
            'start_cursor' => start_position,
            'end_cursor' => end_position,
            'batch_size' => end_position - start_position
          }

          # Allow customization via block
          yield(config, i) if block_given?
          config
        end
      end

      # Category 4: Standard Outcome Builders

      # Create NoBatches outcome for analyzer steps
      #
      # @param reason [String] Reason why batching is not needed
      # @param metadata [Hash] Additional metadata to include in result
      # @return [StepHandlerCallResult::Success] Success result with NoBatches outcome
      #
      # @example
      #   return no_batches_outcome(
      #     reason: 'dataset_too_small',
      #     metadata: { 'total_rows' => 0 }
      #   )
      def no_batches_outcome(reason:, metadata: {})
        outcome = TaskerCore::Types::BatchProcessingOutcome.no_batches

        success(
          result: {
            'batch_processing_outcome' => outcome.to_h,
            'reason' => reason
          }.merge(metadata)
        )
      end

      # Create CreateBatches outcome for analyzer steps
      #
      # @param worker_template_name [String] Name of worker template to use
      # @param cursor_configs [Array<Hash>] Array of cursor configurations
      # @param total_items [Integer] Total number of items to process
      # @param metadata [Hash] Additional metadata to include in result
      # @return [StepHandlerCallResult::Success] Success result with CreateBatches outcome
      #
      # @example
      #   cursor_configs = create_cursor_configs(1000, 5)
      #   return create_batches_outcome(
      #     worker_template_name: 'process_csv_batch',
      #     cursor_configs: cursor_configs,
      #     total_items: 1000,
      #     metadata: { 'csv_file_path' => '/path/to/file.csv' }
      #   )
      def create_batches_outcome(worker_template_name:, cursor_configs:, total_items:, metadata: {})
        outcome = TaskerCore::Types::BatchProcessingOutcome.create_batches(
          worker_template_name: worker_template_name,
          worker_count: cursor_configs.size,
          cursor_configs: cursor_configs,
          total_items: total_items
        )

        success(
          result: {
            'batch_processing_outcome' => outcome.to_h,
            'worker_count' => cursor_configs.size,
            'total_items' => total_items
          }.merge(metadata)
        )
      end

      # Cross-language standard: Return success result for batch worker
      #
      # @param items_processed [Integer] Number of items processed
      # @param items_succeeded [Integer] Number of items that succeeded
      # @param items_failed [Integer] Number of items that failed (default 0)
      # @param items_skipped [Integer] Number of items skipped (default 0)
      # @param last_cursor [Object, nil] Last cursor position processed
      # @param results [Array, nil] Optional array of result items
      # @param errors [Array, nil] Optional array of error items
      # @param metadata [Hash] Additional metadata
      # @return [StepHandlerCallResult::Success] Success result with batch worker outcome
      #
      # @example
      #   batch_worker_success(
      #     items_processed: 100,
      #     items_succeeded: 98,
      #     items_failed: 2,
      #     last_cursor: 500,
      #     metadata: { batch_id: '001' }
      #   )
      def batch_worker_success(
        items_processed:,
        items_succeeded:,
        items_failed: 0,
        items_skipped: 0,
        last_cursor: nil,
        results: nil,
        errors: nil,
        metadata: {}
      )
        result_data = {
          'items_processed' => items_processed,
          'items_succeeded' => items_succeeded,
          'items_failed' => items_failed,
          'items_skipped' => items_skipped
        }

        result_data['last_cursor'] = last_cursor if last_cursor
        result_data['results'] = results if results
        result_data['errors'] = errors if errors

        success(
          result: result_data.merge(metadata),
          metadata: { batch_worker: true }
        )
      end

      # Category 5: Aggregation Helpers

      # Create no-batches aggregation result
      #
      # Used by aggregator steps when no batch processing occurred.
      #
      # @param metadata [Hash] Additional metadata (typically zero metrics)
      # @return [StepHandlerCallResult::Success] Success result for NoBatches scenario
      #
      # @example
      #   return no_batches_aggregation_result(
      #     metadata: {
      #       'total_processed' => 0,
      #       'total_value' => 0.0
      #     }
      #   )
      def no_batches_aggregation_result(metadata: {})
        success(
          result: {
            'worker_count' => 0,
            'scenario' => 'no_batches'
          }.merge(metadata)
        )
      end

      # Aggregate batch worker results
      #
      # Handles both NoBatches and WithBatches scenarios. For WithBatches,
      # delegates aggregation logic to provided block.
      #
      # @param scenario [BatchAggregationScenario] Detected scenario
      # @param zero_metrics [Hash] Metrics to return for NoBatches scenario
      # @yield [batch_results] Block to perform custom aggregation
      # @yieldparam batch_results [Hash] Hash of worker results (worker_name => result_hash)
      # @yieldreturn [Hash] Aggregated metrics
      # @return [StepHandlerCallResult::Success] Success result with aggregated data
      #
      # @example Sum aggregation (simple)
      #   scenario = detect_aggregation_scenario(sequence, 'analyze_csv', 'process_csv_batch_')
      #
      #   aggregate_batch_worker_results(
      #     scenario,
      #     zero_metrics: { 'total_processed' => 0, 'total_value' => 0.0 }
      #   ) do |batch_results|
      #     total_processed = 0
      #     total_value = 0.0
      #
      #     batch_results.each_value do |result|
      #       total_processed += result['processed_count'] || 0
      #       total_value += result['total_value'] || 0.0
      #     end
      #
      #     {
      #       'total_processed' => total_processed,
      #       'total_value' => total_value
      #     }
      #   end
      #
      # @example No customization (pass-through results)
      #   aggregate_batch_worker_results(scenario) { |results| results }
      #
      # @example Max aggregation (finding maximum value)
      #   aggregate_batch_worker_results(
      #     scenario,
      #     zero_metrics: { 'max_price' => 0.0 }
      #   ) do |batch_results|
      #     max_price = batch_results.values.map { |r| r['max_price'] || 0.0 }.max
      #     { 'max_price' => max_price }
      #   end
      #
      # @example Concat aggregation (merging arrays)
      #   aggregate_batch_worker_results(
      #     scenario,
      #     zero_metrics: { 'errors' => [] }
      #   ) do |batch_results|
      #     errors = batch_results.values.flat_map { |r| r['errors'] || [] }
      #     { 'errors' => errors }
      #   end
      #
      # @example Merge aggregation (combining hashes)
      #   aggregate_batch_worker_results(
      #     scenario,
      #     zero_metrics: { 'category_counts' => {} }
      #   ) do |batch_results|
      #     category_counts = batch_results.values.each_with_object({}) do |result, acc|
      #       result_counts = result['category_counts'] || {}
      #       result_counts.each do |category, count|
      #         acc[category] ||= 0
      #         acc[category] += count
      #       end
      #     end
      #     { 'category_counts' => category_counts }
      #   end
      #
      # @example Complex aggregation (multiple metrics with validation)
      #   aggregate_batch_worker_results(
      #     scenario,
      #     zero_metrics: {
      #       'total_processed' => 0,
      #       'total_value' => 0.0,
      #       'errors' => [],
      #       'processing_times_ms' => []
      #     }
      #   ) do |batch_results|
      #     total_processed = 0
      #     total_value = 0.0
      #     errors = []
      #     processing_times = []
      #
      #     batch_results.each do |worker_name, result|
      #       # Validate result structure
      #       unless result.is_a?(Hash)
      #         errors << "Invalid result from #{worker_name}: expected Hash, got #{result.class}"
      #         next
      #       end
      #
      #       total_processed += result['processed_count'] || 0
      #       total_value += result['total_value'] || 0.0
      #       errors.concat(result['errors'] || [])
      #       processing_times << result['processing_time_ms'] if result['processing_time_ms']
      #     end
      #
      #     {
      #       'total_processed' => total_processed,
      #       'total_value' => total_value,
      #       'errors' => errors,
      #       'processing_times_ms' => processing_times,
      #       'avg_processing_time_ms' => processing_times.empty? ? 0 : processing_times.sum / processing_times.size
      #     }
      #   end
      def aggregate_batch_worker_results(scenario, zero_metrics: {})
        return no_batches_aggregation_result(metadata: zero_metrics) if scenario.no_batches?

        aggregated = yield(scenario.batch_results)

        success(
          result: aggregated.merge(
            'worker_count' => scenario.worker_count,
            'scenario' => 'with_batches'
          )
        )
      end
    end
  end
end
