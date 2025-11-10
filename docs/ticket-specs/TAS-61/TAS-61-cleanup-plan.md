# TAS-61 Configuration Cleanup Plan

## Objective
Systematically investigate and remove configuration parameters that are defined in TOML but don't actually drive system behavior.

## Status
- ✅ Analysis complete (TAS-61-config-usage-analysis.md)
- ✅ Critical issues fixed (EventSystemProcessingConfig, EventSystemHealthConfig)
- 🔄 Cleanup phase - ready to execute
- ⚠️  **DO NOT COMMIT** - All changes for review only

## Investigation Categories

### Category 1: Event System Configs (PARTIAL FIX COMPLETE)
**Status**: ✅ FIXED - EventSystemProcessingConfig and EventSystemHealthConfig now use hydrated values

**Remaining Investigation**:
- ❓ **EventSystemBackoffConfig** - Nested in processing config, no evidence of field access
  - Check if retry logic reads `initial_delay_ms`, `max_delay_ms`, `multiplier`, `jitter_percent`
  - May be defined for future use but not implemented yet

**Investigation Script**:
```bash
# Check for backoff field access
rg "backoff\.(initial_delay_ms|max_delay_ms|multiplier|jitter_percent)" --type rust
rg "EventSystemBackoffConfig" --type rust -A 5 -B 5
```

### Category 2: Web Middleware Configs
**Configs to Investigate**:
1. **CorsConfig** - CORS middleware settings
2. **AuthConfig** - Authentication configuration
3. **RateLimitingConfig** - Rate limiting middleware
4. **ResilienceConfig** - Web circuit breaker

**Investigation Steps**:
1. Check `tasker-orchestration/src/web/middleware/` for actual usage
2. Look for `.cors`, `.auth`, `.rate_limiting`, `.resilience` field access
3. Verify middleware initialization reads these configs

**Investigation Script**:
```bash
# Check middleware config usage
rg "\.cors\." --type rust tasker-orchestration/src/web/
rg "\.auth\." --type rust tasker-orchestration/src/web/
rg "\.rate_limiting\." --type rust tasker-orchestration/src/web/
rg "\.resilience\." --type rust tasker-orchestration/src/web/
```

### Category 3: Optional Feature Configs
**Configs to Investigate**:
1. **TelemetryConfig** - Optional telemetry/metrics
2. **RabbitmqConfig** - Alternative queue backend (if using PGMQ only)
3. **TlsConfig** - Optional TLS for web server

**Investigation Steps**:
1. Check if features are conditionally compiled
2. Verify if production uses these features
3. Check for field access in relevant modules

**Investigation Script**:
```bash
# Check feature-gated usage
rg "TelemetryConfig|telemetry\." --type rust
rg "RabbitmqConfig|rabbitmq\." --type rust
rg "TlsConfig|tls\." --type rust
```

### Category 4: DLQ/Staleness Configs
**Configs to Investigate**:
1. **StalenessThresholds** - DLQ staleness detection
2. **StalenessActions** - What to do with stale tasks
3. **DlqOperationsConfig** - Dead letter queue operations

**Investigation Steps**:
1. Check `tasker-orchestration/src/orchestration/staleness_detector.rs`
2. Look for threshold comparisons and action execution
3. Verify DLQ operations use config

**Investigation Script**:
```bash
# Check staleness detection
rg "staleness_threshold|StalenessThresholds" --type rust
rg "DlqOperationsConfig|dlq_operations" --type rust
```

## Parallel Investigation Tasks

### Task 1: Event System Backoff Config
**Agent**: general-purpose
**Scope**: EventSystemBackoffConfig
**Steps**:
1. Search for all references to EventSystemBackoffConfig
2. Check if any code reads the 4 backoff fields
3. If unused: Comment out the field in EventSystemProcessingConfig
4. Run: `cargo test --workspace --all-features`
5. If tests pass: Mark for removal

**Expected Outcome**: Determine if backoff config is dead code or future-use

### Task 2: Web Middleware Configs
**Agent**: general-purpose
**Scope**: CorsConfig, AuthConfig, RateLimitingConfig, ResilienceConfig
**Steps**:
1. Trace middleware initialization in `tasker-orchestration/src/web/`
2. For each config, check if fields are read during middleware setup
3. If unused: Comment out in WebConfig struct
4. Run: `cargo test --workspace --all-features`
5. Document which middlewares use config vs. hardcoded values

**Expected Outcome**: List of middleware configs that can be removed

### Task 3: Optional Feature Configs
**Agent**: general-purpose
**Scope**: TelemetryConfig, RabbitmqConfig, TlsConfig
**Steps**:
1. Check if configs are behind feature flags
2. Verify production deployment uses these features
3. If truly optional and unused: Comment out
4. Run: `cargo test --workspace --all-features`
5. Document feature status (enabled/disabled in production)

**Expected Outcome**: Clarify which optional configs are actually used

### Task 4: DLQ/Staleness Configs
**Agent**: general-purpose
**Scope**: StalenessThresholds, StalenessActions, DlqOperationsConfig
**Steps**:
1. Analyze staleness_detector.rs for config usage
2. Check if threshold values are compared
3. Check if actions are executed based on config
4. If unused: Comment out
5. Run: `cargo test --workspace --all-features`

**Expected Outcome**: Verify DLQ configs drive behavior

## Validation Protocol

### For Each Config Investigation:

#### Phase 1: Evidence Gathering
```bash
# Find all references
rg "ConfigName" --type rust -n

# Find field access patterns
rg "config\.field_name" --type rust -n

# Check test references
rg "ConfigName" --type rust -g "**/*test*.rs"
```

#### Phase 2: Comment-Out Test
1. Comment out the struct field or entire struct
2. Add comment: `// TAS-61 Cleanup: Investigating if unused`
3. Run workspace tests: `cargo test --workspace --all-features`
4. Run clippy: `cargo clippy --workspace --all-targets --all-features`
5. Document results

#### Phase 3: TOML Validation
1. If struct field removed, check TOML files
2. Comment out corresponding TOML sections
3. Test config loading: `cargo test --workspace --all-features config`
4. Verify no deserialization errors

#### Phase 4: Removal (If Approved)
1. Remove struct definition from `tasker-shared/src/config/tasker.rs`
2. Remove TOML sections from all environment configs
3. Update documentation
4. Run full test suite
5. **DO NOT COMMIT** - Present for review

## Investigation Scripts

### Script 1: Config Field Access Analyzer
```bash
#!/bin/bash
# File: /tmp/analyze_config_usage.sh

CONFIG_NAME=$1

echo "=== Analyzing Config: $CONFIG_NAME ==="
echo ""

echo "## Struct Definition:"
rg "pub struct $CONFIG_NAME" --type rust -A 20 | head -30
echo ""

echo "## Field Access Patterns:"
rg "$CONFIG_NAME" --type rust | grep -v "pub struct" | grep -v "impl" | head -20
echo ""

echo "## Test References:"
rg "$CONFIG_NAME" --type rust -g "**/*test*.rs" | head -10
echo ""

echo "## TOML References:"
rg "$CONFIG_NAME" --type toml 2>/dev/null | head -10
```

### Script 2: Comment-Out Validator
```bash
#!/bin/bash
# File: /tmp/validate_removal.sh

echo "=== TAS-61 Config Removal Validation ==="
echo ""

echo "1. Running workspace tests..."
cargo test --workspace --all-features --quiet 2>&1 | tail -5

echo ""
echo "2. Running clippy..."
cargo clippy --workspace --all-targets --all-features --quiet 2>&1 | grep -E "error|warning" | head -10

echo ""
echo "3. Running config tests..."
cargo test --workspace --all-features config --quiet 2>&1 | tail -3

echo ""
echo "✅ Validation complete"
```

## Risk Assessment

### Low Risk (Safe to Remove if Unused)
- EventSystemBackoffConfig - Not yet implemented
- RabbitmqConfig - Alternative backend not in use
- TlsConfig - If production doesn't use TLS

### Medium Risk (Verify Carefully)
- CorsConfig, AuthConfig - May be used in production middleware
- TelemetryConfig - May be enabled in production
- ResilienceConfig - Circuit breaker might be active

### High Risk (Keep Unless Certain)
- StalenessThresholds - DLQ detection likely uses these
- RateLimitingConfig - May be protecting production endpoints

## Success Criteria

### For Each Config:
1. ✅ Evidence gathered showing usage or non-usage
2. ✅ Comment-out test passed at workspace level
3. ✅ No compilation errors after removal
4. ✅ No test failures after removal
5. ✅ TOML deserialization still works
6. ✅ Documentation updated

### Overall:
1. Clear categorization: USED vs UNUSED vs FUTURE-USE
2. Removed configs don't break compilation or tests
3. All changes documented and ready for review
4. No commits made (review first)

## Next Steps

1. **Authorize Plan** - Review and approve investigation approach
2. **Launch Parallel Tasks** - Start 4 sub-agents simultaneously
3. **Gather Evidence** - Each agent reports findings
4. **Review Results** - Evaluate all findings together
5. **Approve Removals** - Decide which configs to remove
6. **Execute Cleanup** - Remove approved configs
7. **Final Validation** - Full test suite at workspace level
8. **Review Changes** - Present all changes for approval before commit

## Notes

- All investigation is non-destructive (comment-out only initially)
- Full workspace testing required at every step
- No commits until final review complete
- Document "future use" configs to prevent confusion
- Update TOML schema docs after cleanup
