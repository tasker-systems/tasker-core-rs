#!/usr/bin/env python3
"""Python Worker Server.

Production-ready server script that bootstraps Rust foundation via FFI
and manages Python handler execution for workflow orchestration.
"""

from __future__ import annotations

import logging
import os
import signal
import sys
import threading
from typing import Any

# Add the python source directory to the path for imports
sys.path.insert(0, os.path.join(os.path.dirname(__file__), "..", "python"))

from tasker_core import (
    bootstrap_worker,
    get_worker_status,
    is_worker_running,
    stop_worker,
    transition_to_graceful_shutdown,
)
from tasker_core.event_bridge import EventBridge, EventNames
from tasker_core.event_poller import EventPoller
from tasker_core.handler import HandlerRegistry
from tasker_core.step_execution_subscriber import StepExecutionSubscriber
from tasker_core.types import BootstrapConfig, FfiStepEvent

# Get log level from environment
log_level = os.environ.get("RUST_LOG", "info").upper()
if log_level == "TRACE":
    log_level = "DEBUG"
logging.basicConfig(
    level=getattr(logging, log_level, logging.INFO),
    format="%(asctime)s [%(levelname)s] %(name)s: %(message)s",
    datefmt="%Y-%m-%dT%H:%M:%S%z",
)
logger = logging.getLogger("python-worker")


def show_banner() -> None:
    """Display startup banner."""
    logger.info("=" * 60)
    logger.info("Starting Python Worker Bootstrap System")
    logger.info("=" * 60)
    logger.info(f"Environment: {os.environ.get('TASKER_ENV', 'development')}")
    logger.info(f"Python Version: {sys.version.split()[0]}")
    logger.info(f"Database URL: {'[REDACTED]' if os.environ.get('DATABASE_URL') else 'Not set'}")
    logger.info(f"Template Path: {os.environ.get('TASKER_TEMPLATE_PATH', 'Not set')}")
    logger.info(f"Web Bind Address: {os.environ.get('TASKER_WEB_BIND_ADDRESS', 'Not set')}")
    logger.info(f"Config Path: {os.environ.get('TASKER_CONFIG_PATH', 'Not set')}")

    logger.info("Configuration:")
    logger.info("  Python will initialize Rust foundation via FFI")
    logger.info("  Worker will process tasks by calling Python handlers")
    if os.environ.get("TASKER_ENV") == "production":
        logger.info("  Production optimizations: Enabled")


def health_check() -> dict[str, Any]:
    """Perform health check on the worker."""
    try:
        status = get_worker_status()
        return {
            "healthy": status.running,
            "status": status.model_dump(),
        }
    except Exception as e:
        return {
            "healthy": False,
            "error": str(e),
        }


def discover_handlers() -> int:
    """Discover and register handlers from configured handler paths.

    Returns:
        Number of handlers discovered.
    """
    registry = HandlerRegistry.instance()
    total_discovered = 0

    # Get handler path from environment
    handler_path = os.environ.get("PYTHON_HANDLER_PATH")
    if not handler_path:
        logger.info("No PYTHON_HANDLER_PATH configured, skipping handler discovery")
        return 0

    if not os.path.isdir(handler_path):
        logger.warning(f"Handler path does not exist: {handler_path}")
        return 0

    # Add handler path to Python path for imports
    if handler_path not in sys.path:
        sys.path.insert(0, handler_path)

    # Discover handlers from the examples subpackage
    # Handlers are organized as flat files: examples/<name>_handlers.py
    examples_path = os.path.join(handler_path, "examples")
    if os.path.isdir(examples_path):
        # Look for *_handlers.py files in the examples directory
        for filename in os.listdir(examples_path):
            if filename.endswith("_handlers.py"):
                # Convert filename to module name (e.g., linear_handlers.py -> examples.linear_handlers)
                module_name = filename[:-3]  # Remove .py extension
                package_name = f"examples.{module_name}"
                try:
                    discovered = registry.discover_handlers(package_name)
                    total_discovered += discovered
                    logger.info(f"Discovered {discovered} handlers from {package_name}")
                except Exception as e:
                    logger.warning(f"Failed to discover handlers from {package_name}: {e}")

    logger.info(f"Total handlers discovered: {total_discovered}")
    logger.info(f"Registered handlers: {registry.list_handlers()}")
    return total_discovered


def main() -> int:
    """Run the Python worker server."""
    show_banner()

    # Shutdown coordination
    shutdown_event = threading.Event()
    shutdown_lock = threading.Lock()

    def handle_shutdown(signum: int, _frame: Any) -> None:
        """Handle shutdown signals."""
        sig_name = signal.Signals(signum).name
        logger.info(f"Received {sig_name} signal, initiating shutdown...")
        with shutdown_lock:
            shutdown_event.set()

    def handle_status(_signum: int, _frame: Any) -> None:
        """Handle status request signal (USR1)."""
        logger.info("Received SIGUSR1 signal, reporting worker status...")
        try:
            status = get_worker_status()
            logger.info(f"Worker Status: {status.model_dump()}")
        except Exception as e:
            logger.error(f"Failed to get worker status: {e}")

    # Set up signal handlers
    signal.signal(signal.SIGTERM, handle_shutdown)
    signal.signal(signal.SIGINT, handle_shutdown)
    if hasattr(signal, "SIGUSR1"):
        signal.signal(signal.SIGUSR1, handle_status)

    try:
        logger.info("Initializing Python Worker Bootstrap...")

        # Discover and register handlers before bootstrap
        logger.info("Discovering Python handlers...")
        handler_count = discover_handlers()
        logger.info(f"Handler discovery complete: {handler_count} handlers registered")

        # Create bootstrap configuration
        config = BootstrapConfig(
            namespace=os.environ.get("TASKER_NAMESPACE", "default"),
            log_level=os.environ.get("RUST_LOG", "info"),
        )

        # Bootstrap the worker
        result = bootstrap_worker(config)

        if not result.success:
            logger.error(f"Failed to bootstrap worker: {result.message}")
            return 3

        # =================================================================
        # Initialize Python event dispatch system
        # This is the key integration that routes FFI events to Python handlers
        # =================================================================

        logger.info("Starting Python event dispatch system...")

        # 1. Start the EventBridge (pub/sub for step events)
        event_bridge = EventBridge.instance()
        event_bridge.start()
        logger.info("  EventBridge: Started")

        # 2. Create and start StepExecutionSubscriber (routes events to handlers)
        handler_registry = HandlerRegistry.instance()
        worker_id = os.environ.get("HOSTNAME", "python-worker")
        step_subscriber = StepExecutionSubscriber(
            event_bridge=event_bridge,
            handler_registry=handler_registry,
            worker_id=worker_id,
        )
        step_subscriber.start()
        logger.info(f"  StepExecutionSubscriber: Started (worker_id={worker_id})")

        # 3. Create EventPoller with callback that publishes to EventBridge
        def on_step_event(event: FfiStepEvent) -> None:
            """Forward polled events to EventBridge for handler dispatch."""
            event_bridge.publish(EventNames.STEP_EXECUTION_RECEIVED, event)

        def on_poller_error(error: Exception) -> None:
            """Log poller errors."""
            logger.error(f"EventPoller error: {error}")
            event_bridge.publish(EventNames.POLLER_ERROR, error)

        event_poller = EventPoller(
            polling_interval_ms=10,  # 10ms polling interval
            starvation_check_interval=100,  # Check every ~1 second
            cleanup_interval=1000,  # Cleanup every ~10 seconds
        )
        event_poller.on_step_event(on_step_event)
        event_poller.on_error(on_poller_error)

        # 4. Start the poller (runs in background daemon thread)
        event_poller.start()
        logger.info("  EventPoller: Started (10ms polling interval)")

        logger.info("Python event dispatch system ready")
        logger.info("=" * 60)

        logger.info("Python worker system started successfully")
        logger.info("  Rust foundation: Bootstrapped via FFI")
        logger.info("  Python handlers: Registered and ready")
        logger.info("  Event dispatch: Active (polling -> handlers -> completion)")
        logger.info("  Worker status: Running and processing tasks")
        logger.info("Worker ready and processing tasks")
        logger.info("=" * 60)

        # Main worker loop
        sleep_interval = 5 if os.environ.get("TASKER_ENV") == "production" else 1
        loop_count = 0

        while not shutdown_event.is_set():
            # Wait for shutdown signal with timeout
            shutdown_event.wait(timeout=sleep_interval)

            if shutdown_event.is_set():
                break

            # Periodic health check (every 60 iterations)
            loop_count += 1
            if loop_count % 60 == 0:
                try:
                    health_status = health_check()
                    if health_status["healthy"]:
                        logger.debug(f"Health check #{loop_count // 60}: OK")
                    else:
                        logger.warning(
                            f"Health check #{loop_count // 60}: UNHEALTHY - {health_status.get('error')}"
                        )
                except Exception as e:
                    logger.warning(f"Health check failed: {e}")

        # Shutdown sequence
        logger.info("Starting shutdown sequence...")

        try:
            # 1. Stop EventPoller first (stops new events from being polled)
            logger.info("  Stopping EventPoller...")
            event_poller.stop(timeout=5.0)
            logger.info("  EventPoller stopped")

            # 2. Stop StepExecutionSubscriber (stops event routing)
            logger.info("  Stopping StepExecutionSubscriber...")
            step_subscriber.stop()
            logger.info("  StepExecutionSubscriber stopped")

            # 3. Stop EventBridge (clears all subscriptions)
            logger.info("  Stopping EventBridge...")
            event_bridge.stop()
            logger.info("  EventBridge stopped")

            # 4. Stop Rust worker foundation
            if is_worker_running():
                logger.info("  Transitioning to graceful shutdown...")
                transition_to_graceful_shutdown()
                logger.info("  Stopping Rust worker...")
                stop_worker()
                logger.info("  Rust worker stopped")

            logger.info("Python worker shutdown completed successfully")
        except Exception as e:
            logger.error(f"Error during shutdown: {e}")
            return 1

        logger.info("Python Worker Server terminated gracefully")
        return 0

    except Exception as e:
        logger.fatal(f"Unexpected error during startup: {type(e).__name__} - {e}")
        import traceback

        logger.fatal(traceback.format_exc())
        return 4


if __name__ == "__main__":
    sys.exit(main())
