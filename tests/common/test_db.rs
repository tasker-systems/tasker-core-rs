/// Generate a unique name for test data
pub fn unique_name(prefix: &str) -> String {
    let timestamp = chrono::Utc::now().timestamp_millis();
    let thread_id = std::thread::current().id();
    let random: u32 = fastrand::u32(..);
    format!("{prefix}_{timestamp}_{random}_{thread_id:?}")
}
