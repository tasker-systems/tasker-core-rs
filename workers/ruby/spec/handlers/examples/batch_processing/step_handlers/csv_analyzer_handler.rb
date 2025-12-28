# frozen_string_literal: true

require 'csv'

module TaskerCore
  module BatchProcessing
    module StepHandlers
      # CsvAnalyzerHandler - Batchable step that analyzes CSV file and decides batching strategy
      #
      # This handler:
      # 1. Counts total rows in CSV file
      # 2. Calculates optimal worker count based on batch size
      # 3. Returns NoBatches outcome if dataset too small
      # 4. Returns CreateBatches outcome with cursor configs if batching needed
      #
      # Expected task context:
      #   - csv_file_path: Path to CSV file to analyze
      #
      # Batch configuration (from handler initialization):
      #   - batch_size: 200 rows per worker
      #   - max_workers: 5 maximum parallel workers
      #
      # Returns:
      #   Success with batch_processing_outcome in result_data
      class CsvAnalyzerHandler < StepHandler::Batchable
        def call(context)
          csv_file_path = context.task.context['csv_file_path']
          raise ArgumentError, 'csv_file_path required in task context' if csv_file_path.nil?

          total_rows = count_csv_rows(csv_file_path)

          # Batch configuration (normally from handler initialization in YAML)
          batch_size = 200
          max_workers = 5
          worker_count = [(total_rows.to_f / batch_size).ceil, max_workers].min

          # If no batching needed, return NoBatches outcome
          if worker_count.zero? || total_rows.zero?
            return no_batches_outcome(
              reason: 'dataset_too_small',
              metadata: { 'total_rows' => total_rows }
            )
          end

          # Create cursor configs for batch workers
          cursor_configs = create_cursor_configs(total_rows, worker_count)
          create_batches_outcome(
            worker_template_name: 'process_csv_batch',
            cursor_configs: cursor_configs,
            total_items: total_rows,
            metadata: {
              'worker_count' => worker_count,
              'csv_file_path' => csv_file_path,
              'total_rows' => total_rows
            }
          )
        end

        private

        # Count total data rows in CSV (excluding header)
        #
        # TODO (SECURITY): In production, implement file size limits before loading CSV
        # This example uses CSV.read which loads entire file into memory.
        # Recommended limits:
        # - Max file size: 100 MB (configurable)
        # - Max row count: 1,000,000 rows (configurable)
        # Example implementation:
        #   max_size_bytes = 100 * 1024 * 1024  # 100 MB
        #   file_size = File.size(csv_file_path)
        #   raise ArgumentError, "CSV file too large (#{file_size} bytes)" if file_size > max_size_bytes
        #
        # @param csv_file_path [String] Path to CSV file
        # @return [Integer] Number of data rows
        def count_csv_rows(csv_file_path)
          # Basic file size check for example purposes
          # TODO: Make configurable in production
          max_size_bytes = 100 * 1024 * 1024 # 100 MB
          file_size = File.size(csv_file_path)

          if file_size > max_size_bytes
            raise ArgumentError,
                  "CSV file too large (#{file_size} bytes, max #{max_size_bytes} bytes)"
          end

          CSV.read(csv_file_path, headers: true).length
        end
      end
    end
  end
end
