"""Observability module for tasker-core Python worker.

This module provides functions for monitoring worker health, metrics,
and configuration.

Example:
    >>> from tasker_core import get_health_check, get_metrics, get_worker_config
    >>>
    >>> # Check health status
    >>> health = get_health_check()
    >>> if health.status == "healthy":
    ...     print("Worker is healthy")
    ...
    >>> # Get performance metrics
    >>> metrics = get_metrics()
    >>> print(f"Pending events: {metrics.dispatch_channel_pending}")
    >>>
    >>> # Get current configuration
    >>> config = get_worker_config()
    >>> print(f"Environment: {config.environment}")
"""

from __future__ import annotations

from tasker_core._tasker_core import (  # type: ignore[attr-defined]
    get_health_check as _get_health_check,
)
from tasker_core._tasker_core import (  # type: ignore[attr-defined]
    get_metrics as _get_metrics,
)
from tasker_core._tasker_core import (  # type: ignore[attr-defined]
    get_worker_config as _get_worker_config,
)

from .exceptions import WorkerNotInitializedError
from .logging import log_debug
from .types import ComponentHealth, HealthCheck, WorkerConfig, WorkerMetrics


def get_health_check() -> HealthCheck:
    """Get comprehensive health check status.

    Returns health status of all worker components including database,
    PGMQ, and handler system.

    Returns:
        HealthCheck: Health status with component details.

    Raises:
        WorkerNotInitializedError: If worker is not running.
        RuntimeError: If health check fails.

    Example:
        >>> health = get_health_check()
        >>> if health.status == "healthy":
        ...     print("All systems operational")
        >>> else:
        ...     for name, component in health.components.items():
        ...         if component.status == "unhealthy":
        ...             print(f"Component {name} is unhealthy")
    """
    try:
        health_dict = _get_health_check()
        log_debug("Got health check from FFI", {"status": health_dict.get("status")})

        # Convert component dicts to ComponentHealth objects
        components = {}
        if "components" in health_dict:
            for name, comp_data in health_dict["components"].items():
                components[name] = ComponentHealth.model_validate(comp_data)

        return HealthCheck(
            status=health_dict.get("status", "unknown"),
            is_running=health_dict.get("is_running", False),
            components=components,
            rust=health_dict.get("rust", {}),
            python=health_dict.get("python", {}),
        )
    except RuntimeError as e:
        error_msg = str(e)
        if "not initialized" in error_msg.lower() or "not running" in error_msg.lower():
            raise WorkerNotInitializedError("Worker is not running") from e
        raise


def get_metrics() -> WorkerMetrics:
    """Get worker performance metrics.

    Returns metrics about step execution, timing, channels, and errors.

    Returns:
        WorkerMetrics: Performance metrics data.

    Raises:
        WorkerNotInitializedError: If worker is not running.
        RuntimeError: If metrics retrieval fails.

    Example:
        >>> metrics = get_metrics()
        >>> if metrics.starvation_detected:
        ...     print(f"Warning: {metrics.starving_event_count} events starving")
        >>> print(f"DB pool: {metrics.database_pool_idle}/{metrics.database_pool_size}")
    """
    try:
        metrics_dict = _get_metrics()
        log_debug("Got metrics from FFI")

        return WorkerMetrics.model_validate(metrics_dict)
    except RuntimeError as e:
        error_msg = str(e)
        if "not initialized" in error_msg.lower() or "not running" in error_msg.lower():
            raise WorkerNotInitializedError("Worker is not running") from e
        raise


def get_worker_config() -> WorkerConfig:
    """Get current worker configuration.

    Returns the runtime configuration settings for the worker.

    Returns:
        WorkerConfig: Current configuration data.

    Raises:
        WorkerNotInitializedError: If worker is not running.
        RuntimeError: If config retrieval fails.

    Example:
        >>> config = get_worker_config()
        >>> print(f"Environment: {config.environment}")
        >>> print(f"Namespaces: {config.supported_namespaces}")
        >>> print(f"Polling interval: {config.polling_interval_ms}ms")
    """
    try:
        config_dict = _get_worker_config()
        log_debug("Got worker config from FFI")

        return WorkerConfig.model_validate(config_dict)
    except RuntimeError as e:
        error_msg = str(e)
        if "not initialized" in error_msg.lower() or "not running" in error_msg.lower():
            raise WorkerNotInitializedError("Worker is not running") from e
        raise


__all__ = [
    "get_health_check",
    "get_metrics",
    "get_worker_config",
]
