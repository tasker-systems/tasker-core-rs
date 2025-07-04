# Changelog

All notable changes to the Tasker Core Rust project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- Comprehensive CI/CD pipeline with multi-platform testing
- Claude Code integration for AI-assisted development
- Automated release workflow with cross-platform binaries
- Dependabot configuration for automated dependency updates
- Code coverage reporting with Codecov integration
- Security auditing with cargo-audit
- Performance benchmarking infrastructure

### Changed
- Migrated from custom test coordinator to SQLx native testing
- Updated legacy rust.yml workflow to redirect to comprehensive CI

## [0.1.0] - Phase 1 Model Migration Complete

### Added
- **Complete Model Layer**: All 18+ Rails models migrated with 100% schema parity
- **Core Table-Based Models**: Task, TaskNamespace, WorkflowStep, WorkflowStepEdge, NamedTask, NamedStep, etc.
- **Orchestration Models**: TaskExecutionContext, StepReadinessStatus, StepDagRelationship via SQL functions
- **Analytics Models**: AnalyticsMetrics, SystemHealthCounts, SlowestSteps, SlowestTasks
- **Query Builder System**: Comprehensive Rails-equivalent scopes with type safety
- **SQL Function Integration**: 8 PostgreSQL functions wrapped with compile-time verification
- **SQLx Native Testing**: Automatic database isolation per test (114 tests)
- **State Machine Foundation**: Task and step state management with transition tracking
- **Configuration System**: Environment-based configuration with validation
- **Error Handling**: Structured error types with comprehensive error propagation

### Technical Achievements
- **Schema Accuracy**: 100% match with Rails production schema
- **Type Safety**: Full SQLx compile-time query verification
- **Performance**: Zero-overhead abstractions over raw SQL
- **Testing**: 114 tests running in parallel with perfect isolation
- **Documentation**: Comprehensive rustdoc with examples

### Migration Details
- **TaskAnnotation**: Fixed to use single JSONB annotation field
- **DependentSystemObjectMap**: Bidirectional mapping with proper foreign keys
- **NamedTasksNamedStep**: Complete junction table with step configuration
- **StepDagRelationship**: SQL VIEW wrapper with recursive CTEs and cycle detection

### Infrastructure
- **Database Migrations**: Auto-discovering with PostgreSQL advisory locks
- **Connection Pooling**: Thread-safe SQLx integration
- **Parallel Testing**: Race condition prevention with database-level synchronization
- **Multi-Language FFI**: Foundation for Ruby, Python, JavaScript integration

### Performance Targets Achieved
- **Model Queries**: High-performance equivalents of ActiveRecord scopes
- **SQL Functions**: Direct PostgreSQL function integration for complex operations
- **Type Safety**: Compile-time prevention of SQL injection and type mismatches
- **Memory Safety**: Zero memory leaks with Rust ownership model

## [0.0.1] - Initial Project Setup

### Added
- Basic Rust project structure with Cargo.toml
- Initial database connection with SQLx
- Basic model foundation (Task, TaskNamespace, Transitions)
- Property-based testing setup
- Initial configuration system
- Multi-language FFI feature flags (Ruby, Python, WASM)

### Technical Foundation
- PostgreSQL integration with SQLx
- Serde for serialization
- Chrono for date/time handling
- Error handling with thiserror
- Testing with tokio-test and proptest