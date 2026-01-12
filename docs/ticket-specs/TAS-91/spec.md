# Python, Rust, Typescript Versions of our Blog Example Handlers

# TAS-91: Multi-Language Blog Examples - Research & Implementation Plan

## Executive Summary

[TAS-91](https://linear.app/tasker-systems/issue/TAS-91/python-rust-typescript-versions-of-our-blog-example-handlers) creates equivalent blog example handlers in Python, TypeScript, and Rust to complement the existing Ruby implementations. These examples serve dual purposes:

1. **Documentation**: Demonstrate tasker-core patterns in each language for the gitbook
2. **Testing**: Provide comprehensive E2E test coverage across all workers

---

## Current State Assessment

### Ruby Blog Examples (Reference Implementation ✅)

| Post | Directory | Handlers | Pattern |
| -- | -- | -- | -- |
| **01: E-commerce** | `post_01_ecommerce/` | 5 step handlers | Linear workflow |
| **02: Data Pipeline** | `post_02_data_pipeline/` | 8 step handlers | DAG with parallel branches (diamond) |
| **03: Microservices** | `post_03_microservices_coordination/` | 5 step handlers | Tree workflow, API integration |
| **04: Team Scaling** | `post_04_team_scaling/` | 9 step handlers (2 namespaces) | Namespace isolation |

**Ruby E2E Tests**: All 4 blog examples have corresponding Rust E2E tests.

### Python Worker Readiness

**Infrastructure** ✅:

* `StepHandler` base class with `call(context)` signature
* `ApiHandler` with HTTP methods and error classification
* `DecisionHandler` with `decision_success()`, `skip_branches()`
* `Batchable` mixin with `get_batch_context()`
* `StepContext` with `get_dependency_result()`

**Existing Pattern Examples**:

* `linear_handlers.py` - FetchData → TransformData → StoreData
* `diamond_handlers.py` - Diamond pattern
* `conditional_approval_handlers.py` - Decision routing
* `batch_processing_handlers.py` - Batch processing
* `domain_event_handlers.py` - Event publishing

**Gaps**:

* No blog-specific examples (ecommerce, data pipeline, etc.)
* Some examples are test scaffolding, not full business logic

### TypeScript Worker Readiness

**Infrastructure** ✅:

* `StepHandler` base class with `async call(context)` signature
* `ApiHandler` with HTTP methods
* `DecisionHandler` with `decisionSuccess()`, `skipBranches()`
* `Batchable` mixin
* `StepContext` with `getDependencyResult()`
* **Domain Events** (complete!) - `BasePublisher`, `BaseSubscriber`, `InProcessDomainEventPoller`

**Existing Pattern Examples**:

* `linear_workflow/` - Linear pattern
* `diamond_workflow/` - Diamond pattern
* `conditional_approval/` - Decision routing
* `batch_processing/` - Batch processing
* `domain_events/` - Publisher/subscriber examples

**Gaps**:

* No blog-specific examples
* Examples are minimal test scaffolding

### Rust Worker Readiness

**Infrastructure** ✅:

* `RustStepHandler` trait with `async call(&self, step_data)` signature
* No ApiHandler trait (not urgent - can use reqwest directly)
* No DecisionHandler trait (manual outcome construction)
* Batch processing via `BatchableStepHandler` trait
* Complete E2E test infrastructure

**Existing Examples**:

* `payment_example.rs` - Payment processing with [TAS-65](https://linear.app/tasker-systems/issue/TAS-65/distributed-event-system-architecture) patterns
* `batch_processing_example.rs` - Batch patterns
* `capability_examples.rs` - Various capabilities

**Gaps**:

* No blog-specific examples
* No DecisionHandler trait helpers (manual outcome construction)

---

## Handler Complexity Analysis

### Post 01: E-commerce Checkout (Linear)

| Handler | Complexity | Key Patterns |
| -- | -- | -- |
| `ValidateCartHandler` | Medium | Input validation, mock data, error types |
| `ProcessPaymentHandler` | High | Dependency access, error classification, retryable vs permanent |
| `UpdateInventoryHandler` | Low | Simple update simulation |
| `CreateOrderHandler` | Medium | Aggregate results from prior steps |
| `SendConfirmationHandler` | Low | Simple notification simulation |

**Total**: 5 handlers, \~300 lines Ruby

### Post 02: Data Pipeline (DAG/Diamond)

| Handler | Complexity | Key Patterns |
| -- | -- | -- |
| `ExtractSalesDataHandler` | Low | Independent extraction |
| `ExtractInventoryDataHandler` | Low | Independent extraction |
| `ExtractCustomerDataHandler` | Low | Independent extraction |
| `TransformSalesHandler` | Medium | Single dependency |
| `TransformInventoryHandler` | Medium | Single dependency |
| `TransformCustomersHandler` | Medium | Single dependency |
| `AggregateMetricsHandler` | High | **Convergence** - 3 dependencies |
| `GenerateInsightsHandler` | Medium | Single dependency |

**Total**: 8 handlers, \~400 lines Ruby, demonstrates DAG convergence

### Post 03: Microservices Coordination (Tree)

| Handler | Complexity | Key Patterns |
| -- | -- | -- |
| `CreateUserAccountHandler` | Medium | API-style, error handling |
| `SetupBillingProfileHandler` | Medium | Dependency on create_user |
| `InitializePreferencesHandler` | Low | Dependency on create_user |
| `SendWelcomeSequenceHandler` | Low | Multiple dependencies |
| `UpdateUserStatusHandler` | Medium | Convergence of all steps |

**Total**: 5 handlers, \~250 lines Ruby

### Post 04: Team Scaling (Namespaces)

**Customer Success Namespace** (5 handlers):

* `ValidateRefundRequestHandler`
* `CheckRefundPolicyHandler`
* `GetManagerApprovalHandler`
* `ExecuteRefundWorkflowHandler`
* `UpdateTicketStatusHandler`

**Payments Namespace** (4 handlers):

* `ValidatePaymentEligibilityHandler`
* `ProcessGatewayRefundHandler`
* `UpdatePaymentRecordsHandler`
* `NotifyCustomerHandler`

**Total**: 9 handlers across 2 namespaces, \~500 lines Ruby, demonstrates isolation

---

## Implementation Strategy

### Language-Specific Considerations

**Python**:

* Use type hints throughout (`StepContext`, `StepHandlerResult`)
* Dataclasses for mock models
* `dict` access patterns match Ruby's `Hash` access

**TypeScript**:

* Full async/await patterns
* Interfaces for mock models
* Strong typing with generics

**Rust**:

* `TaskSequenceStep` for step data access
* `serde_json::json!` for result construction
* Pattern matching for error handling
* `HashMap<String, Value>` for metadata

### File Structure (per language)

```
workers/{language}/tests/handlers/examples/blog_examples/
├── README.md                           # Overview, cross-language mapping
├── post_01_ecommerce/
│   ├── __init__.py | index.ts | mod.rs
│   ├── step_handlers/
│   │   ├── validate_cart_handler.{py|ts|rs}
│   │   ├── process_payment_handler.{py|ts|rs}
│   │   ├── update_inventory_handler.{py|ts|rs}
│   │   ├── create_order_handler.{py|ts|rs}
│   │   └── send_confirmation_handler.{py|ts|rs}
│   └── models/
│       ├── order.{py|ts|rs}
│       └── product.{py|ts|rs}
├── post_02_data_pipeline/
│   └── step_handlers/
│       ├── extract_sales_data_handler.{py|ts|rs}
│       ├── extract_inventory_data_handler.{py|ts|rs}
│       ├── extract_customer_data_handler.{py|ts|rs}
│       ├── transform_sales_handler.{py|ts|rs}
│       ├── transform_inventory_handler.{py|ts|rs}
│       ├── transform_customers_handler.{py|ts|rs}
│       ├── aggregate_metrics_handler.{py|ts|rs}
│       └── generate_insights_handler.{py|ts|rs}
├── post_03_microservices_coordination/
│   └── step_handlers/
│       └── ... (5 handlers)
└── post_04_team_scaling/
    └── step_handlers/
        ├── customer_success/
        │   └── ... (5 handlers)
        └── payments/
            └── ... (4 handlers)
```

---

## Implementation Phases

### Phase 1: Post 01 E-commerce (All Languages) - 3-4 days

**Rationale**: Linear workflow, most straightforward, establishes patterns

**Deliverables**:

- [ ] Python: 5 handlers + models + registration
- [ ] TypeScript: 5 handlers + types + registration
- [ ] Rust: 5 handlers (may need helper functions)
- [ ] E2E tests for Python and TypeScript (Ruby already exists)
- [ ] Update fixtures if needed

**Key Implementation Notes**:

* Mock product database as module-level constant
* Error handling: `PermanentError` for validation, `RetryableError` for transient
* Dependency result access patterns established

### Phase 2: Post 02 Data Pipeline (All Languages) - 3-4 days

**Rationale**: DAG pattern with convergence, demonstrates parallel execution

**Deliverables**:

- [ ] Python: 8 handlers with DAG awareness
- [ ] TypeScript: 8 handlers with DAG awareness
- [ ] Rust: 8 handlers
- [ ] E2E tests validating parallel execution order
- [ ] Convergence handler accessing 3 dependencies

**Key Implementation Notes**:

* Extract handlers can run in parallel (no dependencies)
* Transform handlers depend on their extract counterpart
* Aggregate handler demonstrates multi-dependency convergence

### Phase 3: Post 03 Microservices (All Languages) - 2-3 days

**Rationale**: Tree pattern, simulates API integration

**Deliverables**:

- [ ] Python: 5 handlers with API-style patterns
- [ ] TypeScript: 5 handlers (may use ApiHandler base)
- [ ] Rust: 5 handlers
- [ ] E2E tests

**Key Implementation Notes**:

* CreateUserAccount is the root
* SetupBilling and InitializePreferences depend only on create
* Tree structure naturally converges

### Phase 4: Post 04 Team Scaling (All Languages) - 3-4 days

**Rationale**: Namespace isolation, most complex structure

**Deliverables**:

- [ ] Python: 9 handlers across 2 namespace directories
- [ ] TypeScript: 9 handlers across 2 namespace directories
- [ ] Rust: 9 handlers (may use modules for namespace separation)
- [ ] E2E tests validating namespace isolation
- [ ] Registration patterns for namespaced handlers

**Key Implementation Notes**:

* Handler names prefixed with namespace (e.g., `customer_success::validate_refund_request`)
* Demonstrates team ownership patterns
* Cross-namespace calls (ExecuteRefundWorkflow calls payments handlers)

### Phase 5: Documentation & Integration - 2 days

**Deliverables**:

- [ ] [README.md](http://README.md) in each language's blog_examples directory
- [ ] Cross-language mapping table
- [ ] Integration with [TAS-66](https://linear.app/tasker-systems/issue/TAS-66/tasker-blog-rebuild-plan-tasker-core-centric-documentation) gitbook (codetabs ready)
- [ ] Handler registration documentation

---

## E2E Test Strategy

**Existing Ruby E2E Tests** (reference):

* `tests/e2e/ruby/ecommerce_order_test.rs`
* `tests/e2e/ruby/data_pipeline_test.rs`
* `tests/e2e/ruby/microservices_coordination_test.rs`
* `tests/e2e/ruby/namespace_isolation_test.rs`

**New E2E Tests Needed**:

* `tests/e2e/python/ecommerce_order_test.rs`
* `tests/e2e/python/data_pipeline_test.rs`
* `tests/e2e/python/microservices_coordination_test.rs`
* `tests/e2e/python/namespace_isolation_test.rs`
* `tests/e2e/typescript/ecommerce_order_test.rs`
* `tests/e2e/typescript/data_pipeline_test.rs`
* `tests/e2e/typescript/microservices_coordination_test.rs`
* `tests/e2e/typescript/namespace_isolation_test.rs`

**Note**: Rust handlers would be tested via the existing Rust E2E infrastructure.

---

## Effort Estimation

| Phase | Effort | Description |
| -- | -- | -- |
| Phase 1 | 3-4 days | E-commerce (5 handlers × 3 languages + tests) |
| Phase 2 | 3-4 days | Data Pipeline (8 handlers × 3 languages + tests) |
| Phase 3 | 2-3 days | Microservices (5 handlers × 3 languages + tests) |
| Phase 4 | 3-4 days | Team Scaling (9 handlers × 3 languages + tests) |
| Phase 5 | 2 days | Documentation & Integration |

**Total**: 13-17 days

---

## Dependencies

* [TAS-66](https://linear.app/tasker-systems/issue/TAS-66/tasker-blog-rebuild-plan-tasker-core-centric-documentation): GitBook rebuild (parallel work, codetabs integration)
* [TAS-112](https://linear.app/tasker-systems/issue/TAS-112/cross-language-step-handler-ergonomics-analysis-and-harmonization): Domain events (complete, provides observability patterns for Post 05 later)
* [TAS-122](https://linear.app/tasker-systems/issue/TAS-122/impl-stream-a-typescript-domain-events-module): TypeScript domain events (already complete!)

---

## Open Questions

1. **Rust DecisionHandler helpers**: Should we create a trait before implementing Post 04 decision handlers, or use manual outcome construction?
2. **YAML fixtures**: Do we need separate YAML task templates for each language, or can we share them?
3. **Mock data consistency**: Should mock product/order data be identical across languages for comparison, or language-idiomatic?
4. **Handler naming conventions**:
   * Ruby: `Ecommerce::StepHandlers::ValidateCartHandler`
   * Python: `validate_cart` (handler_name attribute)
   * TypeScript: `ValidateCartHandler` (static handlerName)
   * Rust: `validate_cart` (fn name())

   Should these be standardized?

---

## Success Criteria

- [ ] All 27 handlers implemented in Python, TypeScript, and Rust
- [ ] All E2E tests passing for all languages
- [ ] Handlers demonstrate idiomatic patterns for each language
- [ ] README documentation with cross-language mapping
- [ ] Integration with [TAS-66](https://linear.app/tasker-systems/issue/TAS-66/tasker-blog-rebuild-plan-tasker-core-centric-documentation) gitbook (codetabs compatible)
- [ ] Consistent behavior across languages (same inputs → same outputs)

---

## Key File Paths

**Ruby Reference Implementation**:

* `workers/ruby/spec/handlers/examples/blog_examples/`

**Target Locations**:

* Python: `workers/python/tests/handlers/examples/blog_examples/`
* TypeScript: `workers/typescript/tests/handlers/examples/blog_examples/`
* Rust: `workers/rust/src/step_handlers/blog_examples/`

**E2E Tests**:

* `tests/e2e/python/`
* `tests/e2e/typescript/`
* `tests/e2e/rust/`

**Related Documentation**:

* `docs/workers/api-convergence-matrix.md`
* `docs/workers/patterns-and-practices.md`
* `docs/ticket-specs/TAS-112/recommendations.md`

## Metadata
- URL: [https://linear.app/tasker-systems/issue/TAS-91/python-rust-typescript-versions-of-our-blog-example-handlers](https://linear.app/tasker-systems/issue/TAS-91/python-rust-typescript-versions-of-our-blog-example-handlers)
- Identifier: TAS-91
- Status: In Progress
- Priority: Low
- Assignee: Pete Taylor
- Labels: Improvement
- Project: [Tasker Core Workers](https://linear.app/tasker-systems/project/tasker-core-workers-3e6c7472b199). Workers in Tasker Core
- Related issues: TAS-65, TAS-122, TAS-112, TAS-66
- Created: 2025-12-17T16:44:30.885Z
- Updated: 2026-01-12T01:59:39.664Z
