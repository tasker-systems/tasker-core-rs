# ADR: Processor UUID Ownership Removal

**Status**: Accepted
**Date**: 2025-10
**Ticket**: [TAS-54](https://linear.app/tasker-systems/issue/TAS-54)

## Context

When orchestrators crash with tasks in active processing states (`Initializing`, `EnqueuingSteps`, `EvaluatingResults`), the processor UUID ownership enforcement prevented new orchestrators from taking over. Tasks became permanently stuck until manual intervention.

**Root Cause**: Three states required ownership enforcement (TAS-41 pattern), but when orchestrator A crashed and orchestrator B tried to recover, the ownership check failed: `B != A`.

**Production Impact**:
- Stuck tasks requiring manual intervention
- Orchestrator restarts caused task processing to halt
- 15-second gap between crash and retry, but tasks permanently blocked

## Decision

**Move to audit-only processor UUID tracking**:

1. **Keep** processor UUID in all transitions (audit trail for debugging)
2. **Remove** ownership enforcement from state transitions
3. **Rely on** existing state machine guards for idempotency
4. **Add** configuration flag for gradual rollout

**Key Insight**: The original problem (race conditions) had been solved by multiple other mechanisms since TAS-41:
- TAS-37: Atomic finalization claiming via SQL functions
- TAS-40: Command pattern with stateless async processors
- TAS-46: Actor pattern with 4 production-ready actors

### Idempotency Without Ownership

| Actor | Idempotency Mechanism | Race Condition Protection |
|-------|----------------------|---------------------------|
| TaskRequestActor | `identity_hash` unique constraint | Transaction atomicity |
| ResultProcessorActor | Current state guards | State machine atomicity |
| StepEnqueuerActor | SQL function atomicity | PGMQ transactional operations |
| TaskFinalizerActor | Atomic claiming (TAS-37) | SQL compare-and-swap |

## Consequences

### Positive

- **Task recovery**: Tasks automatically recover after orchestrator crashes
- **Zero manual interventions**: Stuck task count approaches zero
- **Audit trail preserved**: Full debugging capability retained
- **Instant rollback**: Configuration flag allows quick revert

### Negative

- **New debugging patterns**: Processor ownership changes visible in audit trail
- **Team training**: Operators need to understand audit-only interpretation

### Neutral

- No database schema changes required
- No performance impact (one fewer query per transition)

## Alternatives Considered

### Alternative 1: Timeout-Based Ownership Transfer

Add timeout after which ownership can be claimed by another processor.

**Rejected**: Adds complexity; existing idempotency guards make ownership redundant entirely.

### Alternative 2: Keep Ownership Enforcement

Continue with TAS-41 behavior, add manual recovery tools.

**Rejected**: Doesn't address root cause; manual intervention doesn't scale.

## References

- For historical implementation details, see [TAS-54](https://linear.app/tasker-systems/issue/TAS-54)
- [Defense in Depth](../principles/defense-in-depth.md) - Multi-layer protection philosophy
- [Idempotency and Atomicity](../architecture/idempotency-and-atomicity.md) - Defense layer documentation
