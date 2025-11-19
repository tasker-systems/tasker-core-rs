# TAS-59 Batch Processing - Ruby Implementation Plan

**Last Updated**: 2025-11-16
**Status**: READY FOR IMPLEMENTATION (Aligned with actual Ruby architecture)
**Foundation**: Rust implementation complete (see `rust-and-foundations.md`)

---

## Executive Summary

This document plans the Ruby implementation of TAS-59 batch processing, accurately mirroring the Rust patterns while following the **actual Ruby architecture patterns** established in the codebase.

### What We're Actually Building

Based on the completed Rust implementation and actual Ruby patterns:

✅ **Type system integration**: `BatchProcessingOutcome` type using dry-struct (like `DecisionPointOutcome`)
✅ **Helper modules**: `BatchWorkerContext` and `BatchAggregationScenario` in `lib/tasker_core/batch_processing/`
✅ **Example handlers**: In `spec/handlers/examples/batch_processing/` following established patterns
✅ **YAML templates**: In `tests/fixtures/task_templates/ruby/` using standard template structure
✅ **E2E tests**: In `spec/integration/` using RSpec patterns

### What We're NOT Building

The orchestration layer is **complete and shared**:
- ❌ No BatchProcessingService in Ruby (Rust handles this)
- ❌ No BatchProcessingActor in Ruby (Rust handles this)
- ❌ No new configuration (uses existing Rust TOML config)

### Architecture Alignment

Ruby implementation will:
- **Follow established Ruby patterns**: Use `TaskerCore::Types`, `StepHandler::Base`, model wrappers
- **Mirror Rust data structures**: Same `BatchProcessingOutcome` serialization format
- **Reuse Rust orchestration**: All worker creation handled by Rust `BatchProcessingService`
- **Use type system**: dry-struct for `BatchProcessingOutcome` (like `DecisionPointOutcome`)
- **Follow handler conventions**: Namespaced modules, `call(task, sequence, step)` signature
- **Use model wrappers**: `TaskSequenceStepWrapper`, `WorkflowStepWrapper`, etc.

---

## What Was Actually Built in Rust (Foundation)

From reading the actual Rust implementation:

### 1. Batchable Handler Pattern (Rust)

**Returns**: `BatchProcessingOutcome` in step result
**Location**: `batch_processing_outcome` field in result JSON

```rust
// Actual Rust pattern from batch_processing_products_csv.rs
let outcome = BatchProcessingOutcome::create_batches(
    "process_csv_batch".to_string(),
    worker_count as u32,
    cursor_configs,
    total_rows,
);

Ok(success_result(
    step_uuid,
    json!({
        "batch_processing_outcome": outcome.to_value(),  // ← Key field name
        "worker_count": worker_count,
        "total_rows": total_rows
    }),
    elapsed_ms,
    Some(metadata),
))
```

### 2. Batch Worker Pattern (Rust)

**Uses**: `BatchWorkerContext::from_step_data()` helper
**Accesses**: Cursor from `workflow_step.initialization` (NOT results initially)

```rust
// Actual Rust pattern from batch_processing_products_csv.rs
async fn call(&self, step_data: &TaskSequenceStep) -> Result<StepExecutionResult> {
    let context = BatchWorkerContext::from_step_data(step_data)?;

    if context.is_no_op() {
        return Ok(success_result(/*...*/));
    }

    let start_idx = context.start_position();
    let end_idx = context.end_position();

    // Process items in range...
}
```

### 3. Convergence Pattern (Rust)

**Uses**: `BatchAggregationScenario::detect()` helper
**Returns**: Either NoBatches or WithBatches scenario

```rust
// Actual Rust pattern from batch_processing_products_csv.rs
let scenario = BatchAggregationScenario::detect(
    &step_data.dependency_results,
    "analyze_csv",       // batchable step name
    "process_csv_batch_", // batch worker prefix
)?;

match scenario {
    BatchAggregationScenario::NoBatches { batchable_result } => {
        // Handle no batches case
    }
    BatchAggregationScenario::WithBatches { batch_results, worker_count } => {
        // Aggregate from batch_results iterator
    }
}
```

---

## Ruby Implementation Plan

### Phase 0: Type System Integration (Following DecisionPointOutcome Pattern)

#### 0.1 BatchProcessingOutcome Type

**File**: `workers/ruby/lib/tasker_core/types/batch_processing_outcome.rb` (new)

Mirrors `DecisionPointOutcome` pattern with dry-struct for type safety.

```ruby
# frozen_string_literal: true

require 'dry-struct'
require 'dry-types'

module TaskerCore
  module Types
    # BatchProcessingOutcome - TAS-59 Batch Processing Pattern
    #
    # Represents the outcome of a batchable step handler execution.
    # Batchable handlers analyze workload and decide whether to create
    # batch workers dynamically based on business logic.
    #
    # ## Usage Patterns
    #
    # ### No Batches (Dataset too small or empty)
    # ```ruby
    # BatchProcessingOutcome.no_batches
    # 
    #
    # ### Create Batch Workers
    # ```ruby
    # cursor_configs = [
    #   { 'batch_id' => '001', 'start_cursor' => 1, 'end_cursor' => 200, 'batch_size' => 200 },
    #   { 'batch_id' => '002', 'start_cursor' => 201, 'end_cursor' => 400, 'batch_size' => 200 }
    # ]
    # BatchProcessingOutcome.create_batches(
    #   worker_template_name: 'process_csv_batch',
    #   worker_count: 2,
    #   cursor_configs: cursor_configs,
    #   total_items: 400
    # )
    # 
    module BatchProcessingOutcome
      module Types
        include Dry.Types()
      end

      # NoBatches outcome - dataset too small or empty
      class NoBatches < Dry::Struct
        # Type discriminator (matches Rust serde tag field)
        attribute :type, Types::String.default('no_batches')

        # Convert to hash for serialization to Rust
        def to_h
          { 'type' => 'no_batches' }
        end

        def requires_batch_creation?
          false
        end
      end

      # CreateBatches outcome - create N batch workers with cursor configs
      class CreateBatches < Dry::Struct
        # Type discriminator
        attribute :type, Types::String.default('create_batches')

        # Worker template name from YAML
        attribute :worker_template_name, Types::Strict::String

        # Number of workers to create
        attribute :worker_count, Types::Coercible::Integer.constrained(gteq: 1)

        # Cursor configurations for each worker
        attribute :cursor_configs, Types::Array.of(Types::Hash).constrained(min_size: 1)

        # Total items in dataset
        attribute :total_items, Types::Coercible::Integer.constrained(gteq: 0)

        # Convert to hash for serialization to Rust
        def to_h
          {
            'type' => 'create_batches',
            'worker_template_name' => worker_template_name,
            'worker_count' => worker_count,
            'cursor_configs' => cursor_configs,
            'total_items' => total_items
          }
        end

        def requires_batch_creation?
          true
        end
      end

      # Factory methods
      class << self
        # Create a NoBatches outcome
        def no_batches
          NoBatches.new
        end

        # Create a CreateBatches outcome
        def create_batches(worker_template_name:, worker_count:, cursor_configs:, total_items:)
          CreateBatches.new(
            worker_template_name: worker_template_name,
            worker_count: worker_count,
            cursor_configs: cursor_configs,
            total_items: total_items
          )
        end

        # Parse from hash (for deserialization)
        # Uses ActiveSupport deep_symbolize_keys to normalize hash keys
        def from_hash(hash)
          return nil unless hash.is_a?(Hash)

          # Normalize to symbol keys using ActiveSupport
          symbolized = hash.deep_symbolize_keys

          case symbolized[:type]
          when 'no_batches'
            NoBatches.new
          when 'create_batches'
            CreateBatches.new(
              worker_template_name: symbolized[:worker_template_name],
              worker_count: symbolized[:worker_count],
              cursor_configs: symbolized[:cursor_configs],
              total_items: symbolized[:total_items]
            )
          else
            nil
          end
        end
      end
    end
  end
end
```

**File**: `workers/ruby/lib/tasker_core/types.rb` (update)

Add require statement:
```ruby
require_relative 'types/batch_processing_outcome' # TAS-59: batch processing outcomes
```

---

### Phase 1: Helper Modules (Mirror Rust Helpers)

#### 1.1 BatchWorkerContext Helper

**File**: `workers/ruby/lib/tasker_core/batch_processing/batch_worker_context.rb` (new)

Extracts cursor config from `workflow_step.initialization` using model wrappers.

```ruby
# frozen_string_literal: true

module TaskerCore
  module BatchProcessing
    # Ruby equivalent of Rust's BatchWorkerContext
    # Extracts cursor config from workflow_step.initialization via WorkflowStepWrapper
    class BatchWorkerContext
      attr_reader :cursor, :batch_metadata, :is_no_op

      # Extract context from TaskSequenceStepWrapper
      # @param sequence_step [TaskSequenceStepWrapper] Step execution context
      # @return [BatchWorkerContext] Extracted context
      def self.from_step_data(sequence_step)
        new(sequence_step.workflow_step)
      end

      def initialize(workflow_step)
        # Access initialization via WorkflowStepWrapper
        # Use ActiveSupport deep_symbolize_keys for clean key access
        initialization = (workflow_step.initialization || {}).deep_symbolize_keys

        @is_no_op = initialization[:is_no_op] == true

        if @is_no_op
          # Placeholder worker - minimal context
          @cursor = {}
          @batch_metadata = {}
        else
          @cursor = initialization[:cursor] || {}
          @batch_metadata = initialization[:batch_metadata] || {}

          validate_cursor!
        end
      end

      def start_position
        cursor[:start_cursor]&.to_i || 0
      end

      def end_position
        cursor[:end_cursor]&.to_i || 0
      end

      def batch_id
        cursor[:batch_id] || 'unknown'
      end

      def checkpoint_interval
        batch_metadata[:checkpoint_interval]&.to_i || 100
      end

      def no_op?
        @is_no_op
      end

      private

      def validate_cursor!
        return if @is_no_op

        raise ArgumentError, 'Missing cursor configuration' if cursor.empty?
        raise ArgumentError, 'Missing batch_id' unless cursor[:batch_id]
        raise ArgumentError, 'Missing start_cursor' unless cursor.key?(:start_cursor)
        raise ArgumentError, 'Missing end_cursor' unless cursor.key?(:end_cursor)
      end
    end
  end
end
```

#### 1.2 BatchAggregationScenario Helper

**File**: `workers/ruby/lib/tasker_core/batch_processing/batch_aggregation_scenario.rb` (new)

Uses `DependencyResultsWrapper` to access parent step results.

```ruby
# frozen_string_literal: true

module TaskerCore
  module BatchProcessing
    # Ruby equivalent of Rust's BatchAggregationScenario
    # Detects whether batchable step created workers or not
    class BatchAggregationScenario
      attr_reader :type, :batchable_result, :batch_results, :worker_count

      # Detect scenario from TaskSequenceStepWrapper
      # @param sequence_step [TaskSequenceStepWrapper] Step execution context
      # @param batchable_step_name [String] Name of batchable step
      # @param batch_worker_prefix [String] Prefix for batch worker step names
      # @return [BatchAggregationScenario] Detected scenario
      def self.detect(sequence_step, batchable_step_name, batch_worker_prefix)
        new(sequence_step.dependency_results, batchable_step_name, batch_worker_prefix)
      end

      def initialize(dependency_results, batchable_step_name, batch_worker_prefix)
        # Get full result hash for batchable step
        @batchable_result = dependency_results.get_result(batchable_step_name)

        # Find all batch worker results using DependencyResultsWrapper
        @batch_results = {}
        dependency_results.keys.each do |step_name|
          if step_name.start_with?(batch_worker_prefix)
            @batch_results[step_name] = dependency_results.get_result(step_name)
          end
        end

        if @batch_results.empty?
          @type = :no_batches
          @worker_count = 0
        else
          @type = :with_batches
          @worker_count = @batch_results.size
        end
      end

      def no_batches?
        @type == :no_batches
      end

      def with_batches?
        @type == :with_batches
      end
    end
  end
end
```

---

### Phase 2: Example Handlers (Following Actual Rust Patterns)

#### 2.1 CSV Product Analyzer (Batchable Handler)

**File**: `workers/ruby/spec/handlers/csv_product_handlers.rb` (new)

```ruby
# frozen_string_literal: true

require 'csv'

module BatchProcessing
  # CSV Analyzer - Batchable Step
  # Mirrors: workers/rust/src/step_handlers/batch_processing_products_csv.rs::CsvAnalyzerHandler
  class CsvAnalyzerHandler < TaskerCore::StepHandler::Base
    def call(task, _sequence, step)
      csv_file_path = task.context['csv_file_path']
      raise ArgumentError, 'Missing csv_file_path in task context' unless csv_file_path

      # Count CSV rows (excluding header)
      total_rows = count_csv_rows(csv_file_path)

      # Get batch configuration
      batch_size = step_definition_initialization['batch_size'] || 200
      max_workers = step_definition_initialization['max_workers'] || 5

      # Calculate worker count
      worker_count = [(total_rows.to_f / batch_size).ceil, max_workers].min

      if worker_count == 0 || total_rows == 0
        # No batches needed - return NoBatches outcome
        outcome = {
          'type' => 'no_batches'
        }

        return success(
          result_data: {
            'batch_processing_outcome' => outcome,
            'reason' => 'dataset_too_small',
            'total_rows' => total_rows
          }
        )
      end

      # Create cursor configs for workers
      cursor_configs = create_cursor_configs(total_rows, worker_count)

      # Return CreateBatches outcome (ACTUAL PATTERN)
      outcome = {
        'type' => 'create_batches',
        'worker_template_name' => 'process_csv_batch',
        'worker_count' => worker_count,
        'cursor_configs' => cursor_configs,
        'total_items' => total_rows
      }

      success(
        result_data: {
          'batch_processing_outcome' => outcome,  # ← Key field orchestration looks for
          'worker_count' => worker_count,
          'total_rows' => total_rows,
          'csv_file_path' => csv_file_path
        }
      )
    end

    private

    def count_csv_rows(csv_file_path)
      CSV.read(csv_file_path, headers: true).length
    end

    def create_cursor_configs(total_rows, worker_count)
      rows_per_worker = (total_rows.to_f / worker_count).ceil

      (0...worker_count).map do |i|
        start_row = (i * rows_per_worker) + 1  # 1-indexed after header
        end_row = [(start_row + rows_per_worker), total_rows + 1].min

        {
          'batch_id' => format('%03d', i + 1),
          'start_cursor' => start_row,
          'end_cursor' => end_row,
          'batch_size' => end_row - start_row
        }
      end
    end

    def step_definition_initialization
      # Access initialization from step definition (YAML handler.initialization section)
      # This is where batch configuration comes from
      @step_definition_initialization ||= {}
    end
  end

  # CSV Batch Processor - Batch Worker
  # Mirrors: workers/rust/src/step_handlers/batch_processing_products_csv.rs::CsvBatchProcessorHandler
  class CsvBatchProcessorHandler < TaskerCore::StepHandler::Base
    Product = Struct.new(
      :id, :title, :description, :category, :price,
      :discount_percentage, :rating, :stock, :brand, :sku, :weight,
      keyword_init: true
    )

    def call(task, sequence, step)
      # Use helper to extract context (ACTUAL RUST PATTERN)
      context = TaskerCore::BatchProcessing::BatchWorkerContext.from_step_data(step)

      # Check for no-op placeholder worker
      if context.no_op?
        return success(
          result_data: {
            'no_op' => true,
            'reason' => 'NoBatches scenario',
            'batch_id' => context.batch_id
          }
        )
      end

      # Get CSV file path from dependency results
      csv_file_path = sequence.dependency_results.dig('analyze_csv', 'result', 'csv_file_path')
      raise ArgumentError, 'Missing csv_file_path from analyze_csv' unless csv_file_path

      # Extract cursor range
      start_row = context.start_position
      end_row = context.end_position

      # Process CSV rows in cursor range
      metrics = process_csv_batch(csv_file_path, start_row, end_row)

      success(
        result_data: {
          'processed_count' => metrics[:processed_count],
          'total_inventory_value' => metrics[:total_inventory_value],
          'category_counts' => metrics[:category_counts],
          'max_price' => metrics[:max_price],
          'max_price_product' => metrics[:max_price_product],
          'average_rating' => metrics[:average_rating],
          'batch_id' => context.batch_id,
          'start_row' => start_row,
          'end_row' => end_row
        }
      )
    end

    private

    def process_csv_batch(csv_file_path, start_row, end_row)
      metrics = {
        processed_count: 0,
        total_inventory_value: 0.0,
        category_counts: Hash.new(0),
        max_price: 0.0,
        max_price_product: nil,
        ratings: []
      }

      CSV.foreach(csv_file_path, headers: true).with_index(1) do |row, data_row_num|
        # Skip rows before our range
        next if data_row_num < start_row
        # Break when we've processed all our rows
        break if data_row_num >= end_row

        product = parse_product(row)

        # Calculate inventory metrics (matching Rust logic)
        inventory_value = product.price * product.stock
        metrics[:total_inventory_value] += inventory_value

        metrics[:category_counts][product.category] += 1

        if product.price > metrics[:max_price]
          metrics[:max_price] = product.price
          metrics[:max_price_product] = product.title
        end

        metrics[:ratings] << product.rating
        metrics[:processed_count] += 1
      end

      # Calculate average rating
      metrics[:average_rating] = if metrics[:ratings].any?
                                  metrics[:ratings].sum / metrics[:ratings].size.to_f
                                else
                                  0.0
                                end

      metrics.except(:ratings)
    end

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

  # CSV Results Aggregator - Deferred Convergence Step
  # Mirrors: workers/rust/src/step_handlers/batch_processing_products_csv.rs::CsvResultsAggregatorHandler
  class CsvResultsAggregatorHandler < TaskerCore::StepHandler::Base
    def call(_task, sequence, _step)
      # Use helper to detect scenario (ACTUAL RUST PATTERN)
      scenario = TaskerCore::BatchProcessing::BatchAggregationScenario.detect(
        sequence.dependency_results,
        'analyze_csv',
        'process_csv_batch_'
      )

      if scenario.no_batches?
        # NoBatches scenario
        total_rows = scenario.batchable_result&.dig('result', 'total_rows') || 0

        return success(
          result_data: {
            'total_processed' => total_rows,
            'total_inventory_value' => 0.0,
            'category_counts' => {},
            'max_price' => 0.0,
            'max_price_product' => nil,
            'overall_average_rating' => 0.0,
            'worker_count' => 0
          }
        )
      end

      # WithBatches scenario - aggregate results
      aggregated = aggregate_batch_results(scenario.batch_results)

      success(
        result_data: {
          'total_processed' => aggregated[:total_processed],
          'total_inventory_value' => aggregated[:total_inventory_value],
          'category_counts' => aggregated[:category_counts],
          'max_price' => aggregated[:max_price],
          'max_price_product' => aggregated[:max_price_product],
          'overall_average_rating' => aggregated[:overall_average_rating],
          'worker_count' => scenario.worker_count
        }
      )
    end

    private

    def aggregate_batch_results(batch_results)
      total_processed = 0
      total_inventory_value = 0.0
      global_category_counts = Hash.new(0)
      max_price = 0.0
      max_price_product = nil
      all_ratings = []

      batch_results.each do |_step_name, batch_result|
        result = batch_result['result'] || {}

        total_processed += result['processed_count'] || 0
        total_inventory_value += result['total_inventory_value'] || 0.0

        # Merge category counts
        (result['category_counts'] || {}).each do |category, count|
          global_category_counts[category] += count
        end

        # Find global max price
        batch_max_price = result['max_price'] || 0.0
        if batch_max_price > max_price
          max_price = batch_max_price
          max_price_product = result['max_price_product']
        end

        # Collect ratings for weighted average
        batch_count = result['processed_count'] || 0
        batch_avg_rating = result['average_rating'] || 0.0
        all_ratings << { count: batch_count, avg: batch_avg_rating }
      end

      # Calculate overall weighted average rating
      total_items = all_ratings.sum { |r| r[:count] }
      overall_avg_rating = if total_items > 0
                            all_ratings.sum { |r| r[:avg] * r[:count] } / total_items.to_f
                          else
                            0.0
                          end

      {
        total_processed: total_processed,
        total_inventory_value: total_inventory_value,
        category_counts: global_category_counts,
        max_price: max_price,
        max_price_product: max_price_product,
        overall_average_rating: overall_avg_rating
      }
    end
  end
end
```

---

### Phase 3: YAML Templates

#### 3.1 CSV Product Processing Template

**File**: `tests/fixtures/task_templates/ruby/batch_processing_products_csv.yaml` (new)

```yaml
---
name: csv_product_inventory_analyzer_ruby
namespace_name: csv_processing
version: "1.0.0"
description: "Process CSV product data in parallel batches (Ruby implementation)"
task_handler:
  callable: ruby_ffi_handler
  initialization: {}

steps:
  # BATCHABLE STEP: CSV Analysis and Batch Planning
  - name: analyze_csv
    type: batchable
    dependencies: []
    handler:
      callable: BatchProcessing::CsvAnalyzerHandler
      initialization:
        batch_size: 200
        max_workers: 5

  # BATCH WORKER TEMPLATE: Single CSV Batch Processing
  # Orchestration creates N instances from this template
  - name: process_csv_batch
    type: batch_worker
    dependencies:
      - analyze_csv
    lifecycle:
      max_steps_in_process_minutes: 120
      max_retries: 3
      backoff_multiplier: 2.0
    handler:
      callable: BatchProcessing::CsvBatchProcessorHandler
      initialization:
        operation: "inventory_analysis"

  # DEFERRED CONVERGENCE STEP: CSV Results Aggregation
  - name: aggregate_csv_results
    type: deferred_convergence
    dependencies:
      - process_csv_batch  # Template dependency - resolves to all worker instances
    handler:
      callable: BatchProcessing::CsvResultsAggregatorHandler
      initialization:
        aggregation_type: "inventory_metrics"
```

---

### Phase 4: E2E Integration Tests (Language-Agnostic API Tests)

**IMPORTANT**: E2E tests are written in Rust and test via the tasker-client API. They are language-agnostic - they don't know or care what language the worker uses. This ensures the system works correctly regardless of worker implementation language.

#### 4.1 CSV Batch Processing E2E Test

**File**: `tests/e2e/ruby/batch_processing_csv_test.rs` (new)

Follows the pattern from `tests/e2e/ruby/conditional_approval_test.rs` - uses tasker-client library to test complete workflow execution.

```rust
// TAS-59: E2E Tests for Batch Processing with CSV
//
// This module tests batch processing workflows with actual CSV file I/O,
// demonstrating cursor-based parallel processing with Ruby handlers.
//
// The test is language-agnostic from the API perspective - it uses tasker-client
// to create tasks and verify results, regardless of worker implementation language.

use anyhow::Result;
use serde_json::json;
use uuid::Uuid;

use crate::common::integration_test_manager::IntegrationTestManager;
use crate::common::integration_test_utils::{create_task_request, wait_for_task_completion};

/// Helper function to create CSV batch processing task request
fn create_csv_processing_request(
    csv_file_path: &str,
    analysis_mode: &str,
) -> tasker_shared::models::core::task_request::TaskRequest {
    create_task_request(
        "csv_processing",
        "csv_product_inventory_analyzer_ruby",
        json!({
            "csv_file_path": csv_file_path,
            "analysis_mode": analysis_mode
        }),
    )
}

#[tokio::test]
async fn test_csv_batch_processing_with_ruby_handlers() -> Result<()> {
    println!("🚀 Starting CSV Batch Processing E2E Test (Ruby Handlers)");
    println!("   Processing: /app/tests/fixtures/products.csv (1000 data rows)");
    println!("   Expected: 5 batch workers processing 200 rows each");

    let manager = IntegrationTestManager::setup().await?;

    // Use Docker container path for CSV file
    let csv_file_path = "/app/tests/fixtures/products.csv";

    // Create task via API client (language-agnostic)
    let task_request = create_csv_processing_request(csv_file_path, "inventory");
    let response = manager
        .orchestration_client
        .create_task(task_request)
        .await?;

    println!("✅ Task created: {}", response.task_uuid);

    // Wait for completion (30 seconds timeout for CSV I/O)
    wait_for_task_completion(&manager.orchestration_client, &response.task_uuid, 30).await?;

    let task_uuid = Uuid::parse_str(&response.task_uuid)?;
    let task = manager.orchestration_client.get_task(task_uuid).await?;

    // Verify task completion
    assert!(
        task.is_execution_complete(),
        "Task should complete successfully"
    );
    println!("✅ Task execution complete: {}", task.execution_status);

    // Get workflow steps
    let steps = manager
        .orchestration_client
        .list_task_steps(task_uuid)
        .await?;

    // Verify expected steps exist
    let step_names: Vec<&str> = steps.iter().map(|s| s.name.as_str()).collect();
    assert!(
        step_names.contains(&"analyze_csv"),
        "Should have analyze_csv step"
    );
    assert!(
        step_names.contains(&"aggregate_csv_results"),
        "Should have aggregate_csv_results step"
    );

    // Count batch workers (should have 5 for 1000 rows with batch size 200)
    let batch_worker_count = step_names
        .iter()
        .filter(|name| name.starts_with("process_csv_batch_"))
        .count();

    println!("✅ Created {} batch workers", batch_worker_count);
    assert_eq!(
        batch_worker_count, 5,
        "Should have exactly 5 batch workers for 1000 rows"
    );

    // Verify all steps completed
    for step in &steps {
        assert_eq!(
            step.current_state.to_ascii_uppercase(),
            "COMPLETE",
            "Step {} should be completed",
            step.name
        );
    }

    // Verify aggregate results
    let aggregate_step = steps
        .iter()
        .find(|s| s.name == "aggregate_csv_results")
        .expect("Should have aggregate_csv_results step");

    let results = aggregate_step
        .results
        .as_ref()
        .expect("Aggregate step should have results");

    let result = results
        .get("result")
        .expect("Results should contain result object");

    // Verify CSV processing metrics
    let total_processed = result["total_processed"].as_u64().unwrap();
    println!("✅ Total products processed: {}", total_processed);
    assert_eq!(total_processed, 1000, "Should have processed all 1000 rows");

    let total_value = result["total_inventory_value"].as_f64().unwrap();
    println!("✅ Total inventory value: ${}", total_value);
    assert!(total_value > 0.0, "Total inventory value should be positive");

    let worker_count = result["worker_count"].as_u64().unwrap();
    assert_eq!(worker_count, 5, "Should have used 5 workers");

    println!("\n🎉 CSV Batch Processing E2E Test PASSED!");
    println!("✅ Language-agnostic API testing: Working");
    println!("✅ CSV analysis (Ruby handler): Working");
    println!("✅ Batch worker creation (Rust orchestration): Working");
    println!("✅ Parallel CSV processing (Ruby handlers): Working");
    println!("✅ Cursor-based row selection: Working");
    println!("✅ Deferred convergence aggregation: Working");

    Ok(())
}

#[tokio::test]
async fn test_no_batches_scenario() -> Result<()> {
    println!("🚀 Testing NoBatches scenario with empty CSV");

    let manager = IntegrationTestManager::setup().await?;

    // Create minimal CSV with header only (no data rows)
    let empty_csv_path = "/tmp/empty_products.csv";
    // Note: In real test, this file would be created by test setup

    let task_request = create_csv_processing_request(empty_csv_path, "inventory");
    let response = manager
        .orchestration_client
        .create_task(task_request)
        .await?;

    wait_for_task_completion(&manager.orchestration_client, &response.task_uuid, 10).await?;

    let task_uuid = Uuid::parse_str(&response.task_uuid)?;
    let steps = manager
        .orchestration_client
        .list_task_steps(task_uuid)
        .await?;

    // Verify no batch workers created in NoBatches scenario
    let batch_workers: Vec<_> = steps
        .iter()
        .filter(|s| s.name.starts_with("process_csv_batch_"))
        .collect();

    assert!(
        batch_workers.is_empty(),
        "Should have no batch workers in NoBatches scenario"
    );

    println!("✅ NoBatches scenario handled correctly");

    Ok(())
}
```

**File**: `tests/e2e/ruby/mod.rs` (update)

Add module declaration:
```rust
mod batch_processing_csv_test;
```

#### 4.2 RSpec Unit Tests (Optional - Ruby-Specific)

**File**: `workers/ruby/spec/batch_processing/batch_worker_context_spec.rb` (new)

RSpec tests are for unit testing the Ruby code itself (helpers, data transformations):

```ruby
# frozen_string_literal: true

require 'spec_helper'

RSpec.describe TaskerCore::BatchProcessing::BatchWorkerContext do
  describe '.from_step_data' do
    it 'extracts cursor config from workflow step initialization' do
      # Unit test for Ruby helper class
      step_data = double('WorkflowStep', initialization: {
        'cursor' => {
          'batch_id' => '001',
          'start_cursor' => 1,
          'end_cursor' => 200,
          'batch_size' => 200
        },
        'batch_metadata' => {
          'checkpoint_interval' => 100
        },
        'is_no_op' => false
      })

      context = described_class.new(step_data)

      expect(context.start_position).to eq(1)
      expect(context.end_position).to eq(200)
      expect(context.batch_id).to eq('001')
      expect(context.no_op?).to be false
    end
  end
end
```

**Note**: RSpec tests focus on Ruby code logic, not end-to-end workflow execution. E2E testing is handled by language-agnostic Rust tests.

---

### Phase 5: Handler Registration (Automatic - No Code Needed)

**IMPORTANT**: Example handlers in `spec/handlers/examples/` are **automatically loaded in test environment**. No manual registration needed!

#### How Handler Auto-Loading Works (Existing Pattern)

From `workers/ruby/lib/tasker_core/registry/handler_registry.rb:162-165`:

```ruby
# Check if test environment has already loaded handlers
if test_environment_active? && test_handlers_preloaded?
  logger.info('🧪 Test environment detected with preloaded handlers')
  registered_count = register_preloaded_handlers
end
```

And from lines 190-201:

```ruby
def load_example_handlers!
  # Load all example handler files from spec/handlers/examples/
  spec_dir = File.expand_path('../../../spec/handlers/examples', __dir__)
  return unless Dir.exist?(spec_dir)

  Dir.glob("#{spec_dir}/**/*_handler.rb").each do |handler_file|
    require handler_file
    logger.debug("✅ Loaded handler file: #{handler_file}")
  rescue StandardError => e
    logger.warn("❌ Failed to load handler file #{handler_file}: #{e.message}")
  end
end
```

#### What This Means for Batch Processing Handlers

✅ **No manual registration required** - handlers in `spec/handlers/examples/batch_processing/step_handlers/` are automatically:
1. Loaded by `Dir.glob("#{spec_dir}/**/*_handler.rb")`
2. Registered by `register_preloaded_handlers` using `ObjectSpace` to find handler classes
3. Available for template references like `BatchProcessing::StepHandlers::CsvAnalyzerHandler`

✅ **Example handlers are test-only** - they're in `spec/` not `lib/` because they're examples for the gem, not production code

✅ **YAML templates drive discovery** - templates reference handlers by full class name, registry finds and loads them

---

### Phase 6: Docker Configuration

**File**: `docker/docker-compose.test.yml`

Ensure test fixtures are mounted:

```yaml
ruby-worker:
  volumes:
    - ../tests/fixtures:/app/tests/fixtures:ro
    - ../tests/fixtures/task_templates/ruby:/app/tests/fixtures/task_templates/ruby:ro
```

---

## Implementation Checklist

### Phase 0: Type System Integration (0.5 day)
- [ ] Create `BatchProcessingOutcome` type in `lib/tasker_core/types/batch_processing_outcome.rb`
- [ ] Add require statement to `lib/tasker_core/types.rb`
- [ ] Unit test `BatchProcessingOutcome` type creation and serialization

### Phase 1: Helper Modules (0.5 day)
- [ ] Create `BatchWorkerContext` helper in `lib/tasker_core/batch_processing/batch_worker_context.rb`
- [ ] Create `BatchAggregationScenario` helper in `lib/tasker_core/batch_processing/batch_aggregation_scenario.rb`
- [ ] Write RSpec unit tests for helpers in `spec/batch_processing/`

### Phase 2: CSV Example Handlers (1.5 days)
- [ ] Create handler directory: `spec/handlers/examples/batch_processing/step_handlers/`
- [ ] Create `CsvAnalyzerHandler` (batchable) using `StepHandlerCallResult` and `BatchProcessingOutcome`
- [ ] Create `CsvBatchProcessorHandler` (batch_worker) using `BatchWorkerContext` helper
- [ ] Create `CsvResultsAggregatorHandler` (deferred_convergence) using `BatchAggregationScenario` helper
- [ ] Test handlers locally with sample CSV

### Phase 3: YAML Template (0.5 day)
- [ ] Create `tests/fixtures/task_templates/ruby/batch_processing_products_csv.yaml`
- [ ] Use proper handler callable format: `BatchProcessing::StepHandlers::CsvAnalyzerHandler`
- [ ] Validate template syntax
- [ ] Test template loading with handler registry

### Phase 4: E2E Integration Tests (1 day)
- [ ] Create `tests/e2e/ruby/batch_processing_csv_test.rs`
- [ ] Write `test_csv_batch_processing_with_ruby_handlers()` test
- [ ] Write `test_no_batches_scenario()` test
- [ ] Add module to `tests/e2e/ruby/mod.rs`
- [ ] Test with Docker Compose services (`docker-compose -f docker/docker-compose.test.yml up`)
- [ ] Verify all assertions pass

### Phase 5: Verify Handler Auto-Loading (0.5 day)
- [ ] Verify handlers are auto-loaded by `Dir.glob` pattern
- [ ] Confirm `register_preloaded_handlers` finds handlers via `ObjectSpace`
- [ ] Test template references resolve to handler classes
- [ ] Verify no manual registration code needed

### Phase 6: Documentation (0.5 day)
- [ ] Add RDoc comments to type classes and helpers
- [ ] Document handler usage patterns
- [ ] Update Ruby worker README with batch processing examples
- [ ] Document auto-loading pattern for example handlers

**Total Timeline**: ~4 days

**Key Architecture Points**:
- ✅ Types use dry-struct (like `DecisionPointOutcome`)
- ✅ Helpers use `deep_symbolize_keys` for clean hash access
- ✅ Handlers use `TaskSequenceStepWrapper` and model wrappers
- ✅ RSpec for unit tests, Rust for E2E tests (language-agnostic)
- ✅ Handler registry auto-discovers from YAML templates

---

## Key Differences: Ruby vs Rust

### Data Access Patterns

**Rust**:
```rust
let context = BatchWorkerContext::from_step_data(step_data)?;
let start = context.start_position();
```

**Ruby**:
```ruby
context = TaskerCore::BatchProcessing::BatchWorkerContext.from_step_data(step)
start = context.start_position
```

### Scenario Detection

**Rust**:
```rust
let scenario = BatchAggregationScenario::detect(
    &step_data.dependency_results,
    "analyze_csv",
    "process_csv_batch_"
)?;
```

**Ruby**:
```ruby
scenario = TaskerCore::BatchProcessing::BatchAggregationScenario.detect(
  sequence.dependency_results,
  'analyze_csv',
  'process_csv_batch_'
)
```

### Return Values

**Both languages** return same structure:
```json
{
  "batch_processing_outcome": {
    "type": "create_batches",
    "worker_template_name": "process_csv_batch",
    "worker_count": 5,
    "cursor_configs": [...],
    "total_items": 1000
  }
}
```

---

## What We're NOT Building (Already Complete in Rust)

❌ **BatchProcessingService** - Rust orchestration handles worker creation
❌ **BatchProcessingActor** - Rust actor pattern handles messages
❌ **Worker Template Instantiation** - Rust service creates N instances
❌ **DAG Edge Management** - Rust service creates edges
❌ **Convergence Step Discovery** - Rust handles template → instance resolution
❌ **Configuration** - Uses existing Rust TOML configuration

---

## Success Criteria

✅ Ruby handlers return `BatchProcessingOutcome` matching Rust pattern
✅ Helpers mirror Rust `BatchWorkerContext` and `BatchAggregationScenario` behavior
✅ CSV processing test processes 1000 rows with 5 workers
✅ NoBatches scenario handled correctly
✅ All E2E tests pass with Docker Compose services
✅ Results match Rust implementation output format
✅ Code follows Ruby idioms and conventions
✅ Documentation complete with examples

---

## Related Documentation

- **[Rust Implementation](./rust-and-foundations.md)** - Foundation and patterns to mirror
- **[Simplified Plan](./plan.md)** - Post-TAS-49 simplified design
- **[Original Spec](./original-spec.md)** - Pre-TAS-49 comprehensive design

---

## Summary

The Ruby implementation is straightforward because:
1. **Orchestration is complete** - Rust `BatchProcessingService` handles all worker creation
2. **Patterns are proven** - Rust implementation shows exactly what works
3. **Helpers are simple** - Extract data from step initialization, detect scenarios
4. **Focus is narrow** - Only Ruby handlers and tests, no infrastructure

Ruby developers need to:
1. Return `BatchProcessingOutcome` from batchable handlers
2. Use `BatchWorkerContext` helper to extract cursor config
3. Use `BatchAggregationScenario` helper in convergence steps
4. Follow exact patterns from working Rust examples

Timeline: ~4 days vs original 7 days estimate, because we're only building handlers and tests, not infrastructure.
