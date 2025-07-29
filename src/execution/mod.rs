//! Execution Module - Command Pattern & ZeroMQ Architecture

// Legacy ZeroMQ components (will be removed in Phase 5)
pub mod message_protocols;
pub mod zeromq_pub_sub_executor;

// New Command Pattern Architecture
pub mod command;
pub mod command_handlers;
pub mod command_router;
pub mod errors;
pub mod executor;
pub mod generic_executor;
pub mod tokio_tcp_executor;
pub mod transport;
pub mod worker_pool;

// Legacy exports (ZeroMQ)
pub use message_protocols::{
    StepBatchRequest, StepBatchResponse, StepExecutionRequest, StepExecutionResult,
};
pub use zeromq_pub_sub_executor::ZmqPubSubExecutor;

// New Command Pattern exports
pub use command::{Command, CommandPayload, CommandResult, CommandType};
pub use command_router::{CommandHandler, CommandRouter};
pub use executor::{SocketType, TcpExecutorConfig, UnixDatagramConfig};
pub use generic_executor::{ExecutorStats, GenericExecutor, TcpExecutor, UnixDatagramExecutor};
pub use tokio_tcp_executor::TokioTcpExecutor;
pub use transport::{
    ConnectionInfo, TcpTransport, TcpTransportConfig, Transport, TransportConfig,
    TransportConnection, TransportListener, TransportType, UnixDatagramTransport,
    UnixDatagramTransportConfig,
};
pub use worker_pool::{WorkerPool, WorkerPoolConfig, WorkerState};
