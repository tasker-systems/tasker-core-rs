# Tasker Core Rust Documentation

## Project Documentation Index

### **📋 Planning & Status**
- **[PROJECT_MEMORY.md](PROJECT_MEMORY.md)** - ⭐ **Current progress tracking and next steps**
- **[PROJECT_STATUS.md](PROJECT_STATUS.md)** - Current project status and accomplishments
- **[DEVELOPMENT_PLAN_REVISED.md](DEVELOPMENT_PLAN_REVISED.md)** - Updated development plan with delegation-based architecture
- **[ORCHESTRATION_ANALYSIS.md](ORCHESTRATION_ANALYSIS.md)** - Deep analysis of Rails orchestration patterns and implications

### **🏗️ Architecture Documentation**
- **[../CLAUDE.md](../CLAUDE.md)** - Project context and high-level architecture overview
- **[ORCHESTRATION_ANALYSIS.md](ORCHESTRATION_ANALYSIS.md)** - Control flow analysis and delegation patterns
- **[SCHEMA_ANALYSIS.md](SCHEMA_ANALYSIS.md)** - Detailed SQL vs Rust model comparison

### **🧪 Testing & Analysis**
- **[testing-principles.md](testing-principles.md)** - ⭐ **Core testing principles and organization strategy**
- **[test-migration-analysis.md](test-migration-analysis.md)** - Comprehensive analysis and migration plan for all tests
- **[TESTING_STRATEGY.md](TESTING_STRATEGY.md)** - Comprehensive testing approach with property-based testing
- **[testing/](testing/)** - Detailed testing documentation and results

### **📊 Status Overview**

**Current Phase**: Model Schema Alignment (Phase 1)

**Recent Progress**: 
- ✅ Schema analysis complete - major field mismatches identified
- ✅ Rails models analysis complete - complex scopes documented  
- ✅ SQL function wrappers analyzed - 8 major functions identified
- ✅ Configuration system designed - 120+ options across 9 sections

**Current Focus**: Updating Rust models to match PostgreSQL schema exactly

**Architecture**: Delegation-Based Orchestration Core
- Rust handles performance-critical orchestration decisions
- Framework manages queuing, step execution, and business logic
- Seamless handoff patterns maintain compatibility

**Next Steps**: Begin immediate model updates with WorkflowStepEdge, Task, WorkflowStep

---

## Key Architecture Changes

**BEFORE**: Monolithic Rust replacement of Rails orchestration  
**AFTER**: High-performance Rust coordination core enhancing Rails architecture

**Benefits**:
- ✅ Existing step handlers work unchanged
- ✅ Queue system integration maintained  
- ✅ Performance bottlenecks solved (10-100x improvement targets)
- ✅ Gradual migration possible
- ✅ Zero-disruption deployment

---

## Quick Start References

### **For Implementation**
1. Read `DEVELOPMENT_PLAN_REVISED.md` for detailed component breakdown
2. Check `ORCHESTRATION_ANALYSIS.md` for Rails integration patterns
3. Review `PROJECT_STATUS.md` for current accomplishments

### **For Architecture Understanding**
1. Start with `ORCHESTRATION_ANALYSIS.md` for control flow patterns
2. Review `../CLAUDE.md` for project context
3. See `DEVELOPMENT_PLAN_REVISED.md` for implementation strategy

---

*Last Updated: 2025-07-01*