# TypeScript Worker Crate and Build Plans

**Date**: 2025-12-20  
**Context**: TAS-100 TypeScript Worker Implementation  
**Related**: TAS-101 (FFI Bridge and Core Infrastructure)

---

## Executive Summary

The TypeScript worker **does not use Cargo** - it is a pure TypeScript/JavaScript project that uses FFI to call the compiled Rust `libtasker_worker` shared library. Unlike Ruby (Magnus FFI extension) and Python (PyO3 extension), TypeScript workers are **standalone executables** that load the Rust library at runtime.

**Key Insight**: TypeScript is a **consumer** of Rust, not a Cargo crate itself.

---

## Architecture Overview

```
┌─────────────────────────────────────────────────────────────┐
│                   tasker-core Workspace                      │
│                      (Rust + Cargo)                          │
├─────────────────────────────────────────────────────────────┤
│                                                               │
│  tasker-worker (cdylib) ──► libtasker_worker.{so,dylib,dll}  │
│                                                               │
└───────────────────────┬─────────────────────────────────────┘
                        │
                        │ FFI (dlopen at runtime)
                        │
        ┌───────────────┼───────────────┬───────────────┐
        │               │               │               │
        ▼               ▼               ▼               ▼
   Ruby Worker     Python Worker   Bun Worker     Node Worker
   (Magnus FFI)    (PyO3 FFI)     (bun:ffi)     (ffi-napi)
   [Cargo crate]   [Cargo crate]  [npm/bun pkg] [npm package]
        │               │               │               │
        │               │               └───────┬───────┘
        │               │                       │
        │               │                  Deno Worker
        │               │                 (Deno.dlopen)
        │               │                 [deno.land pkg]
        │               │                       │
        └───────────────┴───────────────────────┘
                        │
                        ▼
              libtasker_worker shared library
```

### Comparison: Ruby/Python vs TypeScript

| Aspect | Ruby/Python | TypeScript |
|--------|-------------|------------|
| **Build System** | Cargo (cdylib) | npm/bun/deno |
| **Compilation** | Rust → native .bundle/.so | TypeScript → JavaScript |
| **FFI Integration** | Compile-time (Magnus/PyO3) | Runtime (dlopen) |
| **Distribution** | gem/wheel with compiled extension | npm package + separate Rust lib |
| **Deployment** | Single artifact (includes Rust) | Two artifacts (JS + Rust lib) |
| **Workspace Member** | Yes (in Cargo.toml) | No (parallel directory) |

---

## Directory Structure

```
tasker-core/
├── Cargo.toml                         # Rust workspace (does NOT include TS)
├── workers/
│   ├── ruby/
│   │   ├── ext/tasker_core/
│   │   │   └── Cargo.toml             # Magnus cdylib crate
│   │   ├── lib/                       # Ruby code
│   │   └── Rakefile                   # Ruby build (calls cargo)
│   │
│   ├── python/
│   │   ├── Cargo.toml                 # PyO3 cdylib crate
│   │   ├── python/tasker_core/        # Python code
│   │   └── pyproject.toml             # Maturin build (calls cargo)
│   │
│   ├── rust/
│   │   ├── Cargo.toml                 # Pure Rust worker
│   │   └── src/
│   │
│   └── typescript/                    # NEW: TypeScript worker
│       ├── package.json               # npm/bun package (NOT Cargo)
│       ├── deno.json                  # Deno configuration
│       ├── tsconfig.json              # TypeScript config
│       ├── tsup.config.ts             # Build config
│       ├── biome.json                 # Linter/formatter
│       ├── .npmignore
│       ├── .gitignore
│       ├── bin/
│       │   └── tasker-worker.ts       # Executable
│       ├── src/
│       │   ├── index.ts               # Main entry
│       │   ├── ffi/                   # FFI adapters
│       │   ├── types/                 # Type definitions
│       │   ├── handlers/              # Handler classes
│       │   ├── logging/               # Logging
│       │   ├── bootstrap/             # Bootstrap
│       │   ├── subscriber/            # Execution subscriber
│       │   └── server/                # Server components
│       ├── tests/
│       │   ├── unit/
│       │   ├── integration/
│       │   └── fixtures/
│       └── examples/
│           └── handlers/
│
├── scripts/
│   ├── build_all_workers.sh           # NEW: Build all worker types
│   ├── build_typescript_worker.sh     # NEW: Build TypeScript worker
│   ├── clean_project.sh               # UPDATE: Add TS cleanup
│   └── code_check.sh                  # UPDATE: Add TS linting
│
└── tests/
    └── e2e/
        ├── ruby/
        ├── python/
        ├── rust/
        └── typescript/                # NEW: E2E tests

```

---

## Cargo.toml Changes

### Root Cargo.toml

**DO NOT** add `workers/typescript` as a workspace member - it's not a Cargo crate!

```toml
[workspace]
members = [
  ".",
  "tasker-pgmq",
  "tasker-client",
  "tasker-orchestration",
  "tasker-shared",
  "tasker-worker",
  "workers/ruby/ext/tasker_core",
  "workers/rust",
  "workers/python",
  # NOTE: workers/typescript is NOT a Cargo crate
  # It's a TypeScript project that loads libtasker_worker via FFI
]
```

### No Cargo.toml in workers/typescript

The TypeScript worker **does not have a Cargo.toml** because it's not a Rust project. It only needs:
- `package.json` (npm/bun/pnpm)
- `deno.json` (Deno)
- `tsconfig.json` (TypeScript compiler)

---

## Package Management

### package.json

```json
{
  "name": "@tasker-systems/worker",
  "version": "0.1.0",
  "description": "TypeScript worker for tasker-core (Bun/Node/Deno)",
  "type": "module",
  "main": "./dist/index.js",
  "types": "./dist/index.d.ts",
  "exports": {
    ".": {
      "import": "./dist/index.js",
      "require": "./dist/index.cjs",
      "types": "./dist/index.d.ts"
    }
  },
  "bin": {
    "tasker-worker": "./bin/tasker-worker.ts"
  },
  "scripts": {
    "build": "tsup src/index.ts --format esm,cjs --dts --clean",
    "dev": "tsup src/index.ts --format esm --dts --watch",
    "test": "vitest",
    "test:bun": "bun test",
    "test:deno": "deno test --allow-ffi tests/",
    "test:node": "NODE_ENV=test vitest run",
    "lint": "biome check src/ tests/",
    "lint:fix": "biome check --write src/ tests/",
    "format": "biome format --write src/ tests/",
    "typecheck": "tsc --noEmit",
    "clean": "rm -rf dist/ node_modules/ .turbo/"
  },
  "dependencies": {
    "eventemitter3": "^5.0.1"
  },
  "devDependencies": {
    "@biomejs/biome": "^1.9.4",
    "@types/node": "^22.0.0",
    "bun-types": "^1.1.38",
    "tsup": "^8.3.5",
    "typescript": "^5.7.2",
    "vitest": "^2.1.8"
  },
  "peerDependencies": {
    "ffi-napi": "^4.0.3",
    "ref-napi": "^3.0.3"
  },
  "peerDependenciesMeta": {
    "ffi-napi": {
      "optional": true
    },
    "ref-napi": {
      "optional": true
    }
  },
  "engines": {
    "node": ">=18.0.0",
    "bun": ">=1.0.0"
  }
}
```

### deno.json

```json
{
  "compilerOptions": {
    "lib": ["deno.window", "deno.unstable"],
    "strict": true
  },
  "imports": {
    "@tasker-systems/worker": "./src/index.ts"
  },
  "tasks": {
    "test": "deno test --allow-ffi tests/",
    "check": "deno check src/**/*.ts",
    "fmt": "deno fmt src/ tests/",
    "lint": "deno lint src/ tests/"
  },
  "fmt": {
    "useTabs": false,
    "lineWidth": 100,
    "indentWidth": 2,
    "semiColons": true,
    "singleQuote": true
  },
  "lint": {
    "rules": {
      "tags": ["recommended"]
    }
  }
}
```

---

## Build Scripts

### scripts/build_typescript_worker.sh (NEW)

```bash
#!/usr/bin/env bash
#
# Build TypeScript worker for all runtimes
#

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
TS_WORKER_DIR="$PROJECT_ROOT/workers/typescript"

echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "  Building TypeScript Worker"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo

cd "$TS_WORKER_DIR"

# Check if Bun is available
if command -v bun &> /dev/null; then
    echo "→ Using Bun for build"
    bun install
    bun run build
    bun run typecheck
    echo "✓ TypeScript worker built with Bun"
else
    # Fall back to npm
    echo "→ Using npm for build"
    npm install
    npm run build
    npm run typecheck
    echo "✓ TypeScript worker built with npm"
fi

echo
echo "Build artifacts:"
ls -lh dist/

echo
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "  TypeScript Worker Build Complete"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
```

### scripts/build_all_workers.sh (NEW)

```bash
#!/usr/bin/env bash
#
# Build all worker types in dependency order
#

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "  Building All Workers"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo

# Step 1: Build tasker-worker Rust library (foundation)
echo
echo "━━━ Step 1: Building tasker-worker (Rust library) ━━━"
cd "$PROJECT_ROOT"
cargo build --all-features --package tasker-worker
echo "✓ tasker-worker built"

# Step 2: Build Ruby worker
echo
echo "━━━ Step 2: Building Ruby worker ━━━"
cd "$PROJECT_ROOT/workers/ruby"
if [ -f Rakefile ]; then
    bundle install
    bundle exec rake compile
    echo "✓ Ruby worker built"
else
    echo "⚠ Ruby worker Rakefile not found, skipping"
fi

# Step 3: Build Python worker
echo
echo "━━━ Step 3: Building Python worker ━━━"
cd "$PROJECT_ROOT/workers/python"
if [ -f pyproject.toml ]; then
    uv pip install -e .
    echo "✓ Python worker built"
else
    echo "⚠ Python worker pyproject.toml not found, skipping"
fi

# Step 4: Build Rust worker
echo
echo "━━━ Step 4: Building Rust worker ━━━"
cd "$PROJECT_ROOT"
cargo build --all-features --package tasker-worker-rust
echo "✓ Rust worker built"

# Step 5: Build TypeScript worker
echo
echo "━━━ Step 5: Building TypeScript worker ━━━"
if [ -f "$SCRIPT_DIR/build_typescript_worker.sh" ]; then
    "$SCRIPT_DIR/build_typescript_worker.sh"
else
    echo "⚠ TypeScript worker build script not found, skipping"
fi

echo
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "  All Workers Built Successfully"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo
echo "Artifacts:"
echo "  Rust:       target/debug/libtasker_worker.{so,dylib,dll}"
echo "  Ruby:       workers/ruby/lib/tasker_core/tasker_core.bundle"
echo "  Python:     workers/python/python/tasker_core/_tasker_core.*.so"
echo "  TypeScript: workers/typescript/dist/index.js"
```

### scripts/clean_project.sh (UPDATE)

Add TypeScript cleanup to the existing script:

```bash
# Function to clean TypeScript artifacts
clean_typescript() {
    echo -e "${BLUE}━━━ Cleaning TypeScript Artifacts ━━━${NC}"

    local ts_worker_dir="$PROJECT_ROOT/workers/typescript"

    if [ ! -d "$ts_worker_dir" ]; then
        echo -e "${YELLOW}TypeScript worker directory not found, skipping${NC}"
        echo
        return
    fi

    cd "$ts_worker_dir"

    # Clean build artifacts
    if [ -d "dist" ]; then
        echo -e "${YELLOW}Removing TypeScript dist directory...${NC}"
        rm -rf dist
        echo -e "${GREEN}✓ Removed dist directory${NC}"
    fi

    # Clean node_modules (optional, usually leave it)
    if [ "$AGGRESSIVE" = true ] && [ -d "node_modules" ]; then
        echo -e "${YELLOW}Removing node_modules...${NC}"
        rm -rf node_modules
        echo -e "${GREEN}✓ Removed node_modules${NC}"
    fi

    # Clean test artifacts
    if [ -d "coverage" ]; then
        echo -e "${YELLOW}Removing coverage directory...${NC}"
        rm -rf coverage
        echo -e "${GREEN}✓ Removed coverage directory${NC}"
    fi

    # Clean Turbo cache
    if [ -d ".turbo" ]; then
        echo -e "${YELLOW}Removing .turbo cache...${NC}"
        rm -rf .turbo
        echo -e "${GREEN}✓ Removed .turbo cache${NC}"
    fi

    echo
}
```

Update the main cleanup function to call `clean_typescript`:

```bash
main() {
    # ... existing code ...
    
    clean_cargo 30
    clean_test_artifacts
    clean_ruby
    clean_typescript  # NEW
    clean_docker
    show_summary
}
```

### scripts/code_check.sh (UPDATE)

Add TypeScript linting to the existing script:

```bash
# Add to CONFIGURATION section
CHECK_TYPESCRIPT=false

# Add to parse_args function
--typescript|-ts)
    CHECK_TYPESCRIPT=true
    EXPLICIT_LANGUAGE=true
    shift
    ;;

# If no explicit language selected, check all
if [ "$EXPLICIT_LANGUAGE" = false ]; then
    CHECK_RUST=true
    CHECK_PYTHON=true
    CHECK_RUBY=true
    CHECK_TYPESCRIPT=true  # NEW
fi

# Add TypeScript check function
check_typescript() {
    print_header "TypeScript Code Quality Checks"
    
    local ts_worker_dir="$PROJECT_ROOT/workers/typescript"
    
    if [ ! -d "$ts_worker_dir" ]; then
        print_skip "TypeScript worker not found"
        return 0
    fi
    
    cd "$ts_worker_dir"
    
    # Check if dependencies installed
    if [ ! -d "node_modules" ]; then
        print_step "Installing TypeScript dependencies..."
        if command -v bun &> /dev/null; then
            bun install
        else
            npm install
        fi
    fi
    
    local ts_failed=false
    
    # Type checking
    print_section "TypeScript Type Checking"
    if npm run typecheck; then
        print_success "Type checking passed"
    else
        print_error "Type checking failed"
        ts_failed=true
    fi
    
    # Linting with Biome
    print_section "Biome Linting"
    if [ "$AUTO_FIX" = true ]; then
        if npm run lint:fix; then
            print_success "Linting fixed and passed"
        else
            print_error "Linting failed"
            ts_failed=true
        fi
    else
        if npm run lint; then
            print_success "Linting passed"
        else
            print_error "Linting failed"
            ts_failed=true
        fi
    fi
    
    # Formatting
    print_section "Code Formatting"
    if [ "$AUTO_FIX" = true ]; then
        npm run format
        print_success "Code formatted"
    else
        if npm run format -- --check; then
            print_success "Formatting is correct"
        else
            print_error "Formatting issues found (run with --fix)"
            ts_failed=true
        fi
    fi
    
    # Build check
    print_section "Build Check"
    if npm run build; then
        print_success "Build succeeded"
    else
        print_error "Build failed"
        ts_failed=true
    fi
    
    # Tests (if requested)
    if [ "$RUN_TESTS" = true ]; then
        print_section "Running Tests"
        
        # Test with Bun if available
        if command -v bun &> /dev/null; then
            print_step "Running tests with Bun..."
            if bun test; then
                print_success "Bun tests passed"
            else
                print_error "Bun tests failed"
                ts_failed=true
            fi
        fi
        
        # Test with Node.js
        print_step "Running tests with Node.js..."
        if npm run test:node; then
            print_success "Node.js tests passed"
        else
            print_error "Node.js tests failed"
            ts_failed=true
        fi
        
        # Test with Deno if available
        if command -v deno &> /dev/null; then
            print_step "Running tests with Deno..."
            if npm run test:deno; then
                print_success "Deno tests passed"
            else
                print_error "Deno tests failed"
                ts_failed=true
            fi
        fi
    fi
    
    if [ "$ts_failed" = true ]; then
        print_error "TypeScript checks failed"
        return 1
    else
        print_success "All TypeScript checks passed"
        return 0
    fi
}

# Update main execution to call check_typescript
main() {
    # ... existing checks ...
    
    if [ "$CHECK_TYPESCRIPT" = true ]; then
        check_typescript || overall_failed=true
    fi
    
    # ... rest of main ...
}
```

---

## Testing Strategy

### Unit Tests

Run with multiple runtimes:

```bash
# Bun
cd workers/typescript
bun test

# Node.js
npm run test:node

# Deno
deno test --allow-ffi tests/
```

### Integration Tests

Located in main repo under `tests/e2e/typescript/`:

```bash
cd tests/e2e/typescript
DATABASE_URL=postgresql://... cargo test --test typescript_worker_integration
```

### CI/CD

Add to `.github/workflows/test.yml`:

```yaml
typescript-worker-tests:
  runs-on: ubuntu-latest
  strategy:
    matrix:
      runtime: [bun, node, deno]
  steps:
    - uses: actions/checkout@v3
    
    # Setup Rust (needed for libtasker_worker)
    - uses: actions-rs/toolchain@v1
      with:
        toolchain: stable
    
    # Build tasker-worker
    - name: Build tasker-worker
      run: cargo build --package tasker-worker
    
    # Setup runtime
    - name: Setup Bun
      if: matrix.runtime == 'bun'
      uses: oven-sh/setup-bun@v1
    
    - name: Setup Node.js
      if: matrix.runtime == 'node'
      uses: actions/setup-node@v3
      with:
        node-version: 18
    
    - name: Setup Deno
      if: matrix.runtime == 'deno'
      uses: denoland/setup-deno@v1
    
    # Run tests
    - name: Install dependencies
      if: matrix.runtime != 'deno'
      working-directory: workers/typescript
      run: |
        if [ "${{ matrix.runtime }}" = "bun" ]; then
          bun install
        else
          npm install
        fi
    
    - name: Run tests
      working-directory: workers/typescript
      run: |
        case "${{ matrix.runtime }}" in
          bun)
            bun test
            ;;
          node)
            npm run test:node
            ;;
          deno)
            deno test --allow-ffi tests/
            ;;
        esac
```

---

## Deployment Patterns

### 1. Standalone Executable

```bash
# Bun (single executable)
bun build bin/tasker-worker.ts --compile --outfile tasker-worker-bun
./tasker-worker-bun --namespace payments

# Deno (single executable)
deno compile --allow-ffi --allow-net --allow-read --allow-env \
  --output tasker-worker-deno \
  bin/tasker-worker.ts
./tasker-worker-deno --namespace payments
```

### 2. Docker Container

```dockerfile
# Multi-stage build
FROM rust:1.83 AS rust-builder
WORKDIR /build
COPY . .
RUN cargo build --release --package tasker-worker

FROM oven/bun:1 AS ts-builder
WORKDIR /build
COPY workers/typescript/ ./
RUN bun install && bun run build

FROM oven/bun:1-slim
COPY --from=rust-builder /build/target/release/libtasker_worker.so /usr/local/lib/
COPY --from=ts-builder /build/dist/ /app/
COPY --from=ts-builder /build/bin/ /app/bin/
WORKDIR /app
ENV TASKER_LIBRARY_PATH=/usr/local/lib/libtasker_worker.so
ENTRYPOINT ["bun", "bin/tasker-worker.ts"]
```

### 3. NPM Package

Distribute as npm package (requires users to build Rust separately):

```bash
npm publish @tasker-systems/worker
```

Users install:
```bash
npm install @tasker-systems/worker

# Must also have libtasker_worker available:
# - From system install
# - From environment variable TASKER_LIBRARY_PATH
# - From relative path (target/release/libtasker_worker.*)
```

---

## Key Takeaways

1. **TypeScript is NOT a Cargo crate** - it's a consumer of the Rust library
2. **No Cargo.toml in workers/typescript** - only package.json/deno.json
3. **Build independently** - TypeScript builds with npm/bun/deno, not cargo
4. **Shared library required** - TypeScript worker needs pre-built libtasker_worker
5. **Multi-runtime support** - Bun, Node.js, Deno all supported with same codebase
6. **Script updates needed** - Add TypeScript to clean_project.sh and code_check.sh
7. **CI/CD matrix** - Test all three runtimes in parallel
8. **Deployment flexibility** - Standalone executable, Docker, or npm package

---

## Estimated Implementation Time

- Create `workers/typescript/` directory structure: 30 minutes
- Add `build_typescript_worker.sh`: 1 hour
- Add `build_all_workers.sh`: 1 hour
- Update `clean_project.sh`: 30 minutes
- Update `code_check.sh`: 1 hour
- Setup CI/CD workflows: 1 hour
- Documentation: 1 hour

**Total**: ~6 hours of build infrastructure work (separate from TAS-101 implementation)
