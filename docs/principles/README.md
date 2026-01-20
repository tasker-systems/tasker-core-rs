# Tasker Core Principles

This directory contains the core principles and design philosophy that guide Tasker Core development. These principles are not arbitrary rules but hard-won lessons extracted from implementation experience, root cause analyses, and architectural decisions.

## Core Documents

| Document | Description |
|----------|-------------|
| [Tasker Core Tenets](./tasker-core-tenets.md) | The 10 foundational principles that drive all architecture and design decisions |
| [Defense in Depth](./defense-in-depth.md) | Multi-layered protection model for idempotency and data integrity |
| [Cross-Language Consistency](./cross-language-consistency.md) | The "one API" philosophy for Rust, Ruby, Python, and TypeScript workers |
| [Composition Over Inheritance](./composition-over-inheritance.md) | Mixin-based handler composition pattern |
| [Intentional AI Partnership](./intentional-ai-partnership.md) | Collaborative approach to AI integration |

## Reference

| Document | Description |
|----------|-------------|
| [Zen of Python (PEP-20)](./zen-of-python-PEP-20.md) | Tim Peters' guiding principles - referenced as inspiration |

## How These Principles Were Derived

These principles emerged from:

1. **Root Cause Analyses**: TAS-54's ownership removal revealed that "redundant protection with harmful side effects" is worse than minimal, well-understood protection
2. **Cross-Language Development**: TAS-92/TAS-100/TAS-112 established patterns for consistent APIs across four languages
3. **Architectural Migrations**: TAS-46/TAS-67/TAS-69 proved the actor pattern's effectiveness
4. **Production Incidents**: Real bugs in parallel execution (Heisenbugs becoming Bohrbugs) shaped defensive design

## When to Consult These Documents

- **Design decisions**: Read [Tasker Core Tenets](./tasker-core-tenets.md) before proposing architecture changes
- **Adding protections**: Consult [Defense in Depth](./defense-in-depth.md) to understand existing layers
- **Worker development**: Review [Cross-Language Consistency](./cross-language-consistency.md) for API alignment
- **Handler patterns**: Study [Composition Over Inheritance](./composition-over-inheritance.md) for proper structure

## Related Documentation

- **Architecture Decisions**: `docs/decisions/` for specific ADRs
- **Historical Context**: `docs/CHRONOLOGY.md` for development timeline
- **Implementation Details**: `docs/ticket-specs/` for original specifications
