# Project Brief: Tasker Core Rust

## Project Overview

**tasker-core-rs** is a high-performance Rust orchestration core designed to complement the existing Ruby on Rails **Tasker** engine located at `/Users/petetaylor/projects/tasker/`. This is **not a replacement** but a **delegation-based enhancement** that handles performance-critical orchestration while maintaining full compatibility with existing step handlers and queue systems.

## Core Mission

Build a **high-performance orchestration core** that provides 10-100x performance improvements in critical bottlenecks while enabling seamless integration with the existing Rails Tasker ecosystem.

## Architecture Philosophy

- **Orchestration Core, Not Replacement**: Rust handles decision-making and performance-critical operations
- **Delegation-Based Integration**: Seamless handoff to framework-managed step execution
- **Zero-Disruption Migration**: Existing step handlers and queue systems work unchanged
- **Performance Enhancement**: Target 10-100x improvements in bottleneck operations

## Key Requirements

### Performance Targets
- **10-100x faster** dependency resolution vs PostgreSQL functions
- **<1ms overhead** per step coordination handoff
- **<10% FFI penalty** vs native Ruby execution
- **Sub-millisecond** state transitions
- **>10k events/sec** cross-language event processing

### Integration Requirements
- **Ruby FFI**: Primary integration with Rails Tasker engine
- **Python FFI**: Support for data science workflows
- **C-compatible API**: Maximum interoperability
- **Queue System Compatibility**: Work with existing Sidekiq/background job systems

### Compatibility Requirements
- **Database Schema**: Match existing PostgreSQL schema exactly
- **Event System**: Support 56+ lifecycle events from Rails engine
- **State Management**: Maintain audit trails and transition history
- **Configuration**: Support existing Rails configuration patterns

## Success Criteria

1. **Performance**: Demonstrate 10-100x improvement in dependency resolution
2. **Compatibility**: Existing step handlers work without modification
3. **Integration**: Seamless handoff between Rust core and Rails framework
4. **Reliability**: Memory-safe, concurrent operations with comprehensive error handling
5. **Observability**: OpenTelemetry integration and structured logging

## Project Boundaries

### In Scope
- Database models matching Rails schema exactly
- High-performance dependency resolution (DAG traversal)
- State machine management (Task/Step state transitions)
- Event system with cross-language publishing
- FFI interfaces for Ruby, Python, and C
- Orchestration coordination and delegation logic

### Out of Scope
- Web interface (handled by Rails)
- Step handler implementations (delegated to framework)
- Queue management (delegated to Sidekiq)
- Business logic execution (delegated to existing handlers)
- User authentication/authorization (handled by Rails)

## Technical Constraints

- **Language**: Rust 2021 edition with async/await
- **Database**: PostgreSQL with SQLx for type-safe queries
- **FFI**: Magnus (Ruby), PyO3 (Python), C-compatible ABI
- **Async Runtime**: Tokio for all async operations
- **Memory Safety**: Leverage Rust ownership system for concurrent operations
- **Testing**: Property-based testing for DAG operations, comprehensive coverage

## Reference Implementation

The production Rails Tasker engine at `/Users/petetaylor/projects/tasker/` provides the reference implementation for:
- Database schema (`spec/dummy/db/structure.sql`)
- Core orchestration logic (`lib/tasker/`)
- Models and relationships (`app/models/`)
- Event definitions and lifecycle patterns
