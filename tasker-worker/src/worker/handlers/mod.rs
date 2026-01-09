//! # Handler Abstractions
//!
//! TAS-67: Language-agnostic handler traits and dispatch services for step execution.
//!
//! This module provides the foundation for a unified handler dispatch architecture
//! that supports Rust, Ruby, and Python step handlers through common abstractions.
//!
//! ## Components
//!
//! - **Traits** (`traits.rs`): `StepHandlerRegistry` and `StepHandler` traits
//! - **Dispatch Service** (`dispatch_service.rs`): Non-blocking handler invocation
//! - **Completion Processor** (`completion_processor.rs`): Result routing to orchestration
//! - **FFI Dispatch Channel** (`ffi_dispatch_channel.rs`): Polling-based dispatch for FFI
//!
//! ## Architecture
//!
//! ```text
//! ┌────────────────────────────────────────────────────────────────────────┐
//! │                     HANDLER DISPATCH FLOW                               │
//! └────────────────────────────────────────────────────────────────────────┘
//!
//! StepExecutorActor ──→ Dispatch Channel ──→ HandlerDispatchService
//!                            │                       │
//!                            │                       ├─→ Rust: registry.get() → handler.call()
//!                            │                       │
//!                            │                       └─→ FFI: FfiDispatchChannel.poll()
//!                            │                                    │
//!                            │                                    └─→ Ruby/Python handler
//!                            │
//!                            └──→ Completion Channel ──→ CompletionProcessorService
//!                                                              │
//!                                                              └─→ Orchestration Queue
//! ```

mod completion_processor;
mod dispatch_service;
mod domain_event_callback;
mod ffi_completion_circuit_breaker;
mod ffi_dispatch_channel;
mod resolver_integration;
mod traits;

// Re-export core traits
pub use traits::{StepHandler, StepHandlerRegistry};

// Re-export dispatch service
pub use dispatch_service::{
    create_dispatch_channels, CapacityChecker, HandlerDispatchConfig, HandlerDispatchService,
    LoadSheddingConfig, NoOpCallback, PostHandlerCallback,
};

// Re-export completion processor
pub use completion_processor::{CompletionProcessorConfig, CompletionProcessorService};

// Re-export FFI dispatch channel
pub use ffi_dispatch_channel::{
    FfiDispatchChannel, FfiDispatchChannelConfig, FfiDispatchMetrics, FfiStepEvent,
};

// Re-export FFI completion circuit breaker (TAS-75 Phase 5a)
pub use ffi_completion_circuit_breaker::{
    FfiCompletionCircuitBreaker, FfiCompletionCircuitBreakerConfig,
    FfiCompletionCircuitBreakerMetrics,
};

// Re-export domain event callback (shared by Rust/Ruby/Python workers)
pub use domain_event_callback::DomainEventCallback;

// TAS-93: Re-export resolver integration (adapter pattern for ResolverChain)
pub use resolver_integration::{
    ExecutableHandler, HandlerExecutor, HybridStepHandlerRegistry, ResolverChainRegistry,
    StepHandlerAsResolved, StepHandlerExecutor,
};
