# Archive Documentation Index

## Overview

This archive contains consolidated documentation from multiple architectural iterations of tasker-core-rs. These documents preserve valuable strategic insights, technical lessons learned, and proven methodologies while removing obsolete implementation details.

## Current Architecture Context

**tasker-core-rs** has evolved through several major architectural phases:
- **ZeroMQ pub-sub** → **TCP command architecture** → **pgmq (PostgreSQL message queues)**

The current production architecture uses **pgmq** for queue-based workflow orchestration with autonomous Ruby workers. These archived documents contain insights that remain valuable regardless of the underlying messaging architecture.

## Document Guide

### 📐 [Architectural Evolution](./architectural-evolution.md)
**Purpose**: Strategic insights from the ZeroMQ → TCP → pgmq evolution  
**Audience**: Senior engineers and architects making architectural decisions  
**Key Value**: Lessons learned about distributed system design trade-offs

**When to Read**: 
- Planning architectural changes
- Evaluating messaging patterns
- Understanding why pgmq was chosen over alternatives

### 🔄 [Orchestration Principles](./orchestration-principles.md) 
**Purpose**: Fundamental workflow orchestration concepts and patterns  
**Audience**: Engineers building workflow management systems  
**Key Value**: Architecture-agnostic principles for robust orchestration

**When to Read**:
- Designing new workflow patterns
- Understanding DAG-based execution
- Implementing state machine integration
- Building event-driven orchestration systems

### 🧪 [Testing Methodologies](./testing-methodologies.md)
**Purpose**: Comprehensive testing strategies for complex orchestration systems  
**Audience**: Engineers building and maintaining orchestration platforms  
**Key Value**: Proven testing patterns that ensure reliability and maintainability

**When to Read**:
- Setting up testing infrastructure
- Validating DAG operations
- Testing FFI boundaries
- Implementing performance benchmarks
- Designing integration test suites

### ⚡ [Performance Optimization](./performance-optimization.md)
**Purpose**: Performance targets, analysis, and optimization strategies  
**Audience**: Engineers optimizing production orchestration systems  
**Key Value**: Concrete metrics and optimization techniques for high-scale performance

**When to Read**:
- Optimizing database queries
- Improving FFI performance
- Setting performance targets
- Diagnosing bottlenecks
- Planning capacity and scaling

### 🔗 [Ruby Integration Lessons](./ruby-integration-lessons.md)
**Purpose**: FFI design patterns and hard-won insights from Ruby-Rust integration  
**Audience**: Engineers implementing cross-language orchestration systems  
**Key Value**: Production-proven patterns for successful FFI integration

**When to Read**:
- Designing FFI boundaries
- Debugging cross-language issues
- Optimizing FFI performance
- Planning deployment strategies
- Implementing error handling across language boundaries

## How to Use This Archive

### For New Team Members
1. Start with **Orchestration Principles** to understand core concepts
2. Read **Architectural Evolution** for context on current design decisions
3. Review **Testing Methodologies** to understand quality expectations

### For Architectural Decisions
1. **Architectural Evolution** - Learn from past iterations and trade-offs
2. **Performance Optimization** - Understand performance implications
3. **Orchestration Principles** - Apply proven patterns to new challenges

### For Implementation Work
1. **Orchestration Principles** - Design patterns and best practices
2. **Ruby Integration Lessons** - FFI implementation guidance (if applicable)
3. **Testing Methodologies** - Quality assurance strategies
4. **Performance Optimization** - Optimization techniques and targets

### For Production Issues
1. **Performance Optimization** - Diagnostic approaches and solutions
2. **Ruby Integration Lessons** - FFI debugging and troubleshooting
3. **Testing Methodologies** - Validation and regression testing

## Document Maintenance

### Status
- **Last Consolidated**: August 2025
- **Source Material**: 32 legacy documentation files (now removed)
- **Current Architecture**: pgmq-based autonomous workers

### Maintenance Guidelines
- These documents contain **strategic insights** that transcend specific implementations
- **Do not modify** these documents for current project needs
- **Do create new documentation** for current architecture specifics
- **Do reference** these documents when making architectural decisions

### Related Current Documentation
- `../pgmq-pivot.md` - Current pgmq architecture details
- `../queue-processing.md` - Current queue processing implementation
- `../../CLAUDE.md` - Current project context and development guidelines

## Legacy Content Removed

The following categories of content were removed during consolidation:
- **Implementation-specific details** for obsolete architectures
- **TODO lists** and development plans for completed work
- **Placeholder code** and temporary solutions
- **Duplicate analysis** across multiple documents
- **Debug sessions** and troubleshooting logs

All valuable strategic insights from these removed documents have been preserved and consolidated into the five remaining documents.

## Document Quality

Each consolidated document follows these standards:
- **Evergreen Content**: Focuses on principles and patterns that remain valid
- **Production Proven**: Based on real-world implementation experience
- **Strategic Focus**: Emphasizes decision-making insights over implementation details
- **Clear Audience**: Targeted to specific engineering roles and use cases

---

**Archive Purpose**: Preserve strategic insights while removing implementation complexity  
**Maintenance Model**: Preserve unchanged, create new docs for current architecture  
**Last Updated**: August 2025