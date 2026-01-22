# TAS-71: Profiling Tool Evaluation

**Date:** 2026-01-20
**Status:** Research Complete
**Branch:** `jcoletaylor/tas-71-profiling-benchmarks-and-optimizations`
**Research Area:** A - Profiling Tooling Evaluation

---

## Executive Summary

This document evaluates profiling tools for our Rust async (tokio) application with PostgreSQL backend. Based on our analysis, we recommend a **layered profiling stack**:

1. **CPU Profiling:** `samply` (primary for macOS development) + `cargo-flamegraph` (CI/Linux)
2. **Async Runtime Introspection:** `tokio-console` (for identifying async bottlenecks)
3. **Instrumentation-Based Profiling:** `tracing-flame` (leverages existing tracing infrastructure)
4. **Memory Profiling:** `dhat-rs` (cross-platform, Rust-native)

---

## Tool Comparison Table

| Tool | Purpose | Platform | Integration Effort | Output Format | Async Support | Notes |
|------|---------|----------|-------------------|---------------|---------------|-------|
| **cargo-flamegraph** | CPU sampling profiler | Linux (perf), macOS (xctrace), Windows | Low (cargo subcommand) | Interactive SVG | Stack frames only | Production-ready, well-established |
| **samply** | CPU sampling profiler | macOS (primary), Linux | Low (standalone binary) | Firefox Profiler UI | Stack frames only | Best macOS experience, interactive UI |
| **tokio-console** | Async runtime introspection | Cross-platform | Medium (code changes) | Real-time TUI | Excellent (purpose-built) | Shows task scheduling, waker behavior |
| **tracing-flame** | Instrumentation profiler | Cross-platform | Low (layer addition) | Folded stacks, SVG | Excellent (span-based) | Uses existing tracing infrastructure |
| **dhat-rs** | Heap profiler | Cross-platform | Medium (code changes) | JSON + viewer | N/A | Pure Rust, assertion support |
| **heaptrack** | Heap profiler | Linux (data collection) | Low (external) | GUI analysis | N/A | Rust symbol demangling supported |

---

## Tool Deep Dives

### 1. cargo-flamegraph

**Repository:** https://github.com/flamegraph-rs/flamegraph

#### Installation

```bash
# Install the cargo subcommand
cargo install flamegraph

# macOS: No additional dependencies (uses xctrace)
# xctrace is included with Xcode Command Line Tools

# Linux: Install perf
# Ubuntu/Debian
sudo apt install linux-tools-common linux-tools-generic linux-tools-$(uname -r)

# Allow perf without sudo (optional, recommended for development)
echo 'kernel.perf_event_paranoid=-1' | sudo tee -a /etc/sysctl.conf
sudo sysctl -p
```

#### Release Profile Requirements

Debug symbols are **required** for meaningful flamegraphs. Add to `Cargo.toml`:

```toml
# Profiling profile - release optimizations with debug symbols
[profile.profiling]
inherits = "release"
debug = true

# Or modify existing release profile (not recommended for production)
# [profile.release]
# debug = true
```

**Current Cargo.toml Issue:** Our current release profile has `strip = true` which removes symbols:

```toml
[profile.release]
opt-level = 3
strip = true       # <-- This removes debug symbols!
lto = "fat"
panic = "abort"
codegen-units = 1
```

**Recommendation:** Create a dedicated `profiling` profile that inherits from release but keeps debug symbols.

#### Usage with Long-Running Services

```bash
# Profile a running process by PID
flamegraph --pid $(pgrep -f tasker-orchestration)

# Profile for specific duration
flamegraph --pid 1337 --timeout 60

# Profile a service launched via cargo
cargo flamegraph --profile profiling --bin tasker-orchestration -- --config test

# Profile with custom output
cargo flamegraph --profile profiling -o profile-$(date +%Y%m%d-%H%M%S).svg
```

#### macOS-Specific Considerations

1. **xctrace Requirement:** Uses Apple's Instruments framework via `xctrace`
2. **System Integrity Protection (SIP):** Some system processes cannot be profiled
3. **Symbol Resolution:** Works well with debug symbols; without them, you'll see memory addresses
4. **Performance:** Lower sampling resolution than Linux perf, but adequate for application profiling

#### Output Format

- **Default:** Interactive SVG (`flamegraph.svg`)
- **View:** Must open in web browser (not image viewers) for interactivity
- **Features:** Zoom, search, click-to-focus on functions

#### Pros/Cons

| Pros | Cons |
|------|------|
| Simple cargo integration | macOS sampling less precise than Linux perf |
| Well-established in Rust ecosystem | Static SVG output (no timeline view) |
| Works with release builds | Requires debug symbols for useful output |
| Can attach to running processes | No async-aware stack traces |

---

### 2. samply

**Repository:** https://github.com/mstange/samply

#### Installation on macOS

```bash
# Option 1: Shell installer (recommended)
curl --proto '=https' --tlsv1.2 -LsSf \
  https://github.com/mstange/samply/releases/download/samply-v0.13.1/samply-installer.sh | sh

# Option 2: Cargo
cargo install --locked samply

# Required: Self-sign the binary for process attachment
samply setup
# Note: Run this after each samply update
```

#### Comparison to cargo-flamegraph

| Aspect | samply | cargo-flamegraph |
|--------|--------|------------------|
| **macOS Experience** | Native, excellent | Adequate |
| **Visualization** | Firefox Profiler (interactive web UI) | Static SVG |
| **Timeline View** | Yes | No |
| **Call Tree View** | Yes | Flamegraph only |
| **Multiple Profiles** | Compare in UI | Manual comparison |
| **Linux Support** | Good (perf-based) | Good (perf-based) |
| **Process Attachment** | Requires setup | Works immediately |
| **Cargo Integration** | Manual invocation | Cargo subcommand |

#### Async/Tokio Support

**Limitation:** Like all sampling profilers, samply captures **stack frames at sample points**, not async task boundaries. You'll see:

- Where time is spent (good for CPU hotspots)
- Which functions are called most frequently
- Call chains leading to hotspots

You **won't** see:

- Task scheduling delays
- Which tasks are blocked waiting
- Waker patterns or spurious wakeups
- Async-specific bottlenecks

**For async debugging, use tokio-console in addition to samply.**

#### Usage

```bash
# Profile a command
samply record ./target/profiling/tasker-orchestration --config test

# Profile via cargo
samply record cargo run --profile profiling --bin tasker-orchestration

# Profile with time limit
samply record --duration 60 ./target/profiling/tasker-orchestration

# Attach to running process (requires setup)
samply record --pid $(pgrep -f tasker-orchestration)

# Save profile for later viewing
samply record --save-only -o profile.json ./target/profiling/tasker-orchestration
# Later: samply load profile.json
```

#### Build Configuration

Create `~/.cargo/config.toml` or project-local `.cargo/config.toml`:

```toml
[profile.profiling]
inherits = "release"
debug = true
```

#### Pros/Cons

| Pros | Cons |
|------|------|
| Best macOS profiling experience | Requires self-signing setup |
| Interactive Firefox Profiler UI | No cargo subcommand integration |
| Timeline view for temporal analysis | System-signed binaries cannot be profiled |
| Can save/load profiles | Learning curve for UI features |
| Compare multiple profiles | |

---

### 3. tokio-console

**Repository:** https://github.com/tokio-rs/console

#### What It Provides

tokio-console is a **debugger for async Rust**, providing real-time visibility into:

1. **Task Monitoring:** All spawned tasks with execution statistics ("top for tasks")
2. **Task Metrics:** Poll times, wakeup counts, time to first poll
3. **Waker Behavior:** Self-waking detection, lost wakers
4. **Resource Usage:** Future sizes, stack allocation
5. **Scheduling Patterns:** Tasks that never yield, excessive polling

#### Built-in Lint Warnings

| Warning | Threshold | Meaning |
|---------|-----------|---------|
| Self-wake ratio high | >50% | Task may be spinning |
| Lost waker | - | Waker dropped without waking |
| Never yielded | - | Task never yielded to runtime |
| Large future | - | Future may need boxing |

#### Setup Requirements

**1. Install the CLI tool:**

```bash
cargo install --locked tokio-console
```

**2. Add dependency to Cargo.toml:**

```toml
[dependencies]
console-subscriber = "0.4"

# Feature-gate for dev/profiling builds only (recommended)
[features]
tokio-console = ["console-subscriber"]
```

**3. Enable tokio_unstable cfg flag:**

Option A: Environment variable (for testing)
```bash
RUSTFLAGS="--cfg tokio_unstable" cargo build
```

Option B: `.cargo/config.toml` (persistent)
```toml
[build]
rustflags = ["--cfg", "tokio_unstable"]
```

**4. Add instrumentation to your application:**

```rust
// In main.rs or lib.rs initialization

#[cfg(feature = "tokio-console")]
fn init_console() {
    console_subscriber::init();
}

#[cfg(not(feature = "tokio-console"))]
fn init_console() {
    // No-op when feature disabled
}

fn main() {
    // Must be called before init_tracing() if both are used
    init_console();

    // ... rest of initialization
}
```

**Alternative: Layer composition with existing tracing:**

```rust
use tracing_subscriber::prelude::*;

fn init_with_console() {
    let console_layer = console_subscriber::spawn();

    tracing_subscriber::registry()
        .with(console_layer)
        .with(your_existing_layers())
        .init();
}
```

#### Integration with Existing Tracing Infrastructure

Our current `tasker-shared/src/logging.rs` uses:
- `tracing_subscriber::registry()` with layers
- OpenTelemetry integration
- Environment-based configuration

**Integration approach:**

```rust
// In tasker-shared/src/logging.rs

#[cfg(feature = "tokio-console")]
fn maybe_add_console_layer<S>(subscriber: S) -> impl tracing::Subscriber
where
    S: tracing::Subscriber + for<'span> tracing_subscriber::registry::LookupSpan<'span>,
{
    subscriber.with(console_subscriber::spawn())
}

#[cfg(not(feature = "tokio-console"))]
fn maybe_add_console_layer<S>(subscriber: S) -> S
where
    S: tracing::Subscriber,
{
    subscriber
}
```

#### Usage

```bash
# Terminal 1: Start application with console support
RUSTFLAGS="--cfg tokio_unstable" cargo run --features tokio-console

# Terminal 2: Connect console
tokio-console

# Connect to remote service (gRPC-based)
tokio-console http://remote-host:6669
```

#### Value for Identifying Async Bottlenecks

| Bottleneck Type | How Console Reveals It |
|-----------------|----------------------|
| Task starvation | Long "time to first poll" |
| Spinning tasks | High self-wake ratio |
| Blocking operations | Long poll durations |
| Missing yields | "Never yielded" warning |
| Dropped wakers | "Lost waker" warning |
| Large futures | Stack allocation warnings |
| Contention | Many tasks waiting on same resource |

#### Pros/Cons

| Pros | Cons |
|------|------|
| Purpose-built for async debugging | Requires tokio_unstable flag |
| Real-time monitoring | Some runtime overhead |
| Detects async-specific issues | Requires code changes |
| Remote monitoring support | Learning curve for interpretation |
| Built-in lint warnings | |

---

### 4. tracing-flame

**Repository:** https://github.com/tokio-rs/tracing/tree/master/tracing-flame

#### How It Differs from Sampling-Based Profilers

| Aspect | Sampling Profilers (samply, flamegraph) | tracing-flame |
|--------|----------------------------------------|---------------|
| **Mechanism** | Periodically sample stack | Record every span entry/exit |
| **Precision** | Statistical (misses short ops) | Exact (captures all instrumented code) |
| **Overhead** | Low (~1-5%) | Higher (depends on span frequency) |
| **Async Awareness** | Stack frames only | Span-based (async-native) |
| **Setup** | External tool | Code instrumentation |
| **Coverage** | All code | Only instrumented code |

#### Integration with Existing Tracing Infrastructure

Since our codebase already uses `tracing` extensively with `#[instrument]` attributes, integration is straightforward:

```rust
use tracing_flame::FlameLayer;
use tracing_subscriber::prelude::*;
use std::fs::File;

fn init_with_flame() {
    let (flame_layer, guard) = FlameLayer::with_file("./tracing.folded").unwrap();

    // Compose with existing layers
    tracing_subscriber::registry()
        .with(flame_layer)
        .with(console_layer())  // existing
        .with(otel_layer())     // existing
        .init();

    // IMPORTANT: Keep guard alive for duration of profiling
    // Drop guard to flush and close the file
    std::mem::forget(guard);  // Or store in static/global
}
```

**Feature-gated integration:**

```toml
# Cargo.toml
[dependencies]
tracing-flame = { version = "0.2", optional = true }

[features]
flame-profiling = ["tracing-flame"]
```

#### Output Formats

**Primary:** Folded stack traces (text format)
```
main;runtime::block_on;orchestration::process_task;db::query 1234
main;runtime::block_on;orchestration::process_task;step::execute 5678
```

**Visualization:**

```bash
# Generate SVG with inferno-flamegraph
cargo install inferno
cat tracing.folded | inferno-flamegraph > tracing.svg

# View in speedscope (recommended - supports folded format)
# Upload tracing.folded to https://www.speedscope.app/
```

**Speedscope Compatibility:** Yes, speedscope directly supports folded stack trace format.

#### Flamegraph vs Flamechart

| View | Description | Best For |
|------|-------------|----------|
| **Flamegraph** | Collapse identical frames, sort by name | Aggregate time analysis, finding hotspots |
| **Flamechart** | Preserve chronological order | Understanding execution sequence, debugging |

```bash
# Flamegraph (default)
cat tracing.folded | inferno-flamegraph > flamegraph.svg

# Flamechart (chronological)
cat tracing.folded | inferno-flamechart > flamechart.svg
```

#### Pros/Cons

| Pros | Cons |
|------|------|
| Exact timing (no sampling gaps) | Overhead from instrumentation |
| Async-aware (span-based) | Only captures instrumented code |
| Leverages existing tracing | Experimental maintenance status |
| Works with speedscope | Requires span annotations |
| Cross-platform | Can generate large output files |

---

### 5. Memory Profilers

#### dhat-rs

**Repository:** https://github.com/nnethercote/dhat-rs

**Platform:** Cross-platform (100% Rust)

**Features:**
- Heap allocation profiling
- Test assertions on heap behavior
- Peak memory tracking
- Allocation count verification

**Installation:**

```toml
# Cargo.toml
[dev-dependencies]
dhat = "0.3"

[features]
dhat-heap = []

# For profiling (not tests)
[[bin]]
name = "profiling-bin"
path = "src/main.rs"
required-features = ["dhat-heap"]
```

**Usage:**

```rust
#[cfg(feature = "dhat-heap")]
#[global_allocator]
static ALLOC: dhat::Alloc = dhat::Alloc;

fn main() {
    #[cfg(feature = "dhat-heap")]
    let _profiler = dhat::Profiler::new_heap();

    // ... your code ...

    // On drop, writes dhat-heap.json
}
```

**View results:**
```bash
# Open in browser
open https://nnethercote.github.io/dh_view/dh_view.html
# Upload dhat-heap.json
```

**macOS Compatibility:** Full support (pure Rust, no platform dependencies)

#### heaptrack

**Platform:** Linux only for data collection; GUI works on macOS

**Features:**
- Detailed heap allocation tracking
- Memory leak detection
- Flame graph of allocations
- Rust symbol demangling support

**Usage:**
```bash
# Linux: Collect data
heaptrack ./target/profiling/tasker-orchestration

# macOS: Analyze collected data
heaptrack_gui heaptrack.tasker-orchestration.12345.gz
```

**macOS Compatibility:** Data collection requires Linux; GUI analysis works on macOS via Homebrew:
```bash
brew install heaptrack
```

**Rust Integration:** Add debug symbols to release builds:
```toml
[profile.release]
debug = true
```

#### tikv-jemallocator (Heap Profiling via jemalloc)

**Note:** The upstream jemalloc repository was archived in June 2025. tikv-jemallocator continues to be maintained for Rust usage.

**Platform:** Linux, macOS (BSD-based systems)

**Features:**
- Heap profiling via jemalloc's built-in capabilities
- Memory fragmentation analysis
- Allocation size distribution

**Usage:**
```toml
[dependencies]
tikv-jemallocator = { version = "0.6", features = ["profiling"] }
```

```rust
#[global_allocator]
static GLOBAL: tikv_jemallocator::Jemalloc = tikv_jemallocator::Jemalloc;
```

**Profiling:**
```bash
MALLOC_CONF="prof:true,prof_prefix:jeprof" ./target/release/app
jeprof --svg ./target/release/app jeprof.*.heap > heap.svg
```

**macOS Compatibility:** Works but less tested than Linux

#### Memory Profiler Comparison

| Tool | Platform | Integration | Overhead | Best For |
|------|----------|-------------|----------|----------|
| dhat-rs | Cross-platform | Medium | Medium | Test assertions, detailed analysis |
| heaptrack | Linux (collect) | Low | Low | Leak detection, flame graphs |
| jemalloc | Linux/macOS | Medium | Low | Production profiling, fragmentation |

**Recommendation for macOS Development:** Use **dhat-rs** for its cross-platform support and tight Rust integration. Use heaptrack when testing on Linux CI.

---

## Recommended Tooling Stack

Based on our use case (distributed task orchestration, tokio async, PostgreSQL backend, macOS development), we recommend:

### Development Profiling (macOS)

| Need | Tool | Priority |
|------|------|----------|
| CPU hotspots | samply | P1 |
| Async bottlenecks | tokio-console | P1 |
| Memory leaks | dhat-rs | P2 |
| Span-based profiling | tracing-flame | P3 |

### CI/Production Profiling (Linux)

| Need | Tool | Priority |
|------|------|----------|
| CPU hotspots | cargo-flamegraph | P1 |
| Memory analysis | heaptrack | P2 |
| Performance regression | Criterion baselines | P1 |

---

## Installation Commands (Quick Reference)

### macOS Development Setup

```bash
# CPU profiling
cargo install --locked samply
samply setup  # Self-sign for process attachment

# Async debugging
cargo install --locked tokio-console

# Flamegraph generation (alternative to samply)
cargo install flamegraph

# Memory profiling visualization
cargo install inferno  # For tracing-flame output
```

### Linux CI Setup

```bash
# Install perf
sudo apt-get install linux-tools-common linux-tools-generic

# CPU profiling
cargo install flamegraph

# Memory profiling
sudo apt-get install heaptrack

# Allow perf without sudo
echo 'kernel.perf_event_paranoid=-1' | sudo tee -a /etc/sysctl.conf
sudo sysctl -p
```

---

## Cargo.toml Changes Required

### Workspace Profile for Profiling

Add to root `Cargo.toml`:

```toml
# Profiling profile - release speed with debug symbols
[profile.profiling]
inherits = "release"
debug = true
strip = false  # Keep symbols!
# Keep other release settings for representative performance
lto = "fat"
codegen-units = 1
# Don't abort on panic - allows profiler stack unwinding
panic = "unwind"
```

### Feature Flags for Profiling Tools

Add to relevant crate `Cargo.toml` (suggest `tasker-shared`):

```toml
[dependencies]
# Async runtime introspection
console-subscriber = { version = "0.4", optional = true }

# Instrumentation-based profiling
tracing-flame = { version = "0.2", optional = true }

# Memory profiling (dev-dependency may be more appropriate)
[dev-dependencies]
dhat = "0.3"

[features]
# Enable tokio-console support
tokio-console = ["console-subscriber"]

# Enable tracing-flame output
flame-profiling = ["tracing-flame"]

# Enable dhat heap profiling
dhat-heap = []
```

### Cargo Config for tokio-console

Create/update `.cargo/config.toml`:

```toml
# Required for tokio-console
# Only enable when profiling to avoid overhead in normal builds
# Use: RUSTFLAGS="--cfg tokio_unstable" cargo build --features tokio-console
# Or uncomment below for persistent enablement:
# [build]
# rustflags = ["--cfg", "tokio_unstable"]
```

---

## Integration Examples

### Profiling a Long-Running Service

```bash
# Terminal 1: Start service with profiling profile
cargo build --profile profiling --bin tasker-orchestration

# Option A: Profile with samply (macOS, recommended)
samply record ./target/profiling/tasker-orchestration --config test

# Option B: Attach to running service
./target/profiling/tasker-orchestration --config test &
samply record --pid $!

# Option C: Time-limited profiling
samply record --duration 60 ./target/profiling/tasker-orchestration
```

### Using tokio-console

```bash
# Terminal 1: Run with console support
RUSTFLAGS="--cfg tokio_unstable" \
  cargo run --profile profiling --features tokio-console --bin tasker-orchestration

# Terminal 2: Connect console
tokio-console
```

### Generating Tracing Flamegraphs

```bash
# Build with flame profiling
cargo run --profile profiling --features flame-profiling --bin tasker-orchestration

# After execution, generate visualization
cat tracing.folded | inferno-flamegraph > tracing-flamegraph.svg

# Or upload to speedscope
open https://www.speedscope.app/
# Upload tracing.folded
```

---

## Next Steps

1. **Add profiling profile** to root `Cargo.toml` (non-breaking)
2. **Add console-subscriber** as optional dependency to `tasker-shared`
3. **Integrate console layer** into `tasker-shared/src/logging.rs` behind feature flag
4. **Document profiling workflow** in developer guide
5. **Create profiling baseline** following existing `docs/observability/benchmark-audit-and-profiling-plan.md`

---

---

## CI/Production Profiling Decision

**Decision:** Profiling and benchmarking are **LOCAL DEVELOPMENT ONLY** for this project.

**Rationale:**
- This is a labor-of-love open source project with limited CI resources
- GitHub Actions free-tier runners have constrained performance
- Profiling requires release builds with debug symbols, which are slow to compile
- Results would be non-representative on shared CI runners
- Local profiling on consistent hardware produces more reliable data

**What this means:**
- No profiling cargo-make tasks will run in CI
- No benchmark regression detection in CI pipelines
- Developers run profiling locally on their own machines
- Results are manually compared and documented

**Future consideration:** If the project grows, self-hosted runners or dedicated profiling infrastructure could enable CI-based profiling. This is tracked as a potential future enhancement, not a near-term goal.

---

## References

- [cargo-flamegraph](https://github.com/flamegraph-rs/flamegraph)
- [samply](https://github.com/mstange/samply)
- [tokio-console](https://github.com/tokio-rs/console)
- [tracing-flame](https://github.com/tokio-rs/tracing/tree/master/tracing-flame)
- [dhat-rs](https://github.com/nnethercote/dhat-rs)
- [heaptrack](https://github.com/KDE/heaptrack)
- [Existing benchmark guide](../../../observability/benchmarking-guide.md)
- [Benchmark audit plan](../../../observability/benchmark-audit-and-profiling-plan.md)
