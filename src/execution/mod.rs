//! Execution Module - Command Pattern & ZeroMQ Architecture

// Legacy ZeroMQ components (will be removed in Phase 5)
pub mod message_protocols;
pub mod zeromq_pub_sub_executor;

// New Command Pattern Architecture
pub mod command;
pub mod command_router;
pub mod tokio_tcp_executor;
pub mod worker_pool;
pub mod command_handlers;
pub mod executor;
pub mod errors;
pub mod transport;
pub mod generic_executor;

// Legacy exports (ZeroMQ)
pub use message_protocols::{
    StepBatchRequest, StepBatchResponse, StepExecutionRequest, StepExecutionResult,
};
pub use zeromq_pub_sub_executor::ZmqPubSubExecutor;

// New Command Pattern exports
pub use command::{Command, CommandPayload, CommandType, CommandResult};
pub use command_router::{CommandRouter, CommandHandler};
pub use tokio_tcp_executor::TokioTcpExecutor;
pub use executor::{TcpExecutorConfig, UnixDatagramConfig, SocketType};
pub use worker_pool::{WorkerPool, WorkerState, WorkerPoolConfig};
pub use transport::{
    Transport, TransportListener, TransportConnection, TransportConfig,
    TcpTransport, TcpTransportConfig, UnixDatagramTransport, UnixDatagramTransportConfig,
    ConnectionInfo, TransportType
};
pub use generic_executor::{GenericExecutor, TcpExecutor, UnixDatagramExecutor, ExecutorStats};
