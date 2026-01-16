# Why Tasker

**Last Updated**: 2025-01-09
**Audience**: Engineers evaluating workflow orchestration tools
**Status**: Pre-Alpha

---

## The Story

Tasker is a labor of love.

Over the years, I've built workflow systems at multiple organizations—each time encountering the same fundamental challenges: orchestrating complex, multi-step processes with proper dependency management, ensuring idempotency, handling retries gracefully, and doing all of this in a way that doesn't require teams to rewrite their existing business logic.

Each time, I'd design parts of the solution I wished we could build—but the investment was never justifiable. General-purpose workflow infrastructure rarely makes sense for a single company to build from scratch when there are urgent product features to ship. So I'd compromise, cobble together something workable, and move on.

Tasker represents the opportunity to finally build that system properly—the one that's been evolving in my head for years. Not as a venture-backed startup chasing a market, but as open-source software built by someone who genuinely cares about the problem space and wants to give back to the engineering community.

---

## The Landscape

Honesty is important, and so in full candor: Tasker is not solving an unsolved problem. The workflow orchestration space has mature, battle-tested options.

### Apache Airflow

**What it does well:** Airflow is the industry standard for data pipeline orchestration. Born at Airbnb and now an Apache project with thousands of contributors, it excels at scheduled, batch-oriented workflows defined as Python DAGs. Its ecosystem of operators and integrations is unmatched—if you need to connect to a cloud service, there's probably an Airflow provider for it.

**When to choose it:** You have scheduled ETL/ELT workloads, your team is Python-native, you need managed cloud options (AWS MWAA, Google Cloud Composer, Astronomer), and you value ecosystem breadth over ergonomic simplicity.

**Honest comparison:** Airflow's 10+ years of production use across thousands of companies represents a level of battle-testing Tasker simply cannot match. If your primary use case is data pipeline orchestration with scheduled intervals, Airflow is likely the safer choice.

### Temporal

**What it does well:** Temporal pioneered "durable execution"—workflows that automatically survive crashes, network failures, and infrastructure outages. It reconstructs application state transparently, letting developers write code as if failures don't exist. The event history and replay capabilities are genuinely impressive.

**When to choose it:** You need long-running workflows (hours, days, or longer), your operations require true durability guarantees, you're building microservice orchestration with complex saga patterns, or you need human-in-the-loop workflows with unbounded wait times.

**Honest comparison:** Temporal's durable execution model is architecturally different from Tasker. If your workflows genuinely need to survive arbitrary failures mid-execution and resume from exact state, Temporal was purpose-built for this. Tasker provides resilience through retries and idempotent step execution, but doesn't offer Temporal's deterministic replay.

### Prefect

**What it does well:** Prefect feels like "what if workflow orchestration were just Python decorators?" It emphasizes minimal boilerplate—add `@flow` and `@task` decorators to existing functions, and you have an orchestrated workflow. Prefect 3.0 embraces dynamic workflows with native Python control flow.

**When to choose it:** Your team is Python-native, you want the fastest path from script to production pipeline, you value simplicity and developer experience, or you're doing ML/data science workflows where Jupyter-to-production is important.

**Honest comparison:** Prefect's decorator-based ergonomics are genuinely excellent for Python-only teams. If your organization is homogeneously Python and you don't need polyglot support, Prefect delivers a very clean experience.

### Dagster

**What it does well:** Dagster introduced "software-defined assets" as first-class primitives—you define what data assets should exist and their dependencies, and the orchestrator figures out how to materialize them. This asset-centric model provides excellent lineage tracking and observability.

**When to choose it:** You're building a data platform where understanding asset lineage is critical, you want a declarative approach focused on data products rather than task sequences, or you need strong dbt integration and data quality built into your orchestration layer.

**Honest comparison:** Dagster's asset-centric philosophy is a genuinely different way of thinking about orchestration. If your mental model centers on "what data assets need to exist" rather than "what steps need to execute," Dagster may be a better conceptual fit.

---

## So Why Tasker?

Given this landscape, why build another workflow orchestrator?

### Philosophy: Meet Teams Where They Are

Most workflow tools require you to think in their terms. Define your work as DAGs using their DSL. Adopt their scheduling model. Often, rewrite your business logic to fit their execution model.

Tasker takes a different approach: **bring workflow orchestration to your existing code, rather than bringing your code to a workflow framework.**

If your codebase already has reasonable SOLID characteristics—services with clear responsibilities, well-defined interfaces, operations that can be made idempotent—Tasker aims to turn that code into distributed, event-driven, retryable workflows with minimal ceremony.

This philosophy manifests in several ways:

**Polyglot from the ground up.** Tasker's orchestration engine is written in Rust, but workers can be written in Ruby, Python, TypeScript, or native Rust. Each language implementation shares the same core abstractions—same handler signatures, same result factories, same patterns—expressed idiomatically for each language. This isn't an afterthought; cross-language consistency is a core design principle.

**Minimal migration burden.** Your existing business logic—API calls, database operations, external service integrations—can become step handlers with thin wrappers. You don't need to restructure your application around the orchestrator.

**Framework-agnostic core.** Tasker Core provides the fundamentals without framework opinions. Tasker Contrib then provides framework-specific integrations (Rails, FastAPI, Bun) for teams who want batteries-included ergonomics. Progressive disclosure: learn the core concepts first, add framework sugar when needed.

### Architecture: Event-Driven with Resilience Built In

Tasker's architecture reflects lessons learned from building distributed systems:

**PostgreSQL-native by default.** Everything flows through Postgres—task state, step queues (via PGMQ), event coordination (via LISTEN/NOTIFY). This isn't because Postgres is trendy; it's because many teams already have Postgres expertise and operational knowledge. Tasker works as a single-dependency system on PostgreSQL alone. For environments requiring higher throughput or existing RabbitMQ infrastructure, Tasker also supports RabbitMQ as an alternative messaging backend—switch with a configuration change.

**Event-driven with polling fallback.** Real-time responsiveness through Postgres notifications, but with polling as a reliability backstop. Events can be missed; polling ensures eventual consistency.

**Defense in depth.** Multiple overlapping protection layers provide robust idempotency without single-point dependency. Database-level atomicity, state machine guards, transaction boundaries, and application-level filtering each catch what others might miss.

**Composition over inheritance.** Handler capabilities are composed via mixins/traits, not class hierarchies. This enables selective capability inclusion, clear separation of concerns, and easier testing.

### Performance: Fast by Default

Tasker is built in Rust not for marketing purposes, but because workflow orchestration has real performance implications. When you're coordinating thousands of steps across distributed workers, overhead matters.

- Complex 7-step DAG workflows complete in under 133ms with push-based notifications
- Concurrent execution via work-stealing thread pools
- Lock-free channel-based internal coordination
- Zero-copy where possible in the FFI boundaries

### The Honest Assessment

Tasker excels when:
- You need polyglot worker support across Ruby, Python, TypeScript, and Rust
- Your team already has Postgres expertise and wants to avoid additional infrastructure
- You want to bring orchestration to existing business logic rather than rewriting
- You value clean, consistent APIs across languages
- Performance matters and you're willing to trade ecosystem breadth for it

Tasker may not be the right choice when:
- You need the battle-tested maturity and ecosystem of Airflow
- Your workflows require Temporal-style durable execution with deterministic replay
- You're an all-Python team and Prefect's ergonomics fit perfectly
- You're building a data platform where asset-centric thinking (Dagster) is the right model
- You need managed cloud offerings with SLAs and enterprise support

---

## What Tasker Is (and Isn't)

### Tasker Is:

- **A workflow orchestration engine** for step-based DAG execution with complex dependencies
- **PostgreSQL-native with flexible messaging** using PGMQ (default) or RabbitMQ
- **Polyglot by design** with first-class support for multiple languages
- **Focused on developer experience** for teams who want minimal intrusion
- **Open source (MIT license)** and built as a labor of love

### Tasker Is Not:

- **A data orchestration platform** like Dagster with asset lineage and data quality primitives
- **A durable execution engine** like Temporal with deterministic replay and unlimited durability
- **A scheduled job runner** for simple cron-style workloads (use actual cron)
- **A message bus** for pure pub/sub fan-out (use Kafka or dedicated brokers)
- **Enterprise software** with commercial support, SLAs, or managed offerings

---

## Current State

Tasker is pre-alpha software. This is important context:

**What this means:**
- The architecture is solidifying but breaking changes are expected
- Documentation is comprehensive but evolving
- There are no production deployments (that I know of) outside development
- You should not bet critical business processes on Tasker today

**What this enables:**
- Rapid iteration based on real feedback
- Willingness to break APIs to get the design right
- Focus on architectural correctness over backward compatibility
- Honest experimentation without legacy constraints

If you're evaluating Tasker, I'd encourage you to explore it for non-critical workloads, provide feedback, and help shape what it becomes. If you need production-ready workflow orchestration today, please consider the established tools above—I genuinely recommend them for their respective strengths.

---

## The Path Forward

Tasker is being built with care, not speed. The goal isn't to capture market share or compete with well-funded companies. The goal is to create something genuinely useful—a workflow orchestration system that respects developers' time and meets them where they are.

The codebase is open, the design decisions are documented, and contributions are welcome. This is software built by an engineer for engineers, not a product chasing metrics.

If that resonates with you, welcome. Let's build something good together.

---

## Related Documentation

- **[Tasker Core Tenets](principles/tasker-core-tenets.md)** - The 10 foundational design principles
- **[Use Cases & Patterns](guides/use-cases-and-patterns.md)** - When and how to use Tasker
- **[Quick Start Guide](guides/quick-start.md)** - Get running in 5 minutes
- **[CHRONOLOGY](CHRONOLOGY.md)** - Development timeline and lessons learned

---

← Back to [Documentation Hub](README.md)
