# Tasker Core Rust

High-performance orchestration core for the Rails Tasker workflow engine, built with Rust for 10-100x performance improvements in critical bottlenecks.

## 🎯 **Current Status: Data Modeling Complete**

✅ **Phase 1 - FULLY IMPLEMENTED**: All 18+ Rails models migrated with comprehensive test coverage  
🔄 **Phase 2 - NEXT**: Step handler foundation and FFI bindings for multi-language integration  
🚀 **Performance**: 54% faster parallel test execution with database-level concurrency controls

## 🚀 Architecture

**Delegation-Based Orchestration Core** - Enhances existing Rails Tasker engine without replacing it:
- **Rust Core**: High-performance dependency resolution, state management, and coordination
- **Framework Integration**: Seamless handoff to existing step handlers and queue systems  
- **Zero Disruption**: Existing step handlers and business logic work unchanged

## 📚 Documentation

**➡️ [Complete Documentation in `docs/`](docs/README.md)**

### Quick Links
- **[Development Plan](docs/DEVELOPMENT_PLAN_REVISED.md)** - Detailed implementation roadmap
- **[Orchestration Analysis](docs/ORCHESTRATION_ANALYSIS.md)** - Rails integration patterns  
- **[Project Status](docs/PROJECT_STATUS.md)** - Current accomplishments and next steps
- **[Project Context](CLAUDE.md)** - High-level architecture and vision

## 🎯 Performance Targets

- **10-100x faster** dependency resolution vs PostgreSQL functions
- **<1ms overhead** per step coordination handoff
- **<10% FFI penalty** vs native Ruby execution  
- **>10k events/sec** cross-language event processing

## 🛠️ Current Status

**Phase**: Planning Complete → Ready for Implementation  
**Next**: Phase 1 - Foundation Layer (Database models + FFI interfaces)

Built to complement the production Rails Tasker engine at `/Users/petetaylor/projects/tasker/`
