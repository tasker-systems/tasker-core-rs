"""Batchable mixin and batch processing tests.

These tests verify:
- Batch processing types (CursorConfig, BatchAnalyzerOutcome, etc.)
- Batchable mixin cursor and batch creation
- Batchable with StepHandler integration
- Worker result aggregation
"""

from __future__ import annotations


class TestBatchTypes:
    """Test batch processing types."""

    def test_cursor_config(self):
        """Test CursorConfig model."""
        from tasker_core import CursorConfig

        config = CursorConfig(
            start_cursor=0,
            end_cursor=100,
            step_size=10,
            metadata={"partition": "A"},
        )
        assert config.start_cursor == 0
        assert config.end_cursor == 100
        assert config.step_size == 10
        assert config.metadata["partition"] == "A"

    def test_cursor_config_defaults(self):
        """Test CursorConfig with defaults."""
        from tasker_core import CursorConfig

        config = CursorConfig(start_cursor=0, end_cursor=50)
        assert config.step_size == 1
        assert config.metadata == {}

    def test_batch_analyzer_outcome(self):
        """Test BatchAnalyzerOutcome model."""
        from tasker_core import BatchAnalyzerOutcome, CursorConfig

        configs = [
            CursorConfig(start_cursor=0, end_cursor=100),
            CursorConfig(start_cursor=100, end_cursor=200),
        ]
        outcome = BatchAnalyzerOutcome(
            cursor_configs=configs,
            total_items=200,
            batch_metadata={"source": "database"},
        )
        assert len(outcome.cursor_configs) == 2
        assert outcome.total_items == 200
        assert outcome.batch_metadata["source"] == "database"

    def test_batch_analyzer_outcome_from_ranges(self):
        """Test BatchAnalyzerOutcome.from_ranges factory."""
        from tasker_core import BatchAnalyzerOutcome

        outcome = BatchAnalyzerOutcome.from_ranges(
            ranges=[(0, 100), (100, 200), (200, 300)],
            step_size=10,
            total_items=300,
            batch_metadata={"version": "1"},
        )
        assert len(outcome.cursor_configs) == 3
        assert outcome.cursor_configs[0].start_cursor == 0
        assert outcome.cursor_configs[0].end_cursor == 100
        assert outcome.cursor_configs[0].step_size == 10
        assert outcome.total_items == 300

    def test_batch_worker_context(self):
        """Test BatchWorkerContext model."""
        from tasker_core import BatchWorkerContext, CursorConfig

        config = CursorConfig(start_cursor=0, end_cursor=100, step_size=5)
        context = BatchWorkerContext(
            batch_id="batch_001",
            cursor_config=config,
            batch_index=0,
            total_batches=10,
            batch_metadata={"source": "test"},
        )
        assert context.batch_id == "batch_001"
        assert context.start_cursor == 0
        assert context.end_cursor == 100
        assert context.step_size == 5
        assert context.batch_index == 0
        assert context.total_batches == 10

    def test_batch_worker_outcome(self):
        """Test BatchWorkerOutcome model."""
        from tasker_core import BatchWorkerOutcome

        outcome = BatchWorkerOutcome(
            items_processed=100,
            items_succeeded=95,
            items_failed=5,
            items_skipped=0,
            results=[{"id": i} for i in range(5)],
            errors=[{"id": i, "error": "failed"} for i in range(5)],
            last_cursor=99,
        )
        assert outcome.items_processed == 100
        assert outcome.items_succeeded == 95
        assert outcome.items_failed == 5
        assert len(outcome.results) == 5
        assert len(outcome.errors) == 5
        assert outcome.last_cursor == 99


class TestBatchable:
    """Test Batchable mixin."""

    def test_batchable_create_cursor_config(self):
        """Test Batchable.create_cursor_config."""
        from tasker_core import Batchable

        class TestHandler(Batchable):
            pass

        handler = TestHandler()
        config = handler.create_cursor_config(0, 100, step_size=5)
        assert config.start_cursor == 0
        assert config.end_cursor == 100
        assert config.step_size == 5

    def test_batchable_create_cursor_ranges(self):
        """Test Batchable.create_cursor_ranges."""
        from tasker_core import Batchable

        class TestHandler(Batchable):
            pass

        handler = TestHandler()
        configs = handler.create_cursor_ranges(total_items=250, batch_size=100)
        assert len(configs) == 3
        assert configs[0].start_cursor == 0
        assert configs[0].end_cursor == 100
        assert configs[1].start_cursor == 100
        assert configs[1].end_cursor == 200
        assert configs[2].start_cursor == 200
        assert configs[2].end_cursor == 250

    def test_batchable_create_batch_outcome(self):
        """Test Batchable.create_batch_outcome."""
        from tasker_core import Batchable

        class TestHandler(Batchable):
            pass

        handler = TestHandler()
        outcome = handler.create_batch_outcome(
            total_items=500,
            batch_size=100,
            batch_metadata={"source": "test"},
        )
        assert len(outcome.cursor_configs) == 5
        assert outcome.total_items == 500
        assert outcome.batch_metadata["source"] == "test"

    def test_batchable_create_cursor_ranges_respects_max_batches(self):
        """Test Batchable.create_cursor_ranges adjusts batch size to respect max_batches."""
        from tasker_core import Batchable

        class TestHandler(Batchable):
            pass

        handler = TestHandler()

        # Test: 10,000 items with batch_size=100 would create 100 batches
        # With max_batches=10, it should adjust batch_size to 1000
        configs = handler.create_cursor_ranges(
            total_items=10000,
            batch_size=100,
            max_batches=10,
        )

        assert len(configs) == 10, "Should create exactly max_batches batches"
        # Each batch should handle 1000 items (10000 / 10)
        assert configs[0].start_cursor == 0
        assert configs[0].end_cursor == 1000
        assert configs[1].start_cursor == 1000
        assert configs[1].end_cursor == 2000
        assert configs[9].start_cursor == 9000
        assert configs[9].end_cursor == 10000

    def test_batchable_create_cursor_ranges_max_batches_large_items(self):
        """Test create_cursor_ranges with large total items respects max_batches limit."""
        from tasker_core import Batchable

        class TestHandler(Batchable):
            pass

        handler = TestHandler()

        # Test: 1,000,000 items with batch_size=1000 would create 1000 batches
        # With max_batches=50, it should adjust batch_size to 20000
        configs = handler.create_cursor_ranges(
            total_items=1000000,
            batch_size=1000,
            max_batches=50,
        )

        assert len(configs) == 50, "Should create exactly max_batches batches"
        # Each batch should handle 20000 items (1000000 / 50)
        assert configs[0].start_cursor == 0
        assert configs[0].end_cursor == 20000
        assert configs[1].start_cursor == 20000
        assert configs[1].end_cursor == 40000
        assert configs[49].start_cursor == 980000
        assert configs[49].end_cursor == 1000000

    def test_batchable_create_cursor_ranges_max_batches_no_adjustment_needed(self):
        """Test create_cursor_ranges when max_batches doesn't require adjustment."""
        from tasker_core import Batchable

        class TestHandler(Batchable):
            pass

        handler = TestHandler()

        # Test: 500 items with batch_size=100 creates 5 batches
        # With max_batches=10, no adjustment needed (5 < 10)
        configs = handler.create_cursor_ranges(
            total_items=500,
            batch_size=100,
            max_batches=10,
        )

        assert len(configs) == 5, "Should create 5 batches (no adjustment needed)"
        assert configs[0].end_cursor - configs[0].start_cursor == 100
        assert configs[4].end_cursor - configs[4].start_cursor == 100

    def test_batchable_create_cursor_ranges_max_batches_none(self):
        """Test create_cursor_ranges with max_batches=None uses default behavior."""
        from tasker_core import Batchable

        class TestHandler(Batchable):
            pass

        handler = TestHandler()

        # Test: With max_batches=None, should create batches based on batch_size
        configs = handler.create_cursor_ranges(
            total_items=10000,
            batch_size=100,
            max_batches=None,
        )

        assert len(configs) == 100, "Should create 100 batches (10000/100)"
        assert configs[0].end_cursor - configs[0].start_cursor == 100

    def test_batchable_create_worker_outcome(self):
        """Test Batchable.create_worker_outcome."""
        from tasker_core import Batchable

        class TestHandler(Batchable):
            pass

        handler = TestHandler()
        outcome = handler.create_worker_outcome(
            items_processed=100,
            items_succeeded=95,
            items_failed=5,
            last_cursor=99,
        )
        assert outcome.items_processed == 100
        assert outcome.items_succeeded == 95
        assert outcome.items_failed == 5
        assert outcome.last_cursor == 99

    def test_batchable_aggregate_worker_results(self):
        """Test Batchable.aggregate_worker_results."""
        from tasker_core import Batchable

        results = [
            {
                "items_processed": 100,
                "items_succeeded": 95,
                "items_failed": 5,
                "errors": [{"id": 1, "error": "fail"}],
            },
            {
                "items_processed": 100,
                "items_succeeded": 98,
                "items_failed": 2,
                "errors": [{"id": 2, "error": "fail"}],
            },
            {
                "items_processed": 50,
                "items_succeeded": 50,
                "items_failed": 0,
            },
        ]
        summary = Batchable.aggregate_worker_results(results)
        assert summary["total_processed"] == 250
        assert summary["total_succeeded"] == 243
        assert summary["total_failed"] == 7
        assert summary["batch_count"] == 3
        assert len(summary["errors"]) == 2


class TestBatchableWithStepHandler:
    """Test Batchable mixin with StepHandler."""

    def test_batchable_analyzer_success(self):
        """Test batch_analyzer_success method."""
        from tasker_core import Batchable, StepHandler

        class TestAnalyzer(StepHandler, Batchable):
            handler_name = "test_analyzer"

            def call(self, _context):
                outcome = self.create_batch_outcome(total_items=100, batch_size=25)
                return self.batch_analyzer_success(outcome)

        handler = TestAnalyzer()
        outcome = handler.create_batch_outcome(total_items=100, batch_size=25)
        result = handler.batch_analyzer_success(outcome)

        assert result.is_success is True
        # Result now uses batch_processing_outcome wrapper (matches Rust expectations)
        assert result.result["worker_count"] == 4
        assert result.result["total_items"] == 100
        assert len(result.result["batch_processing_outcome"]["cursor_configs"]) == 4
        assert result.result["batch_processing_outcome"]["type"] == "create_batches"
        assert result.metadata.get("batch_analyzer") is True

    def test_batchable_worker_success(self):
        """Test batch_worker_success method."""
        from tasker_core import Batchable, StepHandler

        class TestWorker(StepHandler, Batchable):
            handler_name = "test_worker"

            def call(self, _context):
                outcome = self.create_worker_outcome(
                    items_processed=25,
                    items_succeeded=24,
                    items_failed=1,
                )
                return self.batch_worker_success(outcome)

        handler = TestWorker()
        outcome = handler.create_worker_outcome(
            items_processed=25,
            items_succeeded=24,
            items_failed=1,
        )
        result = handler.batch_worker_success(outcome)

        assert result.is_success is True
        assert result.result["items_processed"] == 25
        assert result.result["items_succeeded"] == 24
        assert result.result["items_failed"] == 1
        assert result.metadata.get("batch_worker") is True


class TestBatchAggregationScenario:
    """Test BatchAggregationScenario detection (TAS-112)."""

    def test_detect_no_batches_scenario(self):
        """Test detection of NoBatches scenario from batch_processing_outcome."""
        from tasker_core import BatchAggregationScenario

        dependency_results = {
            "analyze_csv": {
                "batch_processing_outcome": {"type": "no_batches"},
                "reason": "empty_dataset",
            }
        }
        scenario = BatchAggregationScenario.detect(
            dependency_results, "analyze_csv", "process_csv_batch_"
        )

        assert scenario.is_no_batches is True
        assert scenario.no_batches() is True
        assert scenario.worker_count == 0
        assert scenario.batch_results == {}
        assert scenario.batchable_result["reason"] == "empty_dataset"

    def test_detect_with_batches_scenario(self):
        """Test detection of WithBatches scenario."""
        from tasker_core import BatchAggregationScenario

        dependency_results = {
            "analyze_csv": {
                "batch_processing_outcome": {
                    "type": "create_batches",
                    "worker_count": 3,
                },
            },
            "process_csv_batch_001": {"items_processed": 100, "items_succeeded": 98},
            "process_csv_batch_002": {"items_processed": 100, "items_succeeded": 100},
            "process_csv_batch_003": {"items_processed": 50, "items_succeeded": 50},
        }
        scenario = BatchAggregationScenario.detect(
            dependency_results, "analyze_csv", "process_csv_batch_"
        )

        assert scenario.is_no_batches is False
        assert scenario.worker_count == 3
        assert len(scenario.batch_results) == 3
        assert "process_csv_batch_001" in scenario.batch_results
        assert scenario.batch_results["process_csv_batch_002"]["items_succeeded"] == 100

    def test_detect_missing_batchable_step_raises_error(self):
        """Test that missing batchable step raises ValueError."""
        import pytest

        from tasker_core import BatchAggregationScenario

        dependency_results = {
            "process_csv_batch_001": {"items_processed": 100},
        }
        with pytest.raises(ValueError, match="Missing batchable step dependency"):
            BatchAggregationScenario.detect(dependency_results, "analyze_csv", "process_csv_batch_")

    def test_detect_no_workers_without_no_batches_raises_error(self):
        """Test that missing workers without NoBatches outcome raises ValueError."""
        import pytest

        from tasker_core import BatchAggregationScenario

        dependency_results = {
            "analyze_csv": {
                "batch_processing_outcome": {
                    "type": "create_batches",
                    "worker_count": 3,
                },
            },
            # No batch workers present
        }
        with pytest.raises(ValueError, match="No batch workers found"):
            BatchAggregationScenario.detect(dependency_results, "analyze_csv", "process_csv_batch_")

    def test_detect_with_wrapped_result(self):
        """Test detection handles wrapped result dicts.

        The actual FFI data comes from the database as dicts with a 'result' key:
        {"result": {...actual_data...}, "error": null, "metadata": {...}}

        DependencyResultsWrapper.get_results() handles this by extracting the inner
        'result' value, matching Ruby's get_results() behavior.
        """
        from tasker_core import BatchAggregationScenario

        # Simulate actual wrapped results from FFI (dict with "result" key)
        dependency_results = {
            "analyze_csv": {
                "result": {
                    "batch_processing_outcome": {"type": "no_batches"},
                },
                "error": None,
                "metadata": {},
            },
        }
        scenario = BatchAggregationScenario.detect(
            dependency_results, "analyze_csv", "process_csv_batch_"
        )

        assert scenario.is_no_batches is True


class TestBatchableAggregationHelpers:
    """Test Batchable aggregation helper methods (TAS-112)."""

    def test_detect_aggregation_scenario_method(self):
        """Test Batchable.detect_aggregation_scenario method."""
        from tasker_core import Batchable, StepHandler

        class TestAggregator(StepHandler, Batchable):
            handler_name = "test_aggregator"

            def call(self, _context):
                pass

        handler = TestAggregator()
        dependency_results = {
            "analyze_csv": {
                "batch_processing_outcome": {"type": "no_batches"},
            }
        }
        scenario = handler.detect_aggregation_scenario(
            dependency_results, "analyze_csv", "process_csv_batch_"
        )

        assert scenario.is_no_batches is True

    def test_no_batches_aggregation_result(self):
        """Test Batchable.no_batches_aggregation_result method."""
        from tasker_core import Batchable, StepHandler

        class TestAggregator(StepHandler, Batchable):
            handler_name = "test_aggregator"

            def call(self, _context):
                pass

        handler = TestAggregator()
        result = handler.no_batches_aggregation_result(
            zero_metrics={"total_processed": 0, "total_value": 0.0}
        )

        assert result.is_success is True
        assert result.result["worker_count"] == 0
        assert result.result["scenario"] == "no_batches"
        assert result.result["total_processed"] == 0
        assert result.result["total_value"] == 0.0

    def test_aggregate_batch_worker_results_no_batches(self):
        """Test aggregate_batch_worker_results for NoBatches scenario."""
        from tasker_core import Batchable, BatchAggregationScenario, StepHandler

        class TestAggregator(StepHandler, Batchable):
            handler_name = "test_aggregator"

            def call(self, _context):
                pass

        handler = TestAggregator()
        scenario = BatchAggregationScenario(
            is_no_batches=True,
            batchable_result={"reason": "empty"},
            batch_results={},
            worker_count=0,
        )
        result = handler.aggregate_batch_worker_results(scenario, zero_metrics={"total": 0})

        assert result.is_success is True
        assert result.result["worker_count"] == 0
        assert result.result["scenario"] == "no_batches"
        assert result.result["total"] == 0

    def test_aggregate_batch_worker_results_with_batches(self):
        """Test aggregate_batch_worker_results for WithBatches scenario."""
        from tasker_core import Batchable, BatchAggregationScenario, StepHandler

        class TestAggregator(StepHandler, Batchable):
            handler_name = "test_aggregator"

            def call(self, _context):
                pass

        handler = TestAggregator()
        scenario = BatchAggregationScenario(
            is_no_batches=False,
            batchable_result={"total_items": 250},
            batch_results={
                "process_batch_001": {"count": 100},
                "process_batch_002": {"count": 100},
                "process_batch_003": {"count": 50},
            },
            worker_count=3,
        )

        def aggregate_fn(batch_results):
            total = sum(r.get("count", 0) for r in batch_results.values())
            return {"total_processed": total}

        result = handler.aggregate_batch_worker_results(
            scenario,
            zero_metrics={"total_processed": 0},
            aggregation_fn=aggregate_fn,
        )

        assert result.is_success is True
        assert result.result["worker_count"] == 3
        assert result.result["scenario"] == "with_batches"
        assert result.result["total_processed"] == 250

    def test_aggregate_batch_worker_results_default_aggregation(self):
        """Test aggregate_batch_worker_results with no aggregation function."""
        from tasker_core import Batchable, BatchAggregationScenario, StepHandler

        class TestAggregator(StepHandler, Batchable):
            handler_name = "test_aggregator"

            def call(self, _context):
                pass

        handler = TestAggregator()
        scenario = BatchAggregationScenario(
            is_no_batches=False,
            batchable_result={},
            batch_results={
                "process_batch_001": {"count": 100},
                "process_batch_002": {"count": 50},
            },
            worker_count=2,
        )

        result = handler.aggregate_batch_worker_results(scenario)

        assert result.is_success is True
        assert result.result["worker_count"] == 2
        assert result.result["scenario"] == "with_batches"
        # Default aggregation passes through batch_results
        assert "batch_results" in result.result
        assert len(result.result["batch_results"]) == 2


class TestCheckpointYield:
    """TAS-125: Test checkpoint_yield for batch processing resumption."""

    def test_checkpoint_yield_with_integer_cursor(self):
        """Test checkpoint_yield with integer cursor."""
        from tasker_core import Batchable, StepHandler

        class TestWorker(StepHandler, Batchable):
            handler_name = "test_checkpoint_worker"

            def call(self, _context):
                pass

        handler = TestWorker()
        result = handler.checkpoint_yield(cursor=5000, items_processed=5000)

        assert result.is_success is True
        assert result.result["type"] == "checkpoint_yield"
        assert result.result["cursor"] == 5000
        assert result.result["items_processed"] == 5000
        assert result.metadata.get("checkpoint_yield") is True
        assert result.metadata.get("batch_worker") is True

    def test_checkpoint_yield_with_string_cursor(self):
        """Test checkpoint_yield with string cursor (pagination token)."""
        from tasker_core import Batchable, StepHandler

        class TestWorker(StepHandler, Batchable):
            handler_name = "test_checkpoint_worker"

            def call(self, _context):
                pass

        handler = TestWorker()
        result = handler.checkpoint_yield(
            cursor="eyJsYXN0X2lkIjoiOTk5In0=",
            items_processed=100,
        )

        assert result.is_success is True
        assert result.result["cursor"] == "eyJsYXN0X2lkIjoiOTk5In0="
        assert result.result["items_processed"] == 100

    def test_checkpoint_yield_with_dict_cursor(self):
        """Test checkpoint_yield with dict cursor (complex pagination)."""
        from tasker_core import Batchable, StepHandler

        class TestWorker(StepHandler, Batchable):
            handler_name = "test_checkpoint_worker"

            def call(self, _context):
                pass

        handler = TestWorker()
        complex_cursor = {"page": 5, "partition": "A", "last_timestamp": "2024-01-15T10:00:00Z"}
        result = handler.checkpoint_yield(
            cursor=complex_cursor,
            items_processed=250,
        )

        assert result.is_success is True
        assert result.result["cursor"] == complex_cursor
        assert result.result["cursor"]["page"] == 5

    def test_checkpoint_yield_with_accumulated_results(self):
        """Test checkpoint_yield with accumulated results."""
        from tasker_core import Batchable, StepHandler

        class TestWorker(StepHandler, Batchable):
            handler_name = "test_checkpoint_worker"

            def call(self, _context):
                pass

        handler = TestWorker()
        result = handler.checkpoint_yield(
            cursor=7500,
            items_processed=7500,
            accumulated_results={"running_total": 375000.50, "processed_count": 7500},
        )

        assert result.is_success is True
        assert result.result["accumulated_results"]["running_total"] == 375000.50
        assert result.result["accumulated_results"]["processed_count"] == 7500

    def test_checkpoint_yield_without_accumulated_results(self):
        """Test checkpoint_yield without accumulated results."""
        from tasker_core import Batchable, StepHandler

        class TestWorker(StepHandler, Batchable):
            handler_name = "test_checkpoint_worker"

            def call(self, _context):
                pass

        handler = TestWorker()
        result = handler.checkpoint_yield(cursor=1000, items_processed=1000)

        assert result.is_success is True
        assert "accumulated_results" not in result.result


class TestCheckpointYieldErrorScenarios:
    """TAS-125: Error scenario tests for checkpoint_yield."""

    def test_checkpoint_yield_with_zero_items_processed(self):
        """Test checkpoint_yield with zero items processed (edge case)."""
        from tasker_core import Batchable, StepHandler

        class TestWorker(StepHandler, Batchable):
            handler_name = "test_checkpoint_worker"

            def call(self, _context):
                pass

        handler = TestWorker()
        # Zero items is valid - it's an initialization checkpoint
        result = handler.checkpoint_yield(cursor=0, items_processed=0)

        assert result.is_success is True
        assert result.result["items_processed"] == 0
        assert result.result["cursor"] == 0

    def test_checkpoint_yield_with_none_cursor(self):
        """Test checkpoint_yield with None cursor (edge case)."""
        from tasker_core import Batchable, StepHandler

        class TestWorker(StepHandler, Batchable):
            handler_name = "test_checkpoint_worker"

            def call(self, _context):
                pass

        handler = TestWorker()
        # None cursor should be allowed (represents "beginning of data")
        result = handler.checkpoint_yield(cursor=None, items_processed=0)

        assert result.is_success is True
        assert result.result["cursor"] is None

    def test_checkpoint_yield_with_empty_dict_cursor(self):
        """Test checkpoint_yield with empty dict cursor."""
        from tasker_core import Batchable, StepHandler

        class TestWorker(StepHandler, Batchable):
            handler_name = "test_checkpoint_worker"

            def call(self, _context):
                pass

        handler = TestWorker()
        result = handler.checkpoint_yield(cursor={}, items_processed=100)

        assert result.is_success is True
        assert result.result["cursor"] == {}

    def test_checkpoint_yield_with_empty_string_cursor(self):
        """Test checkpoint_yield with empty string cursor."""
        from tasker_core import Batchable, StepHandler

        class TestWorker(StepHandler, Batchable):
            handler_name = "test_checkpoint_worker"

            def call(self, _context):
                pass

        handler = TestWorker()
        result = handler.checkpoint_yield(cursor="", items_processed=0)

        assert result.is_success is True
        assert result.result["cursor"] == ""

    def test_checkpoint_yield_with_large_items_processed(self):
        """Test checkpoint_yield with very large items_processed value."""
        from tasker_core import Batchable, StepHandler

        class TestWorker(StepHandler, Batchable):
            handler_name = "test_checkpoint_worker"

            def call(self, _context):
                pass

        handler = TestWorker()
        # Test with a very large number (10 billion items)
        large_count = 10_000_000_000
        result = handler.checkpoint_yield(cursor=large_count, items_processed=large_count)

        assert result.is_success is True
        assert result.result["items_processed"] == large_count

    def test_checkpoint_yield_with_deeply_nested_accumulated_results(self):
        """Test checkpoint_yield with deeply nested accumulated results."""
        from tasker_core import Batchable, StepHandler

        class TestWorker(StepHandler, Batchable):
            handler_name = "test_checkpoint_worker"

            def call(self, _context):
                pass

        handler = TestWorker()
        nested_results = {
            "level1": {
                "level2": {
                    "level3": {
                        "level4": {
                            "value": 12345,
                            "items": [1, 2, 3, 4, 5],
                        }
                    }
                }
            }
        }
        result = handler.checkpoint_yield(
            cursor=5000,
            items_processed=5000,
            accumulated_results=nested_results,
        )

        assert result.is_success is True
        assert (
            result.result["accumulated_results"]["level1"]["level2"]["level3"]["level4"]["value"]
            == 12345
        )

    def test_checkpoint_yield_with_special_characters_in_cursor(self):
        """Test checkpoint_yield with special characters in string cursor."""
        from tasker_core import Batchable, StepHandler

        class TestWorker(StepHandler, Batchable):
            handler_name = "test_checkpoint_worker"

            def call(self, _context):
                pass

        handler = TestWorker()
        # Unicode and special characters in cursor
        special_cursor = "cursor_with_émojis_🎉_and_中文"
        result = handler.checkpoint_yield(cursor=special_cursor, items_processed=100)

        assert result.is_success is True
        assert result.result["cursor"] == special_cursor


class TestBatchableValidationErrors:
    """TAS-125: Test validation error scenarios for Batchable mixin."""

    def test_create_cursor_configs_with_zero_workers(self):
        """Test create_cursor_configs with zero workers raises error."""
        import pytest

        from tasker_core import Batchable

        class TestHandler(Batchable):
            pass

        handler = TestHandler()
        with pytest.raises(ValueError, match="worker_count must be > 0"):
            handler.create_cursor_configs(1000, 0)

    def test_create_cursor_configs_with_negative_workers(self):
        """Test create_cursor_configs with negative workers raises error."""
        import pytest

        from tasker_core import Batchable

        class TestHandler(Batchable):
            pass

        handler = TestHandler()
        with pytest.raises(ValueError, match="worker_count must be > 0"):
            handler.create_cursor_configs(1000, -1)

    def test_create_cursor_configs_with_zero_items(self):
        """Test create_cursor_configs with zero items returns empty list."""
        from tasker_core import Batchable

        class TestHandler(Batchable):
            pass

        handler = TestHandler()
        configs = handler.create_cursor_configs(0, 5)

        assert len(configs) == 0

    def test_create_cursor_ranges_with_zero_items(self):
        """Test create_cursor_ranges with zero items returns empty list."""
        from tasker_core import Batchable

        class TestHandler(Batchable):
            pass

        handler = TestHandler()
        ranges = handler.create_cursor_ranges(0, 100)

        assert len(ranges) == 0

    def test_create_cursor_ranges_with_zero_batch_size(self):
        """Test create_cursor_ranges with zero batch_size raises error."""
        import pytest

        from tasker_core import Batchable

        class TestHandler(Batchable):
            pass

        handler = TestHandler()
        with pytest.raises(ValueError, match="batch_size must be > 0"):
            handler.create_cursor_ranges(1000, 0)

    def test_create_cursor_ranges_with_negative_batch_size(self):
        """Test create_cursor_ranges with negative batch_size raises error."""
        import pytest

        from tasker_core import Batchable

        class TestHandler(Batchable):
            pass

        handler = TestHandler()
        with pytest.raises(ValueError, match="batch_size must be > 0"):
            handler.create_cursor_ranges(1000, -10)
