# Tasker Core Guides

This directory contains practical how-to guides for working with Tasker Core.

## Documents

| Document | Description |
|----------|-------------|
| [Quick Start](./quick-start.md) | Get running in 5 minutes |
| [Use Cases and Patterns](./use-cases-and-patterns.md) | Practical workflow examples |
| [Conditional Workflows](./conditional-workflows.md) | Runtime decision-making and dynamic steps |
| [Batch Processing](./batch-processing.md) | Parallel processing with cursor-based workers |
| [DLQ System](./dlq-system.md) | Dead letter queue investigation and resolution |
| [Retry Semantics](./retry-semantics.md) | Understanding max_attempts and retryable flags |
| [Identity Strategy](./identity-strategy.md) | Task deduplication with STRICT, CALLER_PROVIDED, ALWAYS_UNIQUE |
| [Configuration Management](./configuration-management.md) | TOML architecture, CLI tools, runtime observability |

## When to Read These

- **Getting started**: Begin with Quick Start
- **Implementing features**: Check Use Cases and Patterns
- **Handling errors**: See Retry Semantics and DLQ System
- **Processing data**: Review Batch Processing
- **Deploying**: Consult Configuration Management

## Related Documentation

- [Architecture](../architecture/) - The "what" - system structure
- [Principles](../principles/) - The "why" - design philosophy
- [Workers](../workers/) - Language-specific handler development
