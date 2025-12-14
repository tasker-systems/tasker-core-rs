"""Mock-based tests for bootstrap functions.

These tests verify the Python wrapper logic without requiring a running worker.
"""

from __future__ import annotations

from unittest.mock import patch

from tasker_core import (
    BootstrapConfig,
    BootstrapResult,
    WorkerStatus,
)


class TestBootstrapWorkerWrapper:
    """Tests for bootstrap_worker wrapper logic."""

    @patch("tasker_core.bootstrap._bootstrap_worker")
    def test_bootstrap_worker_success(self, mock_ffi):
        """Test bootstrap_worker processes FFI result correctly."""
        from tasker_core.bootstrap import bootstrap_worker

        mock_ffi.return_value = {
            "status": "started",
            "handle_id": "handle-123",
            "worker_id": "worker-456",
            "message": "Worker started successfully",
        }

        result = bootstrap_worker()

        assert isinstance(result, BootstrapResult)
        assert result.success is True
        assert result.handle_id == "handle-123"
        assert result.worker_id == "worker-456"
        assert result.status == "started"
        mock_ffi.assert_called_once_with(None)

    @patch("tasker_core.bootstrap._bootstrap_worker")
    def test_bootstrap_worker_with_config(self, mock_ffi):
        """Test bootstrap_worker passes config to FFI."""
        from tasker_core.bootstrap import bootstrap_worker

        mock_ffi.return_value = {
            "status": "started",
            "handle_id": "handle-123",
            "worker_id": "worker-456",
        }

        config = BootstrapConfig(
            namespace="payments",
            worker_id="custom-worker",
        )
        result = bootstrap_worker(config)

        assert result.success is True
        # Verify config was converted to dict
        call_args = mock_ffi.call_args[0][0]
        assert call_args is not None
        assert call_args.get("namespace") == "payments"
        assert call_args.get("worker_id") == "custom-worker"

    @patch("tasker_core.bootstrap._bootstrap_worker")
    def test_bootstrap_worker_failure_status(self, mock_ffi):
        """Test bootstrap_worker handles failure status."""
        from tasker_core.bootstrap import bootstrap_worker

        mock_ffi.return_value = {
            "status": "failed",
            "handle_id": "",
            "worker_id": "",
            "message": "Connection failed",
        }

        result = bootstrap_worker()

        assert result.success is False
        assert result.status == "failed"
        assert result.message == "Connection failed"


class TestGetWorkerStatusWrapper:
    """Tests for get_worker_status wrapper logic."""

    @patch("tasker_core.bootstrap._get_worker_status")
    def test_get_worker_status_running(self, mock_ffi):
        """Test get_worker_status when worker is running."""
        from tasker_core.bootstrap import get_worker_status

        mock_ffi.return_value = {
            "running": True,
            "worker_core_status": "Running",
            "database_pool_size": 10,
            "database_pool_idle": 5,
        }

        status = get_worker_status()

        assert isinstance(status, WorkerStatus)
        assert status.running is True
        assert status.worker_core_status == "Running"
        assert status.database_pool_size == 10

    @patch("tasker_core.bootstrap._get_worker_status")
    def test_get_worker_status_not_running(self, mock_ffi):
        """Test get_worker_status when worker is not running."""
        from tasker_core.bootstrap import get_worker_status

        mock_ffi.return_value = {
            "running": False,
            "worker_core_status": "Stopped",
            "error": "Worker not initialized",
        }

        status = get_worker_status()

        assert status.running is False
        assert status.worker_core_status == "Stopped"
        assert status.error == "Worker not initialized"


class TestTransitionToGracefulShutdown:
    """Tests for transition_to_graceful_shutdown wrapper."""

    @patch("tasker_core.bootstrap._transition_to_graceful_shutdown")
    def test_transition_success(self, mock_ffi):
        """Test transition_to_graceful_shutdown returns message."""
        from tasker_core.bootstrap import transition_to_graceful_shutdown

        mock_ffi.return_value = "Graceful shutdown initiated"

        result = transition_to_graceful_shutdown()

        assert result == "Graceful shutdown initiated"
        mock_ffi.assert_called_once()


class TestObservabilityMocks:
    """Mock-based tests for observability functions."""

    @patch("tasker_core.observability._get_health_check")
    def test_get_health_check_success(self, mock_ffi):
        """Test get_health_check processes FFI result."""
        from tasker_core.observability import get_health_check

        mock_ffi.return_value = {
            "status": "healthy",
            "is_running": True,
            "components": {
                "database": {
                    "name": "database",
                    "status": "healthy",
                    "pool_size": 10,
                    "pool_idle": 5,
                },
            },
            "rust": {"environment": "test"},
            "python": {"handler_count": 3},
        }

        health = get_health_check()

        assert health.status == "healthy"
        assert health.is_running is True
        assert "database" in health.components
        assert health.components["database"].status == "healthy"
        assert health.components["database"].pool_size == 10
        assert health.rust["environment"] == "test"

    @patch("tasker_core.observability._get_metrics")
    def test_get_metrics_success(self, mock_ffi):
        """Test get_metrics processes FFI result."""
        from tasker_core.observability import get_metrics

        mock_ffi.return_value = {
            "dispatch_channel_pending": 5,
            "oldest_pending_age_ms": 100,
            "starvation_detected": False,
            "database_pool_size": 10,
            "database_pool_idle": 3,
            "environment": "test",
            "steps_processed": 100,
        }

        metrics = get_metrics()

        assert metrics.dispatch_channel_pending == 5
        assert metrics.oldest_pending_age_ms == 100
        assert metrics.starvation_detected is False
        assert metrics.steps_processed == 100

    @patch("tasker_core.observability._get_worker_config")
    def test_get_worker_config_success(self, mock_ffi):
        """Test get_worker_config processes FFI result."""
        from tasker_core.observability import get_worker_config

        mock_ffi.return_value = {
            "environment": "production",
            "supported_namespaces": ["payments", "orders"],
            "web_api_enabled": True,
            "database_pool_size": 20,
            "polling_interval_ms": 10,
            "starvation_threshold_ms": 1000,
            "max_concurrent_handlers": 10,
            "handler_timeout_ms": 30000,
        }

        config = get_worker_config()

        assert config.environment == "production"
        assert config.supported_namespaces == ["payments", "orders"]
        assert config.web_api_enabled is True
        assert config.polling_interval_ms == 10
