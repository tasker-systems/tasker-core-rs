use anyhow::Result;

use crate::bootstrap::{bootstrap as bootstrap_worker, no_web_api_config};

use tasker_core::test_helpers::shared_test_setup::SharedTestSetup;

pub fn init_test_logging() {
    let _ = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .try_init();
}

pub async fn init_test_worker() -> Result<SharedTestSetup> {
    let mut setup = SharedTestSetup::new()?;
    let worker_handle = bootstrap_worker(no_web_api_config()).await?;
    setup.set_worker_handle(worker_handle);
    Ok(setup)
}
