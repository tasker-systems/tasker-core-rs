# frozen_string_literal: true

module TaskerCore
  module BatchProcessing
    # Ruby equivalent of Rust's BatchAggregationScenario
    #
    # Detects whether a batchable step created batch workers or returned NoBatches outcome.
    # Uses DependencyResultsWrapper to access parent step results.
    #
    # This helper distinguishes two scenarios:
    # - **NoBatches**: Batchable step returned NoBatches outcome, only placeholder worker exists
    # - **WithBatches**: Batchable step created real batch workers for parallel processing
    #
    # @example Detecting aggregation scenario
    #   scenario = BatchAggregationScenario.detect(
    #     sequence_step,
    #     'analyze_dataset',
    #     'process_batch_'
    #   )
    #
    #   if scenario.no_batches?
    #     # Handle NoBatches scenario - use batchable result directly
    #     return scenario.batchable_result
    #   else
    #     # Handle WithBatches scenario - aggregate worker results
    #     total = scenario.batch_results.values.sum
    #     return { total: total, worker_count: scenario.worker_count }
    #   end
    class BatchAggregationScenario
      attr_reader :type, :batchable_result, :batch_results, :worker_count

      # Detect aggregation scenario from step dependencies
      #
      # @param dependency_results [DependencyResultsWrapper] Dependency results wrapper
      # @param batchable_step_name [String] Name of the batchable step
      # @param batch_worker_prefix [String] Prefix for batch worker step names
      # @return [BatchAggregationScenario] Detected scenario
      def self.detect(dependency_results, batchable_step_name, batch_worker_prefix)
        # dependency_results is already a DependencyResultsWrapper - pass it directly
        new(dependency_results, batchable_step_name, batch_worker_prefix)
      end

      # Initialize aggregation scenario
      #
      # @param dependency_results [DependencyResultsWrapper] Dependency results wrapper
      # @param batchable_step_name [String] Name of the batchable step
      # @param batch_worker_prefix [String] Prefix for batch worker step names
      def initialize(dependency_results, batchable_step_name, batch_worker_prefix)
        # Get the batchable step result (extract the 'result' field)
        @batchable_result = dependency_results.get_results(batchable_step_name)

        # Find all batch worker results by prefix matching (extract 'result' field from each)
        @batch_results = {}
        dependency_results.each_key do |step_name|
          if step_name.start_with?(batch_worker_prefix)
            @batch_results[step_name] = dependency_results.get_results(step_name)
          end
        end

        # Determine scenario type based on batch worker count
        if @batch_results.empty?
          @type = :no_batches
          @worker_count = 0
        else
          @type = :with_batches
          @worker_count = @batch_results.size
        end
      end

      # Check if scenario is NoBatches
      #
      # @return [Boolean] True if batchable step returned NoBatches outcome
      def no_batches?
        @type == :no_batches
      end

      # Check if scenario is WithBatches
      #
      # @return [Boolean] True if batch workers were created
      def with_batches?
        @type == :with_batches
      end
    end
  end
end
