pub mod config;
pub mod error;
pub mod models;
pub mod orchestration;
pub mod state_machine;
pub mod events;
pub mod registry;
pub mod database;
pub mod ffi;

pub use config::TaskerConfig;
pub use error::{TaskerError, Result};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_loads_successfully() {
        let config = TaskerConfig::default();
        assert_eq!(config.max_concurrent_steps, 10);
        assert_eq!(config.retry_limit, 3);
    }
}
