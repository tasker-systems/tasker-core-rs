# Tasker Core Rust Documentation

High-performance Rust orchestration core for the Tasker workflow engine.

## 📖 Current Documentation

### Core Architecture (Production)
- **[pgmq-pivot.md](pgmq-pivot.md)** - PostgreSQL message queue architecture (current production system)
- **[queue-processing.md](queue-processing.md)** - Queue-based workflow processing implementation

### Development Context
- **[../CLAUDE.md](../CLAUDE.md)** - Project overview, current status, and development guidelines
- **[../MEMORY.md](../MEMORY.md)** - Project memory and historical context

### API Reference
- **[openapi3.0](openapi3.0)** - OpenAPI 3.0 specification for Tasker engine

## 📚 Historical Documentation Archive

The **[archive/](archive/)** directory contains consolidated strategic insights from previous architectural iterations:

- **[Architectural Evolution](archive/architectural-evolution.md)** - Lessons from ZeroMQ → TCP → pgmq progression
- **[Orchestration Principles](archive/orchestration-principles.md)** - Core workflow management concepts and patterns  
- **[Testing Methodologies](archive/testing-methodologies.md)** - Comprehensive testing strategies for orchestration systems
- **[Performance Optimization](archive/performance-optimization.md)** - Performance targets and optimization techniques
- **[Ruby Integration Lessons](archive/ruby-integration-lessons.md)** - FFI design patterns and production insights

See **[archive/README.md](archive/README.md)** for guidance on using these strategic references.

## 🎯 Project Status

**Current Architecture**: PostgreSQL message queue (pgmq) based system  
**Status**: Phase 5.2 largely complete - Individual step enqueueing with metadata flow  
**Achievement**: Complete separation of Rust orchestration and Ruby worker execution  
**Performance**: Queue-based autonomous processing eliminating coordination complexity

## 🏗️ Architecture Overview

```
Rust Orchestrator → pgmq (PostgreSQL) ← Autonomous Ruby Workers
```

**Key Benefits**:
- ✅ **Autonomous Workers**: Ruby workers poll queues independently, no coordination required
- ✅ **Database-Centric**: Shared PostgreSQL state eliminates message coordination complexity  
- ✅ **Fault Tolerance**: Natural persistence and recovery through database queues
- ✅ **Rails Philosophy**: Return to proven simplicity of original Rails Tasker patterns

## 🚀 Quick Start

1. **Current System**: Review **pgmq-pivot.md** for the production architecture
2. **Development Context**: Check **../CLAUDE.md** for current status and guidelines
3. **Strategic Insights**: Browse **archive/** for architectural lessons and patterns
4. **Implementation Details**: See **queue-processing.md** for current processing logic

## 📋 Documentation Maintenance

- **Current Documentation**: `pgmq-pivot.md`, `queue-processing.md` - maintained actively
- **Archive Documentation**: `archive/` - strategic references, maintained as historical insights
- **Project Context**: `../CLAUDE.md`, `../MEMORY.md` - living documentation updated regularly

---

*This documentation reflects the current pgmq-based architecture. For historical context and strategic insights from previous iterations, see the [archive/](archive/) directory.*