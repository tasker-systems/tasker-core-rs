=== Tasker-Orchestration Configuration Usage Analysis ===

## Database Configuration
- tasker-orchestration/src/lib.rs:64://! assert_eq!(config.database.enable_secondary_database, false);
- tasker-orchestration/src/orchestration/config.rs:34:        assert!(!config.database.enable_secondary_database);

## Orchestration Configuration
- tasker-orchestration/src/orchestration/config.rs:33:        assert!(!config.orchestration.web.auth.enabled);
- tasker-orchestration/src/orchestration/config.rs:53:        assert!(!config.orchestration.web.auth.enabled);
- tasker-orchestration/src/orchestration/bootstrap.rs:161:            enable_web_api: config.orchestration.web_enabled(),
- tasker-orchestration/src/orchestration/bootstrap.rs:223:            let web_config = tasker_config.orchestration.web.clone();
- tasker-orchestration/src/orchestration/bootstrap.rs:252:            let orchestration_config = tasker_config.event_systems.orchestration.clone();
- tasker-orchestration/src/orchestration/event_systems/unified_event_coordinator.rs:84:        let orchestration_config = config.event_systems.orchestration.clone();

## Queue Configuration
- tasker-orchestration/src/orchestration/orchestration_queues/listener.rs:342:        let queue_config = context.tasker_config.queues.clone();
- tasker-orchestration/src/orchestration/orchestration_queues/fallback_poller.rs:226:        let queue_config = self.context.tasker_config.queues.clone();
- tasker-orchestration/src/orchestration/config.rs:42:        assert!(!config.queues.orchestration_namespace.is_empty());
- tasker-orchestration/src/orchestration/config.rs:43:        assert!(!config.queues.worker_namespace.is_empty());
- tasker-orchestration/src/orchestration/config.rs:44:        assert_eq!(config.queues.backend, "pgmq");
- tasker-orchestration/src/orchestration/bootstrap.rs:159:            namespaces: vec![config.queues.orchestration_namespace.clone()],

## MPSC Channels Configuration

## Circuit Breaker Configuration

## Event Systems Configuration
- tasker-orchestration/src/orchestration/bootstrap.rs:249:            let task_readiness_config = tasker_config.event_systems.task_readiness.clone();
- tasker-orchestration/src/orchestration/bootstrap.rs:252:            let orchestration_config = tasker_config.event_systems.orchestration.clone();
- tasker-orchestration/src/orchestration/event_systems/unified_event_coordinator.rs:81:        let task_readiness_config = config.event_systems.task_readiness.clone();
- tasker-orchestration/src/orchestration/event_systems/unified_event_coordinator.rs:84:        let orchestration_config = config.event_systems.orchestration.clone();

## Execution Configuration
- tasker-orchestration/src/lib.rs:65://! assert_eq!(config.execution.max_concurrent_tasks, 100);

## Backoff Configuration
- tasker-orchestration/src/orchestration/backoff_calculator.rs:72:            max_delay_seconds: config.backoff.max_backoff_seconds as u32,
- tasker-orchestration/src/orchestration/backoff_calculator.rs:73:            multiplier: config.backoff.backoff_multiplier,
- tasker-orchestration/src/orchestration/backoff_calculator.rs:74:            jitter_enabled: config.backoff.jitter_enabled,
- tasker-orchestration/src/orchestration/backoff_calculator.rs:75:            max_jitter: config.backoff.jitter_max_percentage,
- tasker-orchestration/src/orchestration/backoff_calculator.rs:92:            max_delay_seconds: config.backoff.max_backoff_seconds as u32,
- tasker-orchestration/src/orchestration/backoff_calculator.rs:93:            multiplier: config.backoff.backoff_multiplier,
- tasker-orchestration/src/orchestration/backoff_calculator.rs:94:            jitter_enabled: config.backoff.jitter_enabled,
- tasker-orchestration/src/orchestration/backoff_calculator.rs:95:            max_jitter: config.backoff.jitter_max_percentage,
- tasker-orchestration/src/orchestration/config.rs:36:            config.backoff.default_backoff_seconds,
- tasker-orchestration/src/orchestration/config.rs:39:        assert_eq!(config.backoff.max_backoff_seconds, 300);
- tasker-orchestration/src/orchestration/config.rs:40:        assert!(config.backoff.jitter_enabled);

## Telemetry Configuration

## Task Readiness Configuration
- tasker-orchestration/src/orchestration/bootstrap.rs:249:            let task_readiness_config = tasker_config.event_systems.task_readiness.clone();
- tasker-orchestration/src/orchestration/event_systems/unified_event_coordinator.rs:81:        let task_readiness_config = config.event_systems.task_readiness.clone();

