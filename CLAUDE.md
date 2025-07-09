# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

**tasker-core-rs** is a high-performance Rust implementation of the core workflow orchestration engine, designed to complement the existing Ruby on Rails **Tasker** engine found at `/Users/petetaylor/projects/tasker/`. This project leverages Rust's memory safety, fearless parallelism, and performance characteristics to handle computationally intensive workflow orchestration, dependency resolution, and state management operations.

**Architecture**: Step handler foundation where Rust implements the complete step handler base class that frameworks (Rails, Python, Node.js) extend through subclassing with `process()` and `process_results()` hooks.

## Code Design Principles

- Use TODO for intentionally delayed future work, but not to sidestep a better pattern, even if it requires changes to other related parts of our code.

## Testing Guidelines

- As much as possible, tests should go in our tests/ directory, excluding doctests
- Use sqlx::test for tests requiring database access
- Use #[test] on its own for pure unit tests

## MCP Server Integration

**Essential Development Tools**: This project uses Model Context Protocol (MCP) servers to enhance development workflow and capabilities.

### Configured MCP Servers
- **PostgreSQL MCP** (`crystaldba/postgres-mcp`): Database operations, performance analysis, and migration management
- **GitHub Official MCP** (`github/github-mcp-server`): Repository operations, PR management, and CI/CD integration  
- **Cargo Package MCP** (`artmann/package-registry-mcp`): Rust dependency management and security analysis
- **Docker MCP** (`docker/mcp-servers`): Containerized testing and deployment automation
- **Rust Documentation MCP** (`Govcraft/rust-docs-mcp-server`): Real-time Rust best practices and API guidance
- **Context7 MCP** (SSE): Enhanced development context and intelligence
- **Tasker MCP** (Added July 2025): We now have access to all of the mcp servers in .mcp.json and should use them

**Configuration**: See `docs/MCP_TOOLS.md` for detailed setup, capabilities, and integration patterns.

**Benefits**: Enhanced database development, automated dependency management, streamlined CI/CD workflows, and real-time Rust guidance.

## Recent Accomplishments (January 2025)

### State Machine Integration (COMPLETED - PR #11)
- **Complete State Machine System**: Implemented all 7 planned phases with TaskStateMachine and StepStateMachine
- **Event-Driven Architecture**: Built EventPublisher with broadcast channels for real-time state transition notifications
- **Orchestration Integration**: Seamlessly integrated with OrchestrationCoordinator for unified workflow management
- **Database Persistence**: Full StateTransitionRepository with SQLx integration and idempotent operations
- **Guard System**: Comprehensive guard traits for dependency validation and completion checking
- **Testing Framework**: 103 integration tests + 42 doc tests with comprehensive coverage
- **Event Publishing Fix**: Resolved broadcast channel issue when no subscribers are present
- **Coverage Migration**: Migrated from cargo-tarpaulin to cargo-llvm-cov for superior LLVM-based coverage analysis

### SQL Scopes Implementation
- Created comprehensive Rails-like SQL scope system in `src/scopes.rs`
- Implemented `TaskScope`, `WorkflowStepScope`, and `TaskTransitionScope` with chainable query builders
- Added support for time-based queries, state filtering, and complex JOIN operations
- Achieved 11/12 test coverage with sophisticated deduplication for SQL JOINs

### Factory System for Complex Workflows
- Built comprehensive factory system in `tests/factories/` inspired by Rails patterns
- Implemented complex workflow patterns: Linear, Diamond, Parallel Merge, Tree, and Mixed DAG
- Created API integration workflow factories with multi-step dependencies
- Added dummy task workflows for orchestration testing
- Implemented find-or-create patterns for idempotent test data creation
- Added batch generation capabilities with controlled pattern distributions

### Configuration Architecture
- Separated configuration (`src/config.rs`) from constants (`src/constants.rs`)
- Created hierarchical configuration structure with nested components
- Maintained clean separation between runtime settings and immutable values

### Factory System Enhancement (January 2025)
- **SQLx Factory Trait System**: Built comprehensive `SqlxFactory` trait with database persistence for all test factories
- **Complex Workflow Patterns**: Implemented Linear (A→B→C→D), Diamond (A→(B,C)→D), Parallel Merge ((A,B,C)→D), Tree, and Mixed DAG patterns
- **Relationship Factories**: Created `WorkflowStepEdgeFactory` for managing dependencies between workflow steps
- **State Machine Integration**: Fully integrated state transition factories with complete model method implementation
- **Batch Generation**: Implemented `ComplexWorkflowBatchFactory` with controlled pattern distributions and proper rounding
- **Type Safety**: All factories work with compile-time verified SQLx queries and proper trait derivations
- **Testing Coverage**: 20/20 factory tests passing with comprehensive edge case handling
- **Rails Pattern Adaptation**: Successfully translated Rails factory patterns to Rust while maintaining type safety