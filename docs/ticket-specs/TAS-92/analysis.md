# Consistent Developer-space APIs and Ergonomics (Parent)

# Overview

Analysis of Ruby, Python, and Rust worker implementations revealed design inconsistencies in developer-facing APIs. While respecting language idioms, we can align common touchpoints (handler signatures, result factories, registry calls, domain event publishers/subscribers) to reduce context switching.

**Status**: Pre-alpha - no backward compatibility required

# Analysis Sources

* `docs/worker-crates/README.md`, `patterns-and-practices.md`, `ruby.md`, `python.md`, `rust.md`
* `docs/domain-events.md`, `docs/events-and-commands.md`, `docs/worker-event-systems.md`

# Current State

## 1\. Handler Signatures and Context

| Language | Signature | Context Structure |
| -- | -- | -- |
| Ruby | `call(task, sequence, step)` | Three separate wrapper objects |
| Python | `call(context: StepContext)` | Unified context object |
| Rust | `call(&TaskSequenceStep)` | Unified context struct |

## 2\. Result API and Error Semantics

| Aspect | Ruby | Python | Rust |
| -- | -- | -- | -- |
| Success method | `success(result:, metadata:)` | `self.success(result, metadata)` | `StepExecutionResult::success(...)` |
| Failure method | `failure(message:, error_type:, error_code:, retryable:, metadata:)` | `self.failure(message, error_type, retryable, metadata)` | `StepExecutionResult::failure(message, error_type, retryable)` |
| Factory names | `.success()`, `.error()` | `.success_handler_result()`, `.failure_handler_result()` | N/A (direct construction) |
| Result types | Separate `Success` / `Error` structs | Single `StepHandlerResult` class | Single `StepExecutionResult` struct |
| error_code field | ✅ Freeform string | ❌ Not present | ❌ Not present |
| error_type field | ✅ Enum-like values | ⚠️ Freeform string | ✅ Enum-like values |

## 3\. Registry API

| Operation | Ruby | Python |
| -- | -- | -- |
| Register | `register_handler(name, klass)` | `register(name, klass)` |
| Check | `handler_available?(name)` | `is_registered(name)` |
| List | `registered_handlers` | `list_handlers()` |
| Resolve | Via `StepHandlerResolver` | `resolve(name)` |

## 4\. Specialized Handlers

### API Handler

* **Ruby**: `connection.get(...)` + `process_response(response)` (explicit error classification)
* **Python**: `self.get/post/put/delete(...)` + `self.api_success(response)` (automatic classification)

### Decision Handler

* **Ruby**: `decision_success(steps: [...], result_data: {})`
* **Python**: `self.decision_success(DecisionPointOutcome.create_steps([...], routing_context={}))`

### Batchable

* **Ruby**: `batch_worker_complete(processed_count:, result_data:)`
* **Python**: `batch_worker_success(items_processed=, items_succeeded=, ...)`
* Field naming differs: `processed_count` vs `items_processed`

## 5\. Domain Events

### Publishers

* **Rust**: `StepEventPublisher` trait with `publish(ctx: &StepEventContext)`
* **Ruby**: `BasePublisher` with `transform_payload`, `should_publish?`, `additional_metadata`
* **Python**: No documented publisher base class yet

### Subscribers

* **Rust**: EventHandler closures registered with `InProcessEventBus`
* **Ruby**: `BaseSubscriber` with `subscribes_to` pattern + `handle(event)`
* **Python**: No documented subscriber base for domain events (only worker internal EventBridge)

# Target State

## A. Unified Handler Call Surface

**All languages**: Standardize on `call(context)` where context is a unified object/struct with:

* `task_uuid`, `step_uuid`, `input_data`, `dependency_results`
* `step_config`, `step_inputs`, `retry_count`, `max_retries`

**Ruby**: Migrate from `call(task, sequence, step)` → `call(context)` with a `StepContext` wrapper.

## B. Aligned Result Factories and Fields

**Method names**: Use `success(...)` / `failure(...)` everywhere

* Ruby: Keep current (already aligned)
* Python: Rename to `success(...)` / `failure(...)` (remove `_handler_result` suffix)

**Fields**: Standardize on:

* `success` (bool), `result` (dict/hash), `metadata` (dict/hash)
* `error_message`, `error_type`, `error_code` (optional), `retryable`

**error_type**: Define recommended enum-like values:

* `permanent_error`, `retryable_error`, `validation_error`, `timeout`, `handler_error`

**error_code**: Add to Python and Rust as optional field.

## C. Registry API Parity

**All languages**: Use these method names:

* `register(name, handler_class)`
* `is_registered(name) -> bool`
* `resolve(name) -> handler_instance`
* `list_handlers() -> list[str]`

## D. Specialized Handler Convergence

### API Handler

* **Ruby**: Add `get/post/put/delete` sugar methods
* **Python**: Keep current
* **Status codes**: Document unified classification table

### Decision Handler

* **Python**: Add `decision_success(steps: list[str], routing_context: dict | None)` helper
* **Ruby**: Optional: accept `DecisionPointOutcome` object for parity

### Batchable

* **Naming**: Use `batch_worker_success(...)` in all languages
* **Cursor context**: Standardize field names: `start_cursor`, `end_cursor`, `batch_size`, `last_cursor`
* **Context helper**: Use `get_batch_context(context)` in all languages

## E. Domain Events Publisher/Subscriber Parity

### Publisher Contract

* All languages: `name()` and `publish(ctx)` methods
* Context fields: `task_uuid`, `step_uuid`, `step_name`, `namespace`, `correlation_id`, `result`, `metadata`

### Subscribers

* **Python**: Add `BaseSubscriber` with `subscribes_to(pattern)` + `handle(event)` for Fast-mode events
* Ensure event payload/metadata parity across languages

## F. Naming Conventions

* **Handler names**: Use `snake_case` everywhere (even for Ruby class names in registry)
* **Structured logging keys**: Document common keys: `task_uuid`, `step_uuid`, `correlation_id`, `handler`, `namespace`, `duration_ms`

# Implementation Strategy

Since this is pre-alpha:

1. Make breaking changes directly (no aliases/deprecation needed)
2. Update all example handlers in each language
3. Update documentation with convergence matrix
4. Update tests to reflect new APIs

# Sub-Issues

See:

* [TAS-96](https://linear.app/tasker-systems/issue/TAS-96/ruby-worker-api-alignment) 
* [TAS-95](https://linear.app/tasker-systems/issue/TAS-95/python-worker-api-alignment) 
* [TAS-97](https://linear.app/tasker-systems/issue/TAS-97/rust-worker-api-alignment) 
* [TAS-98](https://linear.app/tasker-systems/issue/TAS-98/cross-language-worker-api-documentation-updates) 

# Acceptance Criteria

- [ ] All three languages use `call(context)` signature
- [ ] Result factories named `success(...)` / `failure(...)` with consistent fields
- [ ] Registry APIs use `register`, `is_registered`, `resolve`, `list_handlers`
- [ ] Specialized handlers (API/Decision/Batchable) have consistent method names
- [ ] Domain event publisher/subscriber contracts documented and implemented
- [ ] Convergence matrix in docs showing final API surface per language
- [ ] All example handlers updated to new APIs
- [ ] Tests passing in all three worker implementations

# Open Questions

* Should Ruby `StepContext` be a new class or a compatibility wrapper?
* Python error_type: hard enum or soft with Literal type hints?
* Rust error_code: first-class field or metadata?

## Metadata
- URL: [https://linear.app/tasker-systems/issue/TAS-92/consistent-developer-space-apis-and-ergonomics-parent](https://linear.app/tasker-systems/issue/TAS-92/consistent-developer-space-apis-and-ergonomics-parent)
- Identifier: TAS-92
- Status: In Progress
- Priority: Medium
- Assignee: Pete Taylor
- Labels: Improvement
- Project: [Tasker Core Workers](https://linear.app/tasker-systems/project/tasker-core-workers-3e6c7472b199). Workers in Tasker Core
- Created: 2025-12-17T22:33:41.773Z
- Updated: 2025-12-19T17:38:17.358Z

## Comments

- Pete Taylor (thread resolved):

  Cross-language consistency analysis (Ruby, Python, Rust) — proposals for TAS-92

  Date: 2025-12-19

  Scope and sources reviewed

  * docs/worker-crates/README.md, [patterns-and-practices.md](http://patterns-and-practices.md), [ruby.md](http://ruby.md), [python.md](http://python.md), [rust.md](http://rust.md)
  * docs/domain-events.md, docs/events-and-commands.md, docs/worker-event-systems.md

  Executive summary
  We don’t want identical APIs across languages, but we can make developer-facing touchpoints (handler signatures, result factories, registry calls, domain-event publisher/subscriber surface) consistent enough to reduce context switching. Below are concrete alignment proposals that preserve idioms and maintain backward compatibility via aliases.

  Findings (current state)

  1. Step handler signatures and context

  * Ruby: Base#call(task, sequence, step). Context is spread across three wrappers.
  * Python: StepHandler.call(context: StepContext). Unified context object.
  * Rust: RustStepHandler::call(&TaskSequenceStep). Unified context struct.

  2. Result API and error semantics

  * Ruby: Base helpers success(result:, metadata:), failure(message:, error_type:, error_code:, retryable:, metadata:). error_type is enum-like; error_code supported.
  * Python: Base helpers self.success(...) / self.failure(...), and class factories StepHandlerResult.success_handler_result(...) / failure_handler_result(...). error_type freeform; no error_code field.
  * Rust: StepExecutionResult::success(...) / ::failure(message, error_type, retryable). No error_code.

  3. Handler registry ergonomics

  * Ruby: registry.register_handler(name, klass), handler_available?, registered_handlers, resolve via resolver.
  * Python: registry.register(name, klass), is_registered, resolve, list_handlers.

  4. Specialized handlers

  * API: Ruby uses connection.get + process_response; Python exposes get/post/put/delete and api_success classification helper.
  * Decision: Ruby decision_success(steps:, result_data: {}); Python decision_success(DecisionPointOutcome), no simple steps: \[...\] convenience.
  * Batchable: Ruby Batchable#batch_worker_complete(...), extract_cursor_context; Python mixin Batchable with batch_worker_success(...), get_batch_context(...). Field names differ slightly (processed_count vs items_processed).

  5. Domain events (publishers/subscribers)

  * Publisher: Rust trait StepEventPublisher with publish(ctx). Ruby BasePublisher with transform_payload/should_publish?/additional_metadata; both route through EventRouter with modes Durable/Fast/Broadcast.
  * Subscriber: Rust subscribers via InProcessEventBus (tokio::broadcast). Ruby BaseSubscriber with subscribes_to pattern. Python has EventBridge for worker internals, but no documented BaseSubscriber parity for domain events yet.

  Proposals (backward-compatible)
  A. Unify handler call surface (conceptual)

  * Standardize on a conceptual signature call(context) across languages.
  * Ruby: keep call(task, sequence, step) for compatibility, but Base will construct a StepContext-like object and forward to call_with_context(context). New handlers can optionally implement call(context).
  * Document the mapping table from Ruby wrappers → StepContext fields.

  B. Align result factories and fields

  * Method names: encourage success(...) / failure(...) everywhere.
    * Python: keep self.success/self.failure; add classmethod aliases StepHandlerResult.success(...) and .failure(...) that delegate to existing factories. Keep existing \*\_handler_result names as deprecated aliases.
  * Error shape: adopt optional error_code across all three.
    * Python: add optional error_code field (tolerated in metadata for now if type changes are deferred).
    * Rust: accept error_code via metadata field on failure; core type addition can be a follow-up (non-breaking by keeping it optional).
  * error_type semantics: publish a recommended set (permanent_error, retryable_error, validation_error, timeout, handler_error).
    * Python: introduce Literal/Enum of well-known values in [types.py](http://types.py) but continue to accept freeform for compatibility.

  C. Registry API parity

  * Ruby: add aliases register, is_registered, resolve, list_handlers that proxy to existing methods.
  * Python: no change.
  * Docs: one cross-language snippet for “register/resolve/list”.

  D. Specialized handler convergence

  * API Handler
    * Ruby: add sugar methods get/post/put/delete on Api base to mirror Python; process_response internally.
    * Docs: single status table for classification (400/401/403/404/422 → permanent; 408/429/5xx → retryable).
  * Decision Handler
    * Python: add decision_success(steps: list\[str\], routing_context: dict | None) helper that builds DecisionPointOutcome under the hood.
    * Ruby: optional DecisionPointOutcome builder to accept a constructed object for parity.
  * Batchable
    * Naming: converge on batch_worker_success(...) in both languages; keep Ruby batch_worker_complete(...) as an alias with deprecation notice.
    * Cursor context: standardize field names {start_cursor, end_cursor, batch_size, last_cursor}.
    * Provide get_batch_context(context) in Ruby for parity; keep extract_cursor_context as alias.

  E. Domain events publisher/subscriber parity

  * Publisher contract
    * Document a cross-language contract: name() and publish(ctx) with StepEventContext fields (task_uuid, step_uuid, step_name, namespace, correlation_id, result, metadata).
    * Ruby: BasePublisher already aligns semantically; add publish(ctx) that delegates to transform_payload/should_publish?.
  * Subscribers
    * Python: add BaseSubscriber equivalent with subscribes_to(pattern) + handle(event) to consume Fast-mode events via FFI channel, mirroring Ruby.
    * Ensure event payload/metadata parity in Python subscriber callbacks.

  F. Naming conventions

  * Handler names: recommend snake_case handler_name across all languages. Ruby registry should default the name to snake_case of class unless explicitly overridden.
  * Structured logging keys: document common keys (task_uuid, step_uuid, correlation_id, handler, namespace, duration_ms).

  G. Documentation updates

  * Update docs/worker-crates/\* with a single cross-language reference for:
    * Handler signature and context fields
    * Result factory names and fields matrix
    * Specialized handler method names (API/Decision/Batchable)
    * Registry API
    * Domain events publisher/subscriber contracts
  * Add a “Convergence Matrix” that lists current + alias + target per language.

  Migration strategy

  * Ship as minor releases per worker: introduce aliases first, keep both forms for 2 minor versions, emit deprecation warnings in logs when old forms are used (guarded by TASKER_ENV != test).
  * Gate any breaking core type changes (e.g., Rust error_code) behind optional metadata first; follow up with a typed field only if/when needed.
  * Provide codemods/snippets for common transitions (e.g., Ruby batch_worker_complete → batch_worker_success).

  Acceptance criteria for TAS-92 (proposed)

  * Docs updated with the cross-language spec and convergence matrix.
  * Ruby: aliases added (registry methods, API handler HTTP helpers, batch_worker_success alias, optional call(context) path).
  * Python: StepHandlerResult.success()/failure() classmethod aliases; decision_success(steps, routing_context) helper; optional error_code accepted and passed through metadata.
  * Rust: failure() accepts metadata.error_code and preserves it through to orchestration + domain events (no schema change required).
  * Example handlers in each language updated to the target ergonomic surface.

  Open questions

  * Do we want a hard enum for error_type in Python or keep soft-enum with Literal type hints?
  * Should Ruby fully pivot to call(context) for new handlers, or keep dual-form indefinitely?
  * Is adding error_code as a first-class field in Rust desirable, or is metadata sufficient?

  Next steps

  * If this direction looks good, I can break this into sub-issues by language and file PRs to:
    1. Implement aliases + helpers
    2. Update docs with convergence matrix and examples
    3. Add deprecation warnings and toggles

## Sub-issues

- [TAS-95 Python Worker API Alignment](https://linear.app/tasker-systems/issue/TAS-95/python-worker-api-alignment)
- [TAS-96 Ruby Worker API Alignment](https://linear.app/tasker-systems/issue/TAS-96/ruby-worker-api-alignment)
- [TAS-97 Rust Worker API Alignment](https://linear.app/tasker-systems/issue/TAS-97/rust-worker-api-alignment)
- [TAS-98 Cross-Language Worker API Documentation Updates](https://linear.app/tasker-systems/issue/TAS-98/cross-language-worker-api-documentation-updates)
