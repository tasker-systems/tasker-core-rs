//! # Worker Hydration Layer
//!
//! TAS-69: Type-safe message transformation from PGMQ messages to actor messages.
//!
//! This module provides hydrators that transform raw queue messages into
//! typed actor messages, following the same pattern as orchestration hydration.
//!
//! ## Architecture
//!
//! ```text
//! PgmqMessage ──→ Hydrator ──→ ActorMessage
//!                    │
//!                    └─→ Deserialize + Validate
//! ```

mod step_message_hydrator;

pub use step_message_hydrator::StepMessageHydrator;
