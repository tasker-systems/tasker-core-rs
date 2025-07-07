# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

**tasker-core-rs** is a high-performance Rust implementation of the core workflow orchestration engine, designed to complement the existing Ruby on Rails **Tasker** engine found at `/Users/petetaylor/projects/tasker/`. This project leverages Rust's memory safety, fearless parallelism, and performance characteristics to handle computationally intensive workflow orchestration, dependency resolution, and state management operations.

**Architecture**: Step handler foundation where Rust implements the complete step handler base class that frameworks (Rails, Python, Node.js) extend through subclassing with `process()` and `process_results()` hooks.

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
- **State Machine Integration**: Prepared state transition factories (currently disabled pending model method implementation)
- **Batch Generation**: Implemented `ComplexWorkflowBatchFactory` with controlled pattern distributions and proper rounding
- **Type Safety**: All factories work with compile-time verified SQLx queries and proper trait derivations
- **Testing Coverage**: 20/20 factory tests passing with comprehensive edge case handling
- **Rails Pattern Adaptation**: Successfully translated Rails factory patterns to Rust while maintaining type safety

[... rest of the existing content remains unchanged ...]