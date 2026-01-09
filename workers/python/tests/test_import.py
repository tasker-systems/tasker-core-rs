"""Basic import and functionality tests for tasker_core.

These tests verify the Phase 1 (TAS-72-P1) and Phase 2 (TAS-72-P2) implementation:
- Module can be imported
- Version information is accessible
- Health check passes
- Phase 2 exports are available
"""

from __future__ import annotations


def test_import_module():
    """Test that the module can be imported."""
    import tasker_core

    assert tasker_core is not None


def test_version():
    """Test that version is accessible."""
    import tasker_core

    version = tasker_core.version()
    assert isinstance(version, str)
    assert len(version) > 0
    # Version should be semver-like (contains dots)
    assert "." in version


def test_rust_version():
    """Test that Rust version info is accessible."""
    import tasker_core

    rust_version = tasker_core.get_rust_version()
    assert isinstance(rust_version, str)
    assert "tasker-worker-py" in rust_version
    assert "rustc" in rust_version


def test_health_check():
    """Test that health check passes."""
    import tasker_core

    assert tasker_core.health_check() is True


def test_dunder_version():
    """Test that __version__ is exposed."""
    import tasker_core

    assert hasattr(tasker_core, "__version__")
    assert tasker_core.__version__ == tasker_core.version()


class TestModuleExports:
    """Test that expected symbols are exported."""

    def test_all_exports(self):
        """Test __all__ contains expected symbols."""
        import tasker_core

        # Phase 1 exports
        phase1_exports = {
            "__version__",
            "get_version",
            "get_rust_version",
            "health_check",
            "version",
        }

        # Phase 2 exports
        phase2_exports = {
            # Bootstrap functions
            "bootstrap_worker",
            "stop_worker",
            "get_worker_status",
            "transition_to_graceful_shutdown",
            "is_worker_running",
            # Logging functions
            "log_error",
            "log_warn",
            "log_info",
            "log_debug",
            "log_trace",
            # Types
            "BootstrapConfig",
            "BootstrapResult",
            "WorkerStatus",
            "WorkerState",
            "StepHandlerCallResult",
            "LogContext",
            # Exceptions
            "TaskerError",
            "WorkerNotInitializedError",
            "WorkerBootstrapError",
            "WorkerAlreadyRunningError",
            "FFIError",
            "ConversionError",
        }

        # Phase 3 exports
        phase3_exports = {
            # FFI functions
            "poll_step_events",
            "complete_step_event",
            "get_ffi_dispatch_metrics",
            "check_starvation_warnings",
            "cleanup_timeouts",
            # Types
            "FfiStepEvent",
            "StepExecutionResult",
            "StepError",
            "ResultStatus",
            "FfiDispatchMetrics",
            "StarvationWarning",
            # EventPoller
            "EventPoller",
        }

        # Phase 4 exports
        phase4_exports = {
            # Event bridge
            "EventBridge",
            "EventNames",
            # Handler system
            "HandlerRegistry",
            "StepHandler",
            "StepContext",
            "StepHandlerResult",
            # Step execution subscriber
            "StepExecutionSubscriber",
            "StepExecutionError",
            # Template discovery (for manual usage)
            "TemplatePath",
            "TemplateParser",
            "HandlerDiscovery",
        }

        # Phase 5 exports
        phase5_exports = {
            # Domain events
            "poll_in_process_events",
            "InProcessDomainEventPoller",
            "DomainEventMetadata",
            "ExecutionResult",
            "InProcessDomainEvent",
            # Observability
            "get_health_check",
            "get_metrics",
            "get_worker_config",
            "ComponentHealth",
            "HealthCheck",
            "WorkerMetrics",
            "WorkerConfig",
        }

        # Phase 6a exports (Ruby parity)
        phase6a_exports = {
            # Model wrappers
            "TaskSequenceStepWrapper",
            "TaskWrapper",
            "WorkflowStepWrapper",
            "DependencyResultsWrapper",
            "StepDefinitionWrapper",
            "HandlerWrapper",
            # Error classification
            "RetryableError",
            "PermanentError",
            "StepTimeoutError",
            "NetworkError",
            "RateLimitError",
            "ServiceUnavailableError",
            "ResourceContentionError",
            "StepValidationError",
            "NotFoundError",
            "AuthenticationError",
            "AuthorizationError",
            "ConfigurationError",
            "BusinessLogicError",
            "ErrorClassifier",
            "get_classifier",
            "is_retryable",
            "is_permanent",
        }

        # Phase 6b exports (Specialized handlers and batch processing)
        phase6b_exports = {
            # Specialized handlers
            "ApiHandler",
            "ApiResponse",
            "DecisionHandler",
            # Batch processing
            "Batchable",
            "BatchAggregationScenario",
            "BatchWorkerConfig",
            "DecisionType",
            "DecisionPointOutcome",
            "CursorConfig",
            "BatchAnalyzerOutcome",
            "BatchWorkerContext",
            "BatchWorkerOutcome",
        }

        # TAS-95 exports (Cross-language API alignment)
        tas95_exports = {
            "ErrorType",
            "BasePublisher",
            "BaseSubscriber",
            "StepEventContext",
        }

        # TAS-112 exports (Cross-language domain event types)
        tas112_exports = {
            "EventDeclaration",
            "StepResult",
            "PublishContext",
        }

        # TAS-93 exports (Step Handler Router/Resolver)
        tas93_exports = {
            "BaseResolver",
            "ResolverChain",
            "ResolverNotFoundError",
            "ResolutionError",
            "HandlerDefinition",
            "MethodDispatchWrapper",
            "RegistryResolver",
            "ExplicitMappingResolver",
            "ClassLookupResolver",
        }

        expected = (
            phase1_exports
            | phase2_exports
            | phase3_exports
            | phase4_exports
            | phase5_exports
            | phase6a_exports
            | phase6b_exports
            | tas95_exports
            | tas112_exports
            | tas93_exports
        )
        assert set(tasker_core.__all__) == expected

    def test_exports_are_callable(self):
        """Test that exported functions are callable."""
        import tasker_core

        # Phase 1 functions - these should not raise
        tasker_core.version()
        tasker_core.get_version()
        tasker_core.get_rust_version()
        tasker_core.health_check()

        # Phase 2 - is_worker_running should work without bootstrap
        result = tasker_core.is_worker_running()
        assert result is False  # Not running since we haven't bootstrapped
