use tasker_core::execution::command::{Command, CommandType, CommandPayload, CommandSource, HealthCheckLevel};

fn main() {
    let command = Command::new(
        CommandType::HealthCheck,
        CommandPayload::HealthCheck {
            diagnostic_level: HealthCheckLevel::Basic,
        },
        CommandSource::RubyWorker {
            id: "test".to_string(),
        },
    );
    
    let json = serde_json::to_string_pretty(&command).unwrap();
    println!("Expected Rust Command JSON format:");
    println!("{}", json);
}