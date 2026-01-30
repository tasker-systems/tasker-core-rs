# TAS-175: Askama-Based Configuration Documentation Generation

**Branch:** `claude/compare-rust-templating-V062T`
**Date:** 2026-01-29
**Status:** Plan
**Linear:** [TAS-175](https://linear.app/tasker-systems/issue/TAS-175)

---

## Background

Tasker has ~303 configurable parameters across 51 distinct config sections (common, orchestration, worker). The configuration system is mature: `ConfigMerger` deep-merges base TOML + environment overlays into deployable files, `ConfigLoader` deserializes them into the validated `TaskerConfig` hierarchy, and `ConfigDocumentation` infrastructure supports a `_docs` metadata convention.

However:

1. **Minimal `_docs` coverage.** The infrastructure is implemented and Phase 0 seeds ~95 parameters with documentation metadata, but the majority of ~303 parameters remain undocumented. The `tasker-cli config explain` command can render Phase 0 entries.

2. **No rendered documentation.** There's no way to generate a browsable config reference, annotated config examples, or per-section deep-dives. Configuration knowledge lives in code comments and the heads of contributors.

3. **No interstitial layer.** Documentation output is hardcoded `println!` in the CLI. There's no separation between documentation content (what to say about a parameter) and documentation presentation (how to format it for markdown, CLI, annotated TOML).

**Goal:** Use [Askama](https://github.com/askama-rs/askama) (compile-time Jinja2-style templates with struct-level binding) to build a documentation generation pipeline that:

- Renders multiple output formats (markdown, plain text, annotated TOML) from the same source data
- Uses template hierarchies (`extends`, `block`) for consistent structure across formats
- Leverages compile-time checking — template field references are verified at `cargo build`
- Integrates with the existing `tasker-cli` as a `docs` subcommand group
- Does **NOT** replace `ConfigMerger` or `ConfigLoader` — those remain the config generation path

### Why Askama (Not Tera)

Evaluated in detail before this ticket. Key factors:

| Concern | Askama | Tera |
|---------|--------|------|
| Context binding | Struct fields = template variables | String-keyed `BTreeMap<String, serde_json::Value>` |
| Error detection | Compile-time (field typos, missing vars) | Runtime only |
| Template variable drift | Build breaks if struct changes | Silent wrong output |
| Runtime template loading | Not supported | Supported |
| Hot reload | Not supported | Supported |

Runtime template loading and hot reload are not needed — they're antipatterns in this context. Users should not supply custom doc templates; documentation must be authoritative and consistent. Askama's struct binding is a natural extension of the existing type-safe architecture (`TaskerConfig` + `validator` + `bon::Builder`).

---

## Architecture

### Data Flow

```
  config/tasker/base/*.toml          tasker-shared/src/config/tasker.rs
  (_docs metadata sections)          (struct definitions, defaults, ranges)
         │                                        │
         ▼                                        ▼
  ConfigDocumentation::load()        TaskerConfig::default() → serialize
         │                                        │
         └──────────┬─────────────────────────────┘
                    ▼
         DocContext (unified view)
         ├── sections: Vec<SectionContext>
         │    ├── parameters: Vec<ParameterContext>
         │    └── subsections: Vec<SectionContext>
         └── metadata (context_name, env, timestamp)
                    │
                    ▼
         Askama Template structs (#[derive(Template)])
         ├── ConfigReferenceTemplate    → full markdown reference
         ├── SectionDetailTemplate      → single-section deep-dive
         ├── AnnotatedConfigTemplate    → commented TOML example
         ├── ParameterExplainTemplate   → CLI explain output
         └── DocIndexTemplate           → documentation index
                    │
                    ▼
         Rendered output (String)
         ├── docs/generated/config-reference.md
         ├── docs/generated/sections/*.md
         ├── docs/generated/annotated-*.toml
         └── stdout (CLI explain)
```

### Key Design Decisions

**1. Two data sources, one context.**
Struct metadata (actual defaults from `TaskerConfig::default()`, type names, field names) and human-authored content (descriptions, system impact, recommendations from `_docs`) merge into a single `DocContext`. Templates never reach into two different data sources.

**2. Askama lives in `tasker-client` only.**
The `#[derive(Template)]` structs and template files are in `tasker-client`. This keeps compile-time template expansion out of `tasker-shared`, `tasker-orchestration`, and `tasker-worker`. The context builder types live in `tasker-shared` since they depend on `TaskerConfig` and `ConfigDocumentation`.

**3. Templates directory alongside binary source.**
```
tasker-client/
├── templates/              ← Askama template files
│   ├── base.md             ← shared layout with blocks
│   ├── config-reference.md
│   ├── section-detail.md
│   ├── annotated-config.toml
│   ├── parameter-explain.txt
│   └── doc-index.md
├── src/
│   ├── bin/cli/commands/
│   │   ├── config.rs       ← existing (enhance explain)
│   │   └── docs.rs         ← new docs subcommand
│   └── docs/               ← new module
│       ├── mod.rs
│       ├── context.rs      ← DocContext builder
│       └── templates.rs    ← Askama #[derive(Template)] structs
```

Askama resolves templates relative to a configurable directory (set via `askama.toml` or the `path` attribute). We'll set the template root to `tasker-client/templates/`.

**4. `_docs` population is a separate, incremental effort.**
The template system works even with zero `_docs` coverage — it falls back to structural information (field name, type, default value). As `_docs` are added to base TOML files, the rendered output gets richer. This decouples the template engine implementation from the content authoring effort.

**5. Feature-gated dependency.**
Askama is behind a `docs-gen` feature in `tasker-client` (default-enabled) so it can be excluded from minimal builds if needed.

---

## Implementation Plan

### Phase 0: Seed `_docs` Coverage (Low-Hanging Fruit)

Before building the Askama pipeline, populate `_docs` entries for self-evident parameters across all three base TOML files. This provides:

1. **Proof data** for validating the Askama pipeline against real content
2. **Verification** that `ConfigMerger` and `ConfigLoader` correctly strip `_docs` from generated output (the stripping logic in `merge.rs:deep_merge_tables` skips keys ending with `_docs`, and `strip_docs_sections` removes any remaining)
3. **Immediate value** — the existing `tasker-cli config explain` infrastructure can render these even before Askama templates exist

**Scope: ~95 parameters documented across 3 files.**

Selection criteria: parameters whose purpose is self-evident from their name, type, and context — URLs, backend selectors, queue names, bind addresses, boolean toggles, timeout/buffer sizes, IDs, and concurrency limits. These don't require deep architectural research to describe accurately.

| File | Sections Covered | Params Documented |
|------|-----------------|-------------------|
| `common.toml` | system, database (url, pool, variables), pgmq_database, queues (backend, namespaces, orchestration_queues, pgmq, queue_depth_thresholds, rabbitmq), circuit_breakers (enabled, global_settings, default_config), execution, backoff (delays, reenqueue_delays), cache (enabled, backend, redis, moka) | ~70 |
| `orchestration.toml` | mode, enable_performance_logging, event_systems (system_id, deployment_mode), dlq (enabled, auto_dlq, staleness_detection, thresholds, actions), web (enabled, bind_address, request_timeout, auth), grpc (enabled, bind_address, tls, reflection, health) | ~18 |
| `worker.toml` | worker_id, worker_type, circuit_breakers (slow_send_threshold), event_systems (system_id, deployment_mode), step_processing, health_monitoring, orchestration_client (base_url, timeout, retries), web (enabled, bind_address, auth), grpc (enabled, bind_address, max_concurrent_streams) | ~17 |

Each `_docs` entry follows the established convention from `documentation.rs`:
```toml
[section._docs.parameter_name]
description = "Human-readable description"
type = "Rust type name"
valid_range = "Value constraints"
system_impact = "What this controls and what happens at boundary values"
related = ["other.param.paths"]  # optional
example = "..."                   # optional

[section._docs.parameter_name.recommendations]  # optional
test = { value = "...", rationale = "..." }
production = { value = "...", rationale = "..." }
```

**Not in Phase 0 scope**: MPSC channel buffer sizes (require understanding actor message flow), event system timing parameters (require understanding deployment mode interactions), component-specific circuit breaker tuning. These are deferred to Phase 4 tiers.

### Phase 1: Foundation (Askama Integration + Context Types)

**Add Askama to the workspace.**

`Cargo.toml` (workspace):
```toml
[workspace.dependencies]
askama = "0.15"
```

`tasker-client/Cargo.toml`:
```toml
[features]
default = ["grpc", "docs-gen"]
docs-gen = ["askama"]

[dependencies]
askama = { workspace = true, optional = true }
```

`tasker-client/askama.toml` (Askama configuration):
```toml
[general]
dirs = ["templates"]
```

**Define context types in `tasker-shared`.**

New file `tasker-shared/src/config/doc_context.rs`:

```rust
use super::documentation::ParameterDocumentation;
use serde::Serialize;

/// Unified documentation context for a configuration section.
///
/// Combines structural metadata (from TaskerConfig) with human-authored
/// documentation (from _docs sections). Templates consume this type.
#[derive(Debug, Clone, Serialize)]
pub struct SectionContext {
    /// Section name (e.g., "database", "pool")
    pub name: String,
    /// Dotted path from root (e.g., "common.database.pool")
    pub path: String,
    /// Human description (from _docs, or auto-generated fallback)
    pub description: String,
    /// Config context this section belongs to
    pub config_context: ConfigContext,
    /// Parameters in this section (leaf values)
    pub parameters: Vec<ParameterContext>,
    /// Nested subsections
    pub subsections: Vec<SectionContext>,
}

/// Unified documentation context for a single parameter.
#[derive(Debug, Clone, Serialize)]
pub struct ParameterContext {
    /// Field name (e.g., "max_connections")
    pub name: String,
    /// Dotted path from root (e.g., "common.database.pool.max_connections")
    pub path: String,
    /// Rust type name (e.g., "u32", "String", "bool")
    pub rust_type: String,
    /// Serialized default value from TaskerConfig::default()
    pub default_value: String,
    /// Validation range (e.g., "1-1000") — from _docs or validator attributes
    pub valid_range: String,
    /// Human description — from _docs, empty if undocumented
    pub description: String,
    /// System impact — from _docs
    pub system_impact: String,
    /// Related parameter paths — from _docs
    pub related: Vec<String>,
    /// Example TOML snippet — from _docs
    pub example: String,
    /// Per-environment recommendations — from _docs
    pub recommendations: Vec<RecommendationContext>,
    /// Whether this parameter has _docs coverage
    pub is_documented: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct RecommendationContext {
    pub environment: String,
    pub value: String,
    pub rationale: String,
}

/// Which config context a section belongs to
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum ConfigContext {
    Common,
    Orchestration,
    Worker,
}
```

**Implement `DocContextBuilder` in `tasker-shared`.**

New file `tasker-shared/src/config/doc_context_builder.rs`:

```rust
/// Builds DocContext by combining:
/// 1. TaskerConfig::default() serialized to toml::Value (structural metadata)
/// 2. ConfigDocumentation (human-authored _docs content)
pub struct DocContextBuilder {
    config_defaults: toml::Value,
    documentation: Option<ConfigDocumentation>,
}

impl DocContextBuilder {
    pub fn new() -> ConfigResult<Self> {
        // Serialize default config to extract actual defaults
        let defaults = TaskerConfig::default();
        let config_defaults = toml::Value::try_from(&defaults)?;
        Ok(Self { config_defaults, documentation: None })
    }

    pub fn with_documentation(mut self, docs: ConfigDocumentation) -> Self {
        self.documentation = Some(docs);
        self
    }

    /// Build section contexts for a given config context
    pub fn build_sections(&self, context: ConfigContext) -> Vec<SectionContext> {
        // Walk the TOML value tree for the relevant context
        // Cross-reference with _docs for each parameter
        // Return structured section hierarchy
    }

    /// Build context for a single parameter lookup
    pub fn build_parameter(&self, path: &str) -> Option<ParameterContext> {
        // Look up in both TOML defaults and _docs
    }
}
```

The builder walks `TaskerConfig::default()` serialized as `toml::Value` to discover all parameters, their types (inferred from TOML value type), and defaults. It cross-references with `ConfigDocumentation` for human-authored content.

### Phase 2: Core Templates

**Template hierarchy:**

```
templates/
├── base.md                    ← shared markdown layout
├── config-reference.md        ← full reference (extends base.md)
├── section-detail.md          ← single section (extends base.md)
├── annotated-config.toml      ← standalone (no inheritance, TOML output)
├── parameter-explain.txt      ← standalone (plain text for CLI)
└── doc-index.md               ← index page (extends base.md)
```

**Template struct definitions in `tasker-client/src/docs/templates.rs`:**

```rust
use askama::Template;
use tasker_shared::config::doc_context::{
    SectionContext, ParameterContext, ConfigContext,
};

/// Full configuration reference document
#[derive(Template)]
#[template(path = "config-reference.md")]
pub struct ConfigReferenceTemplate<'a> {
    pub context_name: &'a str,
    pub sections: &'a [SectionContext],
    pub total_parameters: usize,
    pub documented_parameters: usize,
    pub generation_timestamp: &'a str,
}

/// Single section deep-dive
#[derive(Template)]
#[template(path = "section-detail.md")]
pub struct SectionDetailTemplate<'a> {
    pub section: &'a SectionContext,
    pub context_name: &'a str,
    pub environment: Option<&'a str>,
}

/// Annotated TOML config with documentation comments
#[derive(Template)]
#[template(path = "annotated-config.toml")]
pub struct AnnotatedConfigTemplate<'a> {
    pub context_name: &'a str,
    pub environment: &'a str,
    pub sections: &'a [SectionContext],
}

/// Single parameter explanation (CLI output)
#[derive(Template)]
#[template(path = "parameter-explain.txt")]
pub struct ParameterExplainTemplate<'a> {
    pub parameter: &'a ParameterContext,
    pub environment: Option<&'a str>,
}

/// Documentation index page
#[derive(Template)]
#[template(path = "doc-index.md")]
pub struct DocIndexTemplate<'a> {
    pub common_sections: &'a [SectionContext],
    pub orchestration_sections: &'a [SectionContext],
    pub worker_sections: &'a [SectionContext],
    pub total_parameters: usize,
    pub documented_parameters: usize,
}
```

**Example template content — `config-reference.md`:**

```jinja
{% extends "base.md" %}

{% block title %}Configuration Reference: {{ context_name }}{% endblock %}

{% block content %}
> {{ documented_parameters }}/{{ total_parameters }} parameters documented
> Generated: {{ generation_timestamp }}

{% for section in sections %}
## {{ section.name }}

{{ section.description }}

**Path:** `{{ section.path }}`

{% if !section.parameters.is_empty() %}
| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
{% for p in section.parameters %}
| `{{ p.name }}` | `{{ p.rust_type }}` | `{{ p.default_value }}` | {{ p.description }} |
{% endfor %}

{% for p in section.parameters %}
{% if p.is_documented %}
### `{{ p.path }}`

{{ p.description }}

- **Type:** `{{ p.rust_type }}`
- **Default:** `{{ p.default_value }}`
- **Valid Range:** {{ p.valid_range }}
- **System Impact:** {{ p.system_impact }}

{% if !p.recommendations.is_empty() %}
**Environment Recommendations:**

| Environment | Value | Rationale |
|-------------|-------|-----------|
{% for rec in p.recommendations %}
| {{ rec.environment }} | {{ rec.value }} | {{ rec.rationale }} |
{% endfor %}
{% endif %}

{% if !p.related.is_empty() %}
**Related:** {% for r in p.related %}`{{ r }}`{% if !loop.last %}, {% endif %}{% endfor %}
{% endif %}

{% if !p.example.is_empty() %}
**Example:**
```toml
{{ p.example }}
```
{% endif %}

{% endif %}
{% endfor %}
{% endif %}

{% for sub in section.subsections %}
{% include "section-detail.md" %}
{% endfor %}

{% endfor %}
{% endblock %}
```

**Example template — `annotated-config.toml`:**

```jinja
# Tasker Configuration Reference - {{ context_name }}
# Environment: {{ environment }}
#
# This is an annotated configuration file showing all available parameters
# with their default values and documentation.
#
# Generate a deployable config: tasker-cli config generate
# Look up a parameter:         tasker-cli config explain -p <path>

{% for section in sections %}
{% call render_section(section, 0) %}
{% endfor %}

{% macro render_section(section, depth) %}
# {{ "─".repeat(60 - depth * 2) }}
# {{ section.name }}{% if !section.description.is_empty() %}: {{ section.description }}{% endif %}
# {{ "─".repeat(60 - depth * 2) }}
[{{ section.path }}]
{% for p in section.parameters %}
{% if p.is_documented %}
# {{ p.description }}
# Type: {{ p.rust_type }} | Range: {{ p.valid_range }}
{% endif %}
{{ p.name }} = {{ p.default_value }}
{% endfor %}

{% for sub in section.subsections %}
{% call render_section(sub, depth + 1) %}
{% endfor %}
{% endmacro %}
```

**Example template — `parameter-explain.txt`:**

```jinja
Configuration Parameter: {{ parameter.path }}

  {{ parameter.description }}

  Type:        {{ parameter.rust_type }}
  Default:     {{ parameter.default_value }}
  Valid Range: {{ parameter.valid_range }}

{% if !parameter.system_impact.is_empty() %}
  System Impact:
    {{ parameter.system_impact }}

{% endif %}
{% if !parameter.recommendations.is_empty() %}
  Environment Recommendations:
{% for rec in parameter.recommendations %}
{% if environment.is_none() || environment == Some(rec.environment.as_str()) %}
    {{ rec.environment }}:
      Value:     {{ rec.value }}
      Rationale: {{ rec.rationale }}
{% endif %}
{% endfor %}
{% endif %}
{% if !parameter.related.is_empty() %}
  Related Parameters:
{% for r in parameter.related %}
    - {{ r }}
{% endfor %}
{% endif %}
{% if !parameter.example.is_empty() %}
  Example:
{{ parameter.example }}
{% endif %}
```

### Phase 3: CLI Integration

**New `DocsCommands` subcommand group in `tasker-cli.rs`:**

```rust
#[derive(Debug, Subcommand)]
pub enum DocsCommands {
    /// Generate full configuration reference documentation
    Reference {
        /// Configuration context (common, orchestration, worker, all)
        #[arg(short, long, default_value = "all")]
        context: String,

        /// Output format (markdown, toml, text)
        #[arg(short, long, default_value = "markdown")]
        format: String,

        /// Write to file instead of stdout
        #[arg(short, long)]
        output: Option<String>,

        /// Base config directory for _docs metadata
        #[arg(long, default_value = "config/tasker/base")]
        base_dir: String,
    },

    /// Generate annotated configuration example
    Annotated {
        /// Configuration context
        #[arg(short, long, default_value = "complete")]
        context: String,

        /// Target environment for recommendations
        #[arg(short, long, default_value = "production")]
        environment: String,

        /// Write to file instead of stdout
        #[arg(short, long)]
        output: Option<String>,

        /// Base config directory for _docs metadata
        #[arg(long, default_value = "config/tasker/base")]
        base_dir: String,
    },

    /// Generate documentation for a specific section
    Section {
        /// Section path (e.g., "common.database.pool", "orchestration.dlq")
        #[arg(short, long)]
        path: String,

        /// Target environment for recommendations
        #[arg(short, long)]
        environment: Option<String>,

        /// Write to file instead of stdout
        #[arg(short, long)]
        output: Option<String>,

        /// Base config directory for _docs metadata
        #[arg(long, default_value = "config/tasker/base")]
        base_dir: String,
    },

    /// Show documentation coverage statistics
    Coverage {
        /// Base config directory for _docs metadata
        #[arg(long, default_value = "config/tasker/base")]
        base_dir: String,
    },
}
```

**Enhance `config explain` to use Askama templates.**

Replace the hardcoded `println!` formatting in `config.rs` (lines 362-538) with:

```rust
ConfigCommands::Explain { parameter, context, environment } => {
    let builder = DocContextBuilder::new()?;
    let builder = if let Some(docs) = load_docs_if_available() {
        builder.with_documentation(docs)
    } else {
        builder
    };

    if let Some(param_path) = parameter {
        if let Some(param_ctx) = builder.build_parameter(&param_path) {
            let template = ParameterExplainTemplate {
                parameter: &param_ctx,
                environment: environment.as_deref(),
            };
            println!("{}", template.render()?);
        } else {
            eprintln!("Parameter not found: {}", param_path);
        }
    } else if let Some(ctx) = context {
        // List parameters for context using template
        let config_ctx = parse_config_context(&ctx)?;
        let sections = builder.build_sections(config_ctx);
        // render section listing...
    }
}
```

### Phase 4: `_docs` Population (Incremental)

This is the content authoring effort. It proceeds independently from the template engine implementation and can be done incrementally across multiple PRs.

**Priority order** (by user-facing impact):

1. **Tier 1 — Most frequently adjusted** (~30 params):
   - `common.database.pool.*` (6 params)
   - `common.execution.*` (9 params)
   - `common.queues.pgmq.*` (4 params)
   - `worker.step_processing.*` (3 params)
   - `worker.event_systems.worker.metadata.fallback_poller.*` (7 params)

2. **Tier 2 — Important for production tuning** (~50 params):
   - `common.circuit_breakers.*` (all component configs)
   - `common.backoff.*` (all reenqueue delays)
   - `orchestration.dlq.staleness_detection.*`
   - `orchestration.batch_processing.*`
   - `common.mpsc_channels.*` and orchestration/worker channel configs
   - `*.web.auth.*` (auth configuration)

3. **Tier 3 — System operators** (~80 params):
   - `orchestration.event_systems.*` (timing, processing, health)
   - `worker.event_systems.*` (metadata, listener, resource limits)
   - `*.web.*` (bind address, timeouts, database pools)
   - `*.grpc.*` (gRPC configuration)
   - `common.queues.rabbitmq.*`
   - `common.cache.*` (redis, moka, memcached)

4. **Tier 4 — Everything else** (~140 params):
   - Worker MPSC channel sub-configs
   - Orchestration MPSC channel sub-configs
   - Overflow policy configs
   - Health monitoring configs
   - Remaining event system sub-configs

**`_docs` authoring pattern:**

```toml
# In config/tasker/base/common.toml:

[common.database.pool]
max_connections = 25
min_connections = 5

[common.database.pool._docs.max_connections]
description = "Maximum number of concurrent database connections in the pool"
type = "u32"
valid_range = "1-1000"
default = "25"
system_impact = "Controls database connection concurrency. Too few causes query queuing under load. Too many risks DB resource exhaustion and OS file descriptor limits."
related = ["common.database.pool.min_connections", "common.database.pool.acquire_timeout_seconds"]
example = """
[common.database.pool]
max_connections = 50  # Production: scale with worker count
"""

[common.database.pool._docs.max_connections.recommendations]
test = { value = "5-10", rationale = "Minimal connections for test isolation" }
development = { value = "10-25", rationale = "Moderate pool for local development" }
production = { value = "30-50", rationale = "Scale based on worker count and concurrent task volume" }
```

### Phase 5: Documentation Pipeline Integration

**Generated docs directory:**

```
docs/generated/
├── config-reference-common.md
├── config-reference-orchestration.md
├── config-reference-worker.md
├── config-reference-complete.md
├── sections/
│   ├── database-pool.md
│   ├── circuit-breakers.md
│   ├── mpsc-channels.md
│   └── ...
├── annotated-production.toml
├── annotated-development.toml
└── index.md
```

**cargo-make tasks:**

```toml
# In Makefile.toml:
[tasks.docs-generate]
description = "Generate configuration documentation"
command = "cargo"
args = ["run", "--package", "tasker-client", "--bin", "tasker-cli", "--features", "docs-gen",
        "--", "docs", "reference", "--context", "all", "--output", "docs/generated/config-reference-complete.md"]

[tasks.docs-coverage]
description = "Show documentation coverage"
command = "cargo"
args = ["run", "--package", "tasker-client", "--bin", "tasker-cli", "--features", "docs-gen",
        "--", "docs", "coverage"]

[tasks.docs-annotated]
description = "Generate annotated config examples"
script = '''
cargo run --package tasker-client --bin tasker-cli --features docs-gen -- \
    docs annotated --context complete --environment production --output docs/generated/annotated-production.toml
cargo run --package tasker-client --bin tasker-cli --features docs-gen -- \
    docs annotated --context complete --environment development --output docs/generated/annotated-development.toml
'''
```

**CI freshness check (future):**

```yaml
# Verify generated docs are up-to-date
- name: Check documentation freshness
  run: |
    cargo make docs-generate
    git diff --exit-code docs/generated/
```

---

## Files to Create

| File | Purpose |
|------|---------|
| `tasker-shared/src/config/doc_context.rs` | `SectionContext`, `ParameterContext`, `ConfigContext` types |
| `tasker-shared/src/config/doc_context_builder.rs` | `DocContextBuilder` — merges struct defaults + `_docs` |
| `tasker-client/askama.toml` | Askama configuration (template directory) |
| `tasker-client/templates/base.md` | Shared markdown layout with blocks |
| `tasker-client/templates/config-reference.md` | Full config reference template |
| `tasker-client/templates/section-detail.md` | Section deep-dive template |
| `tasker-client/templates/annotated-config.toml` | Annotated TOML template |
| `tasker-client/templates/parameter-explain.txt` | CLI explain output template |
| `tasker-client/templates/doc-index.md` | Documentation index template |
| `tasker-client/src/docs/mod.rs` | Docs module root |
| `tasker-client/src/docs/context.rs` | Context building for CLI (wraps `DocContextBuilder`) |
| `tasker-client/src/docs/templates.rs` | `#[derive(Template)]` struct definitions |
| `tasker-client/src/bin/cli/commands/docs.rs` | `DocsCommands` handler |

## Files to Modify

| File | Change |
|------|--------|
| `Cargo.toml` (workspace) | Add `askama = "0.15"` to workspace dependencies |
| `tasker-client/Cargo.toml` | Add `askama` dependency, `docs-gen` feature |
| `tasker-shared/Cargo.toml` | No changes needed (context types use existing deps) |
| `tasker-shared/src/config/mod.rs` | Export `doc_context` and `doc_context_builder` modules |
| `tasker-client/src/bin/tasker-cli.rs` | Add `Docs(DocsCommands)` variant to CLI enum |
| `tasker-client/src/bin/cli/commands/mod.rs` | Add `docs` module |
| `tasker-client/src/bin/cli/commands/config.rs` | Refactor `Explain` handler to use `ParameterExplainTemplate` |
| `tasker-client/src/lib.rs` | Add `docs` module (feature-gated) |
| `config/tasker/base/common.toml` | Add `_docs` sections (Phase 4, incremental) |
| `config/tasker/base/orchestration.toml` | Add `_docs` sections (Phase 4, incremental) |
| `config/tasker/base/worker.toml` | Add `_docs` sections (Phase 4, incremental) |

---

## Dependencies

### Compile-Time

| Crate | Version | Purpose | Scope |
|-------|---------|---------|-------|
| `askama` | 0.15 | Template engine (proc macro) | `tasker-client` only |

No new runtime dependencies. Askama generates inline Rust code — no template parser or interpreter at runtime.

### Build-Time Impact

Askama's proc macro adds compile time proportional to template count and complexity. With ~6 templates, the impact is marginal. Template compilation errors surface as standard Rust compiler errors with file/line context.

---

## Verification

### Unit Tests

```rust
// In tasker-shared - doc_context_builder tests
#[test]
fn test_builder_discovers_all_parameters() {
    let builder = DocContextBuilder::new().unwrap();
    let sections = builder.build_sections(ConfigContext::Common);
    // Verify all known common sections are present
    assert!(sections.iter().any(|s| s.name == "database"));
    assert!(sections.iter().any(|s| s.name == "execution"));
}

#[test]
fn test_builder_merges_docs_metadata() {
    let builder = DocContextBuilder::new().unwrap()
        .with_documentation(load_test_docs());
    let param = builder.build_parameter("database.pool.max_connections").unwrap();
    assert!(!param.description.is_empty());
    assert!(param.is_documented);
}

#[test]
fn test_builder_works_without_docs() {
    let builder = DocContextBuilder::new().unwrap();
    let param = builder.build_parameter("common.database.pool.max_connections").unwrap();
    // Structural metadata present even without _docs
    assert_eq!(param.rust_type, "integer");  // from TOML value type
    assert!(!param.default_value.is_empty());
    assert!(!param.is_documented);  // no _docs present
}
```

### Template Rendering Tests

```rust
// In tasker-client - template rendering tests
#[cfg(feature = "docs-gen")]
#[test]
fn test_config_reference_renders() {
    let sections = test_sections();
    let template = ConfigReferenceTemplate {
        context_name: "common",
        sections: &sections,
        total_parameters: 10,
        documented_parameters: 3,
        generation_timestamp: "2026-01-29T00:00:00Z",
    };
    let output = template.render().unwrap();
    assert!(output.contains("Configuration Reference: common"));
    assert!(output.contains("3/10 parameters documented"));
}

#[cfg(feature = "docs-gen")]
#[test]
fn test_parameter_explain_renders() {
    let param = test_parameter();
    let template = ParameterExplainTemplate {
        parameter: &param,
        environment: Some("production"),
    };
    let output = template.render().unwrap();
    assert!(output.contains("Configuration Parameter:"));
    assert!(output.contains(&param.default_value));
}

#[cfg(feature = "docs-gen")]
#[test]
fn test_annotated_toml_renders() {
    let sections = test_sections();
    let template = AnnotatedConfigTemplate {
        context_name: "complete",
        environment: "production",
        sections: &sections,
    };
    let output = template.render().unwrap();
    // Must be valid TOML (comments + values)
    assert!(output.contains("[common.database.pool]"));
}
```

### Integration Tests

```bash
# After implementation, verify CLI commands work:
cargo run --package tasker-client --bin tasker-cli -- docs reference --context common
cargo run --package tasker-client --bin tasker-cli -- docs annotated --context complete --environment production
cargo run --package tasker-client --bin tasker-cli -- docs section --path common.database.pool
cargo run --package tasker-client --bin tasker-cli -- docs coverage
cargo run --package tasker-client --bin tasker-cli -- config explain --parameter common.database.pool.max_connections
```

### Compile-Time Verification

The key value proposition: if `TaskerConfig` struct hierarchy changes (fields added, renamed, removed), and the `DocContextBuilder` reflects those changes, any template that references a removed field will fail at `cargo build` — not at runtime when a user runs the CLI.

---

## Risks and Mitigations

| Risk | Impact | Mitigation |
|------|--------|------------|
| Askama compile-time overhead | Slower `tasker-client` builds | Feature-gate behind `docs-gen`; only ~6 templates |
| `_docs` authoring is large effort | 303 parameters to document | Tiered priority; system works without docs (structural fallback) |
| Template syntax learning curve | Contributors unfamiliar with Jinja2 | Templates are simple (loops, conditionals, blocks); Askama docs are good |
| `DocContextBuilder` maintenance | Must track `TaskerConfig` changes | Builder uses serialized defaults — new fields auto-discovered |
| Template file organization | Could grow unwieldy | Start minimal (6 templates); add only when needed |

---

## Out of Scope

- **Replacing `ConfigMerger` or `ConfigLoader`** — config generation stays as-is
- **Runtime/user-supplied templates** — documentation is authoritative, not customizable
- **HTML output** — markdown is sufficient; HTML can be generated from markdown if needed
- **Automatic CI doc generation** — can be added later once templates stabilize
- **API endpoint for docs** — the web server could serve rendered docs, but that's a separate ticket
