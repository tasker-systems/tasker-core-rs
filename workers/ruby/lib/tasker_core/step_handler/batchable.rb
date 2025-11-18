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
    # ## Usage
    #
    # ```ruby
    # class CsvBatchProcessorHandler < TaskerCore::StepHandler::Batchable
    #   def call(_task, sequence, step)
    #     # Extract cursor context
    #     context = extract_cursor_context(step)
    #
    #     # Handle no-op placeholder
    #     no_op_result = handle_no_op_worker(context)
    #     return no_op_result if no_op_result
    #
    #     # Get dependency results
    #     csv_file = get_dependency_result(sequence, 'analyze_csv', 'csv_file_path')
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

      # Extract cursor context from workflow step
      #
      # @param step [WorkflowStepWrapper] Workflow step containing cursor configuration
      # @return [BatchWorkerContext] Extracted cursor context
      #
      # @example
      #   context = extract_cursor_context(step)
      #   batch_id = context.batch_id
      #   start = context.start_position
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
      # @param total_items [Integer] Total number of items to process
      # @param worker_count [Integer] Number of workers to create configs for
      # @yield [config, index] Optional block to customize each config
      # @yieldparam config [Hash] Cursor config being created
      # @yieldparam index [Integer] Worker index (0-based)
      # @return [Array<Hash>] Array of cursor configurations
      #
      # @example Basic usage
      #   configs = create_cursor_configs(1000, 5)
      #   # => [
      #   #   { 'batch_id' => '001', 'start_cursor' => 1, 'end_cursor' => 201, 'batch_size' => 200 },
      #   #   { 'batch_id' => '002', 'start_cursor' => 201, 'end_cursor' => 401, 'batch_size' => 200 },
      #   #   ...
      #   # ]
      #
      # @example With customization
      #   configs = create_cursor_configs(1000, 5) do |config, i|
      #     config['worker_name'] = "worker_#{i + 1}"
      #   end
      def create_cursor_configs(total_items, worker_count)
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
      # @example
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
