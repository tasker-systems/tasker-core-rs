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

        assert result.success is True
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

        assert result.success is True
        assert result.result["items_processed"] == 25
        assert result.result["items_succeeded"] == 24
        assert result.result["items_failed"] == 1
        assert result.metadata.get("batch_worker") is True
