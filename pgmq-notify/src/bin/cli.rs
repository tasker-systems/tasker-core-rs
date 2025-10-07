//! CLI tool for generating PGMQ notification migration files
//!
//! This tool generates SQL migration files with customizable PGMQ notification
//! triggers based on configuration parameters.

use chrono::Utc;
use clap::{Args, Parser, Subcommand};
use pgmq_notify::PgmqNotifyConfig;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "pgmq-notify-cli")]
#[command(about = "Generate PGMQ notification migration files")]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Generate a new migration file
    GenerateMigration(GenerateMigrationArgs),
    /// Validate configuration file
    ValidateConfig(ValidateConfigArgs),
}

#[derive(Args)]
struct GenerateMigrationArgs {
    /// Output directory for migration file
    #[arg(short, long, default_value = "migrations")]
    output_dir: PathBuf,

    /// Configuration file path (optional)
    #[arg(short, long)]
    config: Option<PathBuf>,

    /// Queue naming pattern regex
    #[arg(long, default_value = r"(?P<namespace>\w+)_queue")]
    queue_pattern: String,

    /// Channel prefix for notifications
    #[arg(long)]
    channel_prefix: Option<String>,

    /// Migration name
    #[arg(short, long, default_value = "add_pgmq_notifications")]
    name: String,

    /// Include rollback (DOWN migration)
    #[arg(long, default_value_t = true)]
    include_rollback: bool,
}

#[derive(Args)]
struct ValidateConfigArgs {
    /// Configuration file path
    config: PathBuf,
}

#[derive(Serialize, Deserialize)]
struct CliConfig {
    /// Queue naming pattern for namespace extraction
    pub queue_naming_pattern: String,
    /// Optional prefix for notification channels
    pub channels_prefix: Option<String>,
    /// Maximum payload size in bytes
    pub max_payload_size: usize,
    /// Whether to include metadata in notifications
    pub include_metadata: bool,
    /// Migration-specific settings
    pub migration: MigrationConfig,
}

#[derive(Serialize, Deserialize)]
struct MigrationConfig {
    /// Include DROP statements for rollback
    pub include_rollback: bool,
    /// Custom migration name prefix
    pub name_prefix: String,
}

impl Default for CliConfig {
    fn default() -> Self {
        Self {
            queue_naming_pattern: r"(?P<namespace>\w+)_queue".to_string(),
            channels_prefix: None,
            max_payload_size: 7800,
            include_metadata: true,
            migration: MigrationConfig {
                include_rollback: true,
                name_prefix: "add_pgmq_notifications".to_string(),
            },
        }
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    match cli.command {
        Commands::GenerateMigration(args) => generate_migration(args)?,
        Commands::ValidateConfig(args) => validate_config(args)?,
    }

    Ok(())
}

fn generate_migration(args: GenerateMigrationArgs) -> Result<(), Box<dyn std::error::Error>> {
    // Load configuration
    let config = if let Some(config_path) = args.config {
        let config_content = fs::read_to_string(config_path)?;
        toml::from_str::<CliConfig>(&config_content)?
    } else {
        CliConfig {
            queue_naming_pattern: args.queue_pattern,
            channels_prefix: args.channel_prefix,
            include_metadata: true,
            max_payload_size: 7800,
            migration: MigrationConfig {
                include_rollback: args.include_rollback,
                name_prefix: args.name.clone(),
            },
        }
    };

    // Ensure output directory exists
    fs::create_dir_all(&args.output_dir)?;

    // Generate timestamp for migration filename
    let timestamp = Utc::now().format("%Y%m%d%H%M%S").to_string();
    let filename = format!("{}_{}.sql", timestamp, args.name);
    let filepath = args.output_dir.join(filename);

    // Generate migration content
    let migration_content = generate_migration_sql(&config)?;

    // Write migration file
    fs::write(&filepath, migration_content)?;

    println!("Generated migration file: {}", filepath.display());
    Ok(())
}

fn validate_config(args: ValidateConfigArgs) -> Result<(), Box<dyn std::error::Error>> {
    let config_content = fs::read_to_string(args.config)?;
    let cli_config: CliConfig = toml::from_str(&config_content)?;

    // Convert to PgmqNotifyConfig for validation
    let mut pgmq_config = PgmqNotifyConfig::new()
        .with_queue_naming_pattern(&cli_config.queue_naming_pattern)
        .with_max_payload_size(cli_config.max_payload_size)
        .with_metadata_included(cli_config.include_metadata);

    if let Some(ref prefix) = cli_config.channels_prefix {
        pgmq_config = pgmq_config.with_channels_prefix(prefix.clone());
    }

    match pgmq_config.validate() {
        Ok(()) => {
            println!("Configuration is valid");
            println!("   Queue pattern: {}", cli_config.queue_naming_pattern);
            println!("   Channel prefix: {:?}", cli_config.channels_prefix);
            println!("   Max payload: {} bytes", cli_config.max_payload_size);
        }
        Err(e) => {
            eprintln!("Configuration validation failed: {}", e);
            std::process::exit(1);
        }
    }

    Ok(())
}

fn generate_migration_sql(config: &CliConfig) -> Result<String, Box<dyn std::error::Error>> {
    // Convert regex pattern for PostgreSQL
    let pg_pattern = config.queue_naming_pattern.replace("(?P<namespace>", "(");

    // Build channel names
    let queue_created_channel = match &config.channels_prefix {
        Some(prefix) => format!("{}.pgmq_queue_created", prefix),
        None => "pgmq_queue_created".to_string(),
    };

    let message_ready_channel_template = match &config.channels_prefix {
        Some(prefix) => format!("{}.pgmq_message_ready", prefix),
        None => "pgmq_message_ready".to_string(),
    };

    let global_message_ready_channel = message_ready_channel_template.clone();
    let namespace_message_ready_channel = format!(
        "{}.' || namespace_name || '",
        message_ready_channel_template
    );

    let migration_sql = format!(
        r#"-- Migration: Add PGMQ Notification Triggers
-- Generated at: {}
-- Configuration:
--   Queue pattern: {}
--   Channel prefix: {:?}
--   Max payload size: {} bytes

-- UP Migration

-- Function to notify when queues are created
CREATE OR REPLACE FUNCTION pgmq_notify_queue_created()
RETURNS trigger AS $$
DECLARE
    event_payload TEXT;
    channel_name TEXT := '{}';
    namespace_name TEXT;
BEGIN
    -- Extract namespace from queue name using configured pattern
    namespace_name := (regexp_match(NEW.queue_name, '{}'))[1];
    IF namespace_name IS NULL THEN
        namespace_name := 'default';
    END IF;
    
    -- Build event payload
    event_payload := json_build_object(
        'event_type', 'queue_created',
        'queue_name', NEW.queue_name,
        'namespace', namespace_name,
        'created_at', NOW()::timestamptz
    )::text;
    
    -- Truncate if payload exceeds limit
    IF length(event_payload) > {} THEN
        event_payload := substring(event_payload, 1, {}) || '...}}';
    END IF;
    
    -- Send notification
    PERFORM pg_notify(channel_name, event_payload);
    
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

-- Function to notify when messages are ready
CREATE OR REPLACE FUNCTION pgmq_notify_message_ready()
RETURNS trigger AS $$
DECLARE
    event_payload TEXT;
    namespace_channel TEXT;
    global_channel TEXT := '{}';
    namespace_name TEXT;
    queue_name_val TEXT;
BEGIN
    -- Get queue name from table name (remove 'q_' prefix)
    queue_name_val := substring(TG_TABLE_NAME, 3);
    
    -- Extract namespace from queue name using configured pattern
    namespace_name := (regexp_match(queue_name_val, '{}'))[1];
    IF namespace_name IS NULL THEN
        namespace_name := 'default';
    END IF;
    
    -- Build namespace-specific channel name
    namespace_channel := '{}';
    
    -- Build event payload
    event_payload := json_build_object(
        'event_type', 'message_ready',
        'msg_id', NEW.msg_id,
        'queue_name', queue_name_val,
        'namespace', namespace_name,
        'ready_at', NOW()::timestamptz,
        'visibility_timeout_seconds', EXTRACT(EPOCH FROM NEW.vt - NOW())::integer
    )::text;
    
    -- Truncate if payload exceeds limit
    IF length(event_payload) > {} THEN
        event_payload := substring(event_payload, 1, {}) || '...}}';
    END IF;
    
    -- Send to namespace-specific channel
    PERFORM pg_notify(namespace_channel, event_payload);
    
    -- Also send to global channel if different
    IF namespace_channel != global_channel THEN
        PERFORM pg_notify(global_channel, event_payload);
    END IF;
    
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

-- Install trigger on pgmq.meta table for queue creation notifications
-- (Only if the table exists)
DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM information_schema.tables 
               WHERE table_schema = 'pgmq' AND table_name = 'meta') THEN
        
        -- Drop existing trigger if it exists (migrations might run multiple times)
        DROP TRIGGER IF EXISTS pgmq_queue_created_trigger ON pgmq.meta;

        CREATE TRIGGER pgmq_queue_created_trigger
            AFTER INSERT ON pgmq.meta
            FOR EACH ROW
            EXECUTE FUNCTION pgmq_notify_queue_created();
            
        RAISE NOTICE 'Installed queue creation trigger on pgmq.meta';
    ELSE
        RAISE NOTICE 'pgmq.meta table not found - skipping queue creation trigger';
    END IF;
END;
$$;

-- Install message ready triggers on all existing PGMQ queue tables
DO $$
DECLARE
    queue_record RECORD;
    trigger_name TEXT;
BEGIN
    FOR queue_record IN
        SELECT table_name
        FROM information_schema.tables
        WHERE table_schema = 'pgmq' 
          AND table_name LIKE 'q_%'
    LOOP
        trigger_name := 'pgmq_message_ready_trigger_' || queue_record.table_name;
        
        -- Drop existing trigger if it exists (migrations might run multiple times)
        EXECUTE format('DROP TRIGGER IF EXISTS %I ON pgmq.%I',
            trigger_name, queue_record.table_name);

        EXECUTE format('CREATE TRIGGER %I
            AFTER INSERT ON pgmq.%I
            FOR EACH ROW
            EXECUTE FUNCTION pgmq_notify_message_ready()',
            trigger_name, queue_record.table_name);
            
        RAISE NOTICE 'Installed message ready trigger on pgmq.%', queue_record.table_name;
    END LOOP;
END;
$$;"#,
        Utc::now().format("%Y-%m-%d %H:%M:%S UTC"),
        config.queue_naming_pattern,
        config.channels_prefix,
        config.max_payload_size,
        queue_created_channel,
        pg_pattern,
        config.max_payload_size,
        config.max_payload_size - 10,
        global_message_ready_channel,
        pg_pattern,
        namespace_message_ready_channel,
        config.max_payload_size,
        config.max_payload_size - 10
    );

    // Add rollback if requested
    let rollback_sql = if config.migration.include_rollback {
        r#"

-- DOWN Migration (Rollback)

-- Drop triggers on all PGMQ queue tables
DO $$
DECLARE
    trigger_record RECORD;
BEGIN
    FOR trigger_record IN
        SELECT trigger_name, event_object_table
        FROM information_schema.triggers
        WHERE trigger_schema = 'pgmq'
          AND trigger_name LIKE 'pgmq_%_trigger%'
    LOOP
        EXECUTE format('DROP TRIGGER IF EXISTS %I ON pgmq.%I',
            trigger_record.trigger_name, trigger_record.event_object_table);
            
        RAISE NOTICE 'Dropped trigger % on pgmq.%', 
            trigger_record.trigger_name, trigger_record.event_object_table;
    END LOOP;
END;
$$;

-- Drop trigger functions
DROP FUNCTION IF EXISTS pgmq_notify_message_ready() CASCADE;
DROP FUNCTION IF EXISTS pgmq_notify_queue_created() CASCADE;
"#
    } else {
        ""
    };

    Ok(format!("{}{}", migration_sql, rollback_sql))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = CliConfig::default();
        assert_eq!(config.queue_naming_pattern, r"(?P<namespace>\w+)_queue");
        assert_eq!(config.channels_prefix, None);
        assert_eq!(config.max_payload_size, 7800);
    }

    #[test]
    fn test_migration_sql_generation() {
        let config = CliConfig::default();
        let sql = generate_migration_sql(&config).unwrap();

        assert!(sql.contains("CREATE OR REPLACE FUNCTION pgmq_notify_queue_created()"));
        assert!(sql.contains("CREATE OR REPLACE FUNCTION pgmq_notify_message_ready()"));
        assert!(sql.contains("pgmq_queue_created"));
        assert!(sql.contains("pgmq_message_ready"));
    }

    #[test]
    fn test_migration_with_prefix() {
        let config = CliConfig {
            channels_prefix: Some("app1".to_string()),
            ..Default::default()
        };

        let sql = generate_migration_sql(&config).unwrap();
        assert!(sql.contains("app1.pgmq_queue_created"));
        assert!(sql.contains("app1.pgmq_message_ready"));
    }
}
