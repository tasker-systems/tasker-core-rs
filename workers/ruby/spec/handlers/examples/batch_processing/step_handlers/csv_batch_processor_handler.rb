# frozen_string_literal: true

require 'csv'

module TaskerCore
  module BatchProcessing
    module StepHandlers
      # CsvBatchProcessorHandler - Batch worker that processes a specific range of CSV rows
      #
      # This handler:
      # 1. Extracts cursor config from step data
      # 2. Handles no-op placeholder scenarios
      # 3. Processes CSV rows in the specified range
      # 4. Calculates inventory metrics for processed rows
      #
      # Expected dependency results:
      #   - analyze_csv.result.csv_file_path: Path to CSV file
      #
      # Expected step data (from cursor config):
      #   - batch_id: Unique batch identifier
      #   - start_cursor: Starting row number (1-indexed)
      #   - end_cursor: Ending row number (exclusive)
      #
      # Returns:
      #   Success with inventory metrics and batch metadata
      class CsvBatchProcessorHandler < StepHandler::Batchable
        # Product struct for CSV parsing
        Product = Struct.new(
          :id, :title, :description, :category, :price,
          :discount_percentage, :rating, :stock, :brand, :sku, :weight,
          keyword_init: true
        )

        def call(context)
          # Extract cursor context from step data using new context-based API
          batch_context = get_batch_context(context)

          # Handle no-op placeholder scenario
          no_op_result = handle_no_op_worker(batch_context)
          return no_op_result if no_op_result

          # Get CSV file path from analyzer dependency using new context-based API
          csv_result = context.get_dependency_result('analyze_csv')
          csv_file_path = csv_result&.dig('csv_file_path') || csv_result&.dig(:csv_file_path)
          raise ArgumentError, 'csv_file_path not found in analyze_csv results' if csv_file_path.nil?

          # TODO: (SECURITY): In production, validate CSV file path to prevent path traversal
          # This is an example handler for testing, but production code should:
          # 1. Define allowed base directory for CSV files
          # 2. Validate path is within allowed directory using File.realpath
          # 3. Reject paths containing '..' or absolute paths outside allowed directory
          # Example: validate_csv_path(csv_file_path, allowed_dir: '/app/data/csv')
          validate_csv_path(csv_file_path)

          # Extract cursor range from batch context
          start_row = batch_context.start_cursor
          end_row = batch_context.end_cursor
          batch_id = batch_context.batch_id

          # Process CSV rows in range
          metrics = process_csv_range(csv_file_path, start_row, end_row)

          success(
            result: {
              'batch_id' => batch_id,
              'start_row' => start_row,
              'end_row' => end_row,
              'processed_count' => metrics[:processed_count],
              'total_inventory_value' => metrics[:total_inventory_value],
              'category_counts' => metrics[:category_counts],
              'max_price' => metrics[:max_price],
              'max_price_product' => metrics[:max_price_product],
              'average_rating' => metrics[:average_rating]
            }
          )
        end

        private

        # Process CSV rows in the specified range and calculate metrics
        #
        # @param csv_file_path [String] Path to CSV file
        # @param start_row [Integer] Starting row number (1-indexed, after header)
        # @param end_row [Integer] Ending row number (exclusive)
        # @return [Hash] Inventory metrics
        def process_csv_range(csv_file_path, start_row, end_row)
          processed_count = 0
          total_inventory_value = 0.0
          category_counts = Hash.new(0)
          max_price = 0.0
          max_price_product = nil
          total_rating = 0.0

          # Process CSV rows in range
          CSV.foreach(csv_file_path, headers: true).with_index(1) do |row, data_row_num|
            next if data_row_num < start_row
            break if data_row_num >= end_row

            product = parse_product(row)

            # Calculate inventory value (price * stock)
            inventory_value = product.price * product.stock
            total_inventory_value += inventory_value

            # Count products by category
            category_counts[product.category] += 1

            # Track maximum price product
            if product.price > max_price
              max_price = product.price
              max_price_product = product.title
            end

            # Accumulate rating for average calculation
            total_rating += product.rating

            processed_count += 1
          end

          average_rating = processed_count.positive? ? (total_rating / processed_count).round(2) : 0.0

          {
            processed_count: processed_count,
            total_inventory_value: total_inventory_value.round(2),
            category_counts: category_counts,
            max_price: max_price,
            max_price_product: max_price_product,
            average_rating: average_rating
          }
        end

        # Validate CSV file path to prevent path traversal attacks
        #
        # NOTE: This is a basic example implementation for testing purposes.
        # Production code should use a more robust validation with:
        # - Defined allowed base directory
        # - File.realpath to resolve symlinks and '..' references
        # - Explicit rejection of paths outside allowed directory
        #
        # @param file_path [String] Path to validate
        # @raise [ArgumentError] If path appears to be a path traversal attempt
        def validate_csv_path(file_path)
          # Basic validation for example purposes
          # TODO: Replace with production-grade path validation
          if file_path.include?('..')
            raise ArgumentError,
                  "Path traversal detected in CSV file path: #{file_path}"
          end

          # Check file exists and is readable
          unless File.exist?(file_path)
            raise ArgumentError,
                  "CSV file not found: #{file_path}"
          end

          return if File.readable?(file_path)

          raise ArgumentError,
                "CSV file not readable: #{file_path}"
        end

        # Parse a CSV row into a Product struct
        #
        # @param row [CSV::Row] CSV row data
        # @return [Product] Parsed product
        def parse_product(row)
          Product.new(
            id: row['id'].to_i,
            title: row['title'],
            description: row['description'],
            category: row['category'],
            price: row['price'].to_f,
            discount_percentage: row['discountPercentage'].to_f,
            rating: row['rating'].to_f,
            stock: row['stock'].to_i,
            brand: row['brand'],
            sku: row['sku'],
            weight: row['weight'].to_i
          )
        end
      end
    end
  end
end
