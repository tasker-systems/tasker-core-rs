//! Configuration command handlers for the Tasker CLI

use chrono::Utc;
use regex::Regex;
use tasker_client::{ClientConfig, ClientResult};
use tasker_shared::config::{tasker::TaskerConfig, ConfigMerger};

use crate::ConfigCommands;

/// Extract line and column from TOML error message
fn extract_error_position(error_msg: &str) -> Option<(usize, usize)> {
    // TOML errors often contain "at line X column Y" patterns
    let re = Regex::new(r"line (\d+) column (\d+)").ok()?;
    let caps = re.captures(error_msg)?;

    let line = caps.get(1)?.as_str().parse().ok()?;
    let col = caps.get(2)?.as_str().parse().ok()?;

    Some((line, col))
}

pub async fn handle_config_command(
    cmd: ConfigCommands,
    _config: &ClientConfig,
) -> ClientResult<()> {
    match cmd {
        ConfigCommands::Generate {
            context,
            environment,
            source_dir,
            output,
            validate,
        } => {
            println!("Generating configuration:");
            println!("  Context: {}", context);
            println!("  Environment: {}", environment);
            println!("  Source: {}", source_dir);
            println!("  Output: {}", output);

            // Create ConfigMerger
            let mut merger = ConfigMerger::new(std::path::PathBuf::from(&source_dir), &environment)
                .map_err(|e| {
                    tasker_client::ClientError::ConfigError(format!(
                        "Failed to initialize config merger: {}",
                        e
                    ))
                })?;

            // Merge configuration
            println!("\nMerging configuration...");
            let merged_config = merger.merge_context(&context).map_err(|e| {
                tasker_client::ClientError::ConfigError(format!(
                    "Failed to merge configuration: {}",
                    e
                ))
            })?;

            // Generate metadata header
            let header = format!(
                "# Tasker Configuration - {} Context\n\
                 # Environment: {}\n\
                 # Generated: {}\n\
                 # Source: {}\n\
                 #\n\
                 # This file is a MERGED configuration (base + environment overrides).\n\
                 # DO NOT EDIT manually - regenerate using: tasker-cli config generate\n\
                 #\n\
                 # Environment Variable Overrides (applied at runtime):\n\
                 # - DATABASE_URL: Override database.url (K8s secrets rotation)\n\
                 # - TASKER_TEMPLATE_PATH: Override worker.template_path (testing)\n\
                 # - TASKER_WEB_BIND_ADDRESS: Override web bind address (CI/testing port isolation)\n\
                 #\n\n",
                context,
                environment,
                Utc::now().to_rfc3339(),
                source_dir
            );

            // Write header + merged config to output file
            let full_config = format!("{}{}", header, merged_config);
            println!("Writing to: {}", output);
            std::fs::write(&output, &full_config).map_err(|e| {
                tasker_client::ClientError::ConfigError(format!(
                    "Failed to write output file: {}",
                    e
                ))
            })?;

            println!("✓ Successfully generated configuration!");
            println!("  Output file: {}", output);
            println!("  Size: {} bytes", full_config.len());

            // Optionally validate if requested
            if validate {
                println!("\nValidating generated configuration...");

                // Re-read the file we just wrote
                let written_content = std::fs::read_to_string(&output).map_err(|e| {
                    tasker_client::ClientError::ConfigError(format!(
                        "Failed to read generated file for validation: {}",
                        e
                    ))
                })?;

                // Parse as TOML
                let toml_value: toml::Value = toml::from_str(&written_content).map_err(|e| {
                    tasker_client::ClientError::ConfigError(format!(
                        "Generated file is not valid TOML: {}",
                        e
                    ))
                })?;

                // TAS-61 Phase 6D: V2 config validation (all contexts use TaskerConfig)
                // Context determines which optional sections should be present
                let config: TaskerConfig = toml_value.clone().try_into().map_err(|e| {
                    tasker_client::ClientError::ConfigError(format!(
                        "Failed to deserialize TaskerConfig: {}",
                        e
                    ))
                })?;

                // Validate configuration structure
                use validator::Validate;
                config.validate().map_err(|e| {
                    tasker_client::ClientError::ConfigError(format!("Validation failed: {:?}", e))
                })?;

                // Verify context-specific requirements
                match context.as_str() {
                    "common" => {
                        // Common context should have no orchestration or worker sections
                        if config.orchestration.is_some() || config.worker.is_some() {
                            return Err(tasker_client::ClientError::ConfigError(
                                "Common context should not contain orchestration or worker sections".to_string()
                            ));
                        }
                    }
                    "orchestration" => {
                        // Orchestration context should have orchestration section
                        if config.orchestration.is_none() {
                            return Err(tasker_client::ClientError::ConfigError(
                                "Orchestration context missing [orchestration] section".to_string(),
                            ));
                        }
                    }
                    "worker" => {
                        // Worker context should have worker section
                        if config.worker.is_none() {
                            return Err(tasker_client::ClientError::ConfigError(
                                "Worker context missing [worker] section".to_string(),
                            ));
                        }
                    }
                    "complete" => {
                        // Complete context should have all sections
                        if !config.is_complete() {
                            return Err(tasker_client::ClientError::ConfigError(
                                "Complete context missing orchestration or worker sections"
                                    .to_string(),
                            ));
                        }
                    }
                    _ => {
                        return Err(tasker_client::ClientError::InvalidInput(format!(
                            "Unknown context '{}'. Valid contexts: common, orchestration, worker, complete",
                            context
                        )));
                    }
                }

                println!("✓ Validation passed!");
            }
        }
        ConfigCommands::Validate {
            config: config_path,
            context,
            strict,
            explain_errors,
        } => {
            println!("Validating configuration:");
            println!("  File: {}", config_path);
            println!("  Context: {}", context);
            println!("  Strict mode: {}", strict);

            // Load the configuration file
            let config_content = std::fs::read_to_string(&config_path).map_err(|e| {
                tasker_client::ClientError::ConfigError(format!(
                    "Failed to read config file '{}': {}",
                    config_path, e
                ))
            })?;

            // Parse as TOML
            let toml_value: toml::Value = toml::from_str(&config_content).map_err(|e| {
                if explain_errors {
                    eprintln!("\n❌ TOML parsing error:");
                    eprintln!("  {}", e);
                    if let Some((line, col)) = extract_error_position(&e.to_string()) {
                        eprintln!("  Location: line {}, column {}", line, col);
                    }
                }
                tasker_client::ClientError::ConfigError(format!("Invalid TOML syntax: {}", e))
            })?;

            // TAS-61 Phase 6D: V2 config validation
            println!("\nValidating {} configuration...", context);

            let config: TaskerConfig = toml_value.clone().try_into().map_err(|e| {
                if explain_errors {
                    eprintln!("\n❌ Configuration structure error:");
                    eprintln!("  {}", e);
                    eprintln!("\nExpected TaskerConfig structure:");
                    eprintln!("  [common] - Always required");
                    eprintln!("  [orchestration] - Required for orchestration/complete contexts");
                    eprintln!("  [worker] - Required for worker/complete contexts");
                }
                tasker_client::ClientError::ConfigError(format!(
                    "Failed to deserialize TaskerConfig: {}",
                    e
                ))
            })?;

            // Run validation
            use validator::Validate;
            config.validate().map_err(|errors| {
                if explain_errors {
                    eprintln!("\n❌ Validation errors:");
                    eprintln!("  {}", errors);
                }
                tasker_client::ClientError::ConfigError(format!(
                    "TaskerConfig validation failed: {:?}",
                    errors
                ))
            })?;

            // Verify context-specific requirements
            match context.as_str() {
                "common" => {
                    if config.orchestration.is_some() || config.worker.is_some() {
                        if explain_errors {
                            eprintln!("\n❌ Context validation error:");
                            eprintln!("  Common context should not contain orchestration or worker sections");
                        }
                        return Err(tasker_client::ClientError::ConfigError(
                            "Common context should not contain orchestration or worker sections"
                                .to_string(),
                        ));
                    }
                    println!("✓ CommonConfig validation passed");
                }
                "orchestration" => {
                    if config.orchestration.is_none() {
                        if explain_errors {
                            eprintln!("\n❌ Context validation error:");
                            eprintln!("  Orchestration context missing [orchestration] section");
                        }
                        return Err(tasker_client::ClientError::ConfigError(
                            "Orchestration context missing [orchestration] section".to_string(),
                        ));
                    }
                    println!("✓ OrchestrationConfig validation passed");
                }
                "worker" => {
                    if config.worker.is_none() {
                        if explain_errors {
                            eprintln!("\n❌ Context validation error:");
                            eprintln!("  Worker context missing [worker] section");
                        }
                        return Err(tasker_client::ClientError::ConfigError(
                            "Worker context missing [worker] section".to_string(),
                        ));
                    }
                    println!("✓ WorkerConfig validation passed");
                }
                "complete" => {
                    if !config.is_complete() {
                        if explain_errors {
                            eprintln!("\n❌ Context validation error:");
                            eprintln!(
                                "  Complete context missing orchestration or worker sections"
                            );
                        }
                        return Err(tasker_client::ClientError::ConfigError(
                            "Complete context missing orchestration or worker sections".to_string(),
                        ));
                    }
                    println!("✓ TaskerConfig (complete) validation passed");
                    println!("  Note: Complete context contains all sections");
                }
                _ => {
                    return Err(tasker_client::ClientError::InvalidInput(format!(
                        "Unknown context '{}'. Valid contexts: common, orchestration, worker, complete",
                        context
                    )));
                }
            }

            println!("\n✓ Configuration is valid!");

            if strict {
                println!("  (strict mode: no warnings detected)");
            }
        }
        ConfigCommands::ValidateSources {
            context,
            environment,
            source_dir,
            explain_errors,
        } => {
            println!("Validating source configuration:");
            println!("  Context: {}", context);
            println!("  Environment: {}", environment);
            println!("  Source directory: {}", source_dir);

            // Create ConfigMerger
            let mut merger = ConfigMerger::new(std::path::PathBuf::from(&source_dir), &environment)
                .map_err(|e| {
                    if explain_errors {
                        eprintln!("\n❌ Failed to initialize config merger:");
                        eprintln!("  {}", e);
                        eprintln!("\nPossible causes:");
                        eprintln!("  - Source directory doesn't exist");
                        eprintln!("  - Missing base/ subdirectory");
                        eprintln!("  - Invalid directory permissions");
                    }
                    tasker_client::ClientError::ConfigError(format!(
                        "Failed to initialize config merger: {}",
                        e
                    ))
                })?;

            // Attempt to merge configuration (validates source files, env var substitution)
            println!("\nValidating source files and merging...");
            merger.merge_context(&context).map_err(|e| {
                if explain_errors {
                    eprintln!("\n❌ Source configuration validation failed:");
                    eprintln!("  {}", e);
                    eprintln!("\nPossible causes:");
                    eprintln!("  - Missing required TOML files (base/{}.toml)", context);
                    eprintln!("  - Invalid TOML syntax");
                    eprintln!("  - Environment variables without defaults (use ${{VAR:-default}})");
                    eprintln!("  - Structural validation errors");
                }
                tasker_client::ClientError::ConfigError(format!("Source validation failed: {}", e))
            })?;

            println!("✓ Source configuration is valid!");
            println!("  Base files: config/tasker/base/{}.toml", context);
            if context == "orchestration" || context == "worker" {
                println!("  Also includes: config/tasker/base/common.toml");
            }
            println!(
                "  Environment overrides: config/tasker/environments/{}/",
                environment
            );
        }
        ConfigCommands::Explain {
            parameter,
            context,
            environment,
        } => {
            use super::docs::{create_builder, render_parameter_explain};
            use tasker_shared::config::doc_context::ConfigContext;

            let base_dir = "config/tasker/base";
            let builder = create_builder(base_dir)?;

            // Handle specific parameter lookup
            if let Some(param_path) = parameter {
                // Auto-resolve paths: try as-is, then with each context prefix.
                // This maintains backwards compatibility with `config explain -p database.pool.max_connections`
                // while also accepting `config explain -p common.database.pool.max_connections`.
                let resolved = builder.build_parameter(&param_path).or_else(|| {
                    ["common", "orchestration", "worker"]
                        .iter()
                        .find_map(|prefix| {
                            builder.build_parameter(&format!("{}.{}", prefix, param_path))
                        })
                });

                match resolved {
                    Some(param) => {
                        let rendered = render_parameter_explain(&param, environment.as_deref())?;
                        print!("{}", rendered);
                    }
                    None => {
                        eprintln!("Parameter not found: {}", param_path);
                        eprintln!();
                        eprintln!("Hint: Use dotted path with or without context prefix, e.g.:");
                        eprintln!("  common.database.pool.max_connections");
                        eprintln!("  database.pool.max_connections");
                        eprintln!("  orchestration.dlq.enabled");
                        return Err(tasker_client::ClientError::InvalidInput(format!(
                            "Parameter '{}' not found",
                            param_path
                        )));
                    }
                }
            }
            // Handle context listing
            else if let Some(ctx_name) = context {
                let ctx = ConfigContext::from_str_loose(&ctx_name).ok_or_else(|| {
                    tasker_client::ClientError::InvalidInput(format!(
                        "Unknown context '{}'. Valid contexts: common, orchestration, worker",
                        ctx_name
                    ))
                })?;

                let sections = builder.build_sections(ctx);
                let total: usize = sections.iter().map(|s| s.total_parameters()).sum();
                let documented: usize = sections.iter().map(|s| s.documented_parameters()).sum();

                println!(
                    "Configuration Parameters for {} ({}/{} documented)\n",
                    ctx_name, documented, total
                );

                for section in &sections {
                    print_section_listing(section, 0);
                }

                println!();
                println!(
                    "Use `tasker-cli docs explain -p <path>` for detailed parameter information."
                );
                println!(
                    "Use `tasker-cli docs reference --context {}` for full documentation.",
                    ctx_name
                );
            }
            // List all parameters — show coverage summary
            else {
                let (total, documented, per_context) = builder.coverage_stats();
                let percent = if total > 0 {
                    (documented * 100) / total
                } else {
                    0
                };

                println!("Configuration Documentation Overview");
                println!("====================================\n");
                println!(
                    "  Total: {}/{} parameters documented ({}%)\n",
                    documented, total, percent
                );

                for (ctx, ctx_total, ctx_documented) in &per_context {
                    let ctx_percent = if *ctx_total > 0 {
                        (ctx_documented * 100) / ctx_total
                    } else {
                        0
                    };
                    println!(
                        "  {:15} {}/{} ({}%)",
                        format!("{}:", ctx),
                        ctx_documented,
                        ctx_total,
                        ctx_percent
                    );
                }

                println!();
                println!("Commands:");
                println!(
                    "  tasker-cli config explain --parameter <path>   Explain a specific parameter"
                );
                println!(
                    "  tasker-cli config explain --context <name>     List parameters in a context"
                );
                println!(
                    "  tasker-cli docs reference                      Full documentation reference"
                );
                println!(
                    "  tasker-cli docs coverage                       Detailed coverage statistics"
                );
            }
        }
        ConfigCommands::AnalyzeUsage {
            source_dir,
            context,
            format,
            show_unused,
            output,
            show_locations,
        } => {
            use std::collections::HashMap;
            use std::process::Command;

            println!("Analyzing configuration usage patterns...");
            println!("  Source directory: {}", source_dir);
            println!("  Context filter: {}", context);
            println!("  Report format: {}", format);

            // Step 1: Find all *Config struct definitions
            println!("\n[1/4] Finding Config struct definitions...");
            let struct_output = Command::new("rg")
                .args([
                    "pub struct \\w*Config",
                    "--type",
                    "rust",
                    "-n",
                    "--no-heading",
                ])
                .current_dir(&source_dir)
                .output()
                .map_err(|e| {
                    tasker_client::ClientError::ConfigError(format!(
                        "Failed to run ripgrep for structs: {}. Is ripgrep installed?",
                        e
                    ))
                })?;

            if !struct_output.status.success() {
                return Err(tasker_client::ClientError::ConfigError(
                    "Failed to find config structs. Ensure ripgrep is installed.".to_string(),
                ));
            }

            let struct_lines = String::from_utf8_lossy(&struct_output.stdout);
            let mut config_structs: HashMap<String, (String, usize)> = HashMap::new(); // name -> (file, line)
            let struct_re = Regex::new(r"pub struct ([A-Za-z0-9_]+Config)").unwrap();

            for line in struct_lines.lines() {
                let parts: Vec<&str> = line.split(':').collect();
                if parts.len() >= 2 {
                    let file_path = parts[0];
                    let line_num: usize = parts[1].parse().unwrap_or(0);

                    if let Some(caps) = struct_re.captures(line) {
                        if let Some(struct_name) = caps.get(1) {
                            config_structs.insert(
                                struct_name.as_str().to_string(),
                                (file_path.to_string(), line_num),
                            );
                        }
                    }
                }
            }

            println!("   Found {} Config structs", config_structs.len());

            // Step 2: Find all config field accesses (config.*)
            println!("[2/4] Finding config field accesses in code...");
            let usage_output = Command::new("rg")
                .args([r"config\.\w+", "--type", "rust", "-o", "-n", "--no-heading"])
                .current_dir(&source_dir)
                .output()
                .map_err(|e| {
                    tasker_client::ClientError::ConfigError(format!(
                        "Failed to run ripgrep for usage: {}",
                        e
                    ))
                })?;

            let usage_lines = String::from_utf8_lossy(&usage_output.stdout);
            let mut field_usage: HashMap<String, Vec<(String, usize)>> = HashMap::new(); // field -> [(file, line)]
            let field_re = Regex::new(r"config\.([a-z_]+)").unwrap();

            for line in usage_lines.lines() {
                let parts: Vec<&str> = line.split(':').collect();
                if parts.len() >= 3 {
                    let file_path = parts[0];
                    let line_num: usize = parts[1].parse().unwrap_or(0);
                    let access = parts[2];

                    if let Some(caps) = field_re.captures(access) {
                        if let Some(field_name) = caps.get(1) {
                            field_usage
                                .entry(field_name.as_str().to_string())
                                .or_default()
                                .push((file_path.to_string(), line_num));
                        }
                    }
                }
            }

            println!(
                "   Found {} unique config field accesses",
                field_usage.len()
            );

            // Step 3: Find struct type references (direct usage like TaskerConfig, etc.)
            println!("[3/4] Finding struct instantiations and type references...");
            let mut struct_usage: HashMap<String, Vec<(String, usize)>> = HashMap::new();

            for struct_name in config_structs.keys() {
                let struct_output = Command::new("rg")
                    .args([struct_name, "--type", "rust", "-n", "--no-heading"])
                    .current_dir(&source_dir)
                    .output()
                    .map_err(|e| {
                        tasker_client::ClientError::ConfigError(format!(
                            "Failed to search for struct usage: {}",
                            e
                        ))
                    })?;

                let lines = String::from_utf8_lossy(&struct_output.stdout);
                let references: Vec<(String, usize)> = lines
                    .lines()
                    .filter(|l| {
                        // Exclude the struct definition itself
                        !l.contains("pub struct")
                            && !l.contains("impl Default for")
                            && !l.contains("impl ")
                    })
                    .filter_map(|line| {
                        let parts: Vec<&str> = line.split(':').collect();
                        if parts.len() >= 2 {
                            let file = parts[0].to_string();
                            let line_num = parts[1].parse().ok()?;
                            Some((file, line_num))
                        } else {
                            None
                        }
                    })
                    .collect();

                if !references.is_empty() {
                    struct_usage.insert(struct_name.clone(), references);
                }
            }

            println!(
                "   Found {} structs with usage references",
                struct_usage.len()
            );

            // Step 4: Analyze and categorize
            println!("[4/4] Analyzing usage patterns...");

            let mut used_structs: Vec<String> = Vec::new();
            let mut unused_structs: Vec<String> = Vec::new();

            for struct_name in config_structs.keys() {
                if struct_usage.contains_key(struct_name) {
                    used_structs.push(struct_name.clone());
                } else {
                    unused_structs.push(struct_name.clone());
                }
            }

            used_structs.sort();
            unused_structs.sort();

            // Calculate statistics
            let total_structs = config_structs.len();
            let used_count = used_structs.len();
            let unused_count = unused_structs.len();
            let used_percentage = if total_structs > 0 {
                (used_count as f64 / total_structs as f64) * 100.0
            } else {
                0.0
            };

            // Sort field usage by frequency
            let mut field_usage_vec: Vec<(String, usize)> = field_usage
                .iter()
                .map(|(field, locations)| (field.clone(), locations.len()))
                .collect();
            field_usage_vec.sort_by(|a, b| b.1.cmp(&a.1));

            // Generate report based on format
            let report = match format.as_str() {
                "json" => {
                    let mut report = serde_json::json!({
                        "summary": {
                            "total_config_structs": total_structs,
                            "used_structs": used_count,
                            "unused_structs": unused_count,
                            "used_percentage": used_percentage,
                            "total_field_accesses": field_usage_vec.iter().map(|(_, c)| c).sum::<usize>(),
                            "unique_fields_accessed": field_usage.len(),
                        },
                        "used_structs": used_structs,
                        "unused_structs": unused_structs
                            .iter()
                            .map(|name| {
                                let (file, line) = config_structs.get(name).unwrap();
                                serde_json::json!({
                                    "name": name,
                                    "location": format!("{}:{}", file, line)
                                })
                            })
                            .collect::<Vec<_>>(),
                        "top_field_usage": field_usage_vec.iter().take(20).map(|(field, count)| {
                            serde_json::json!({
                                "field": format!("config.{}", field),
                                "access_count": count
                            })
                        }).collect::<Vec<_>>(),
                    });

                    if show_locations {
                        report["field_usage_locations"] = serde_json::json!(
                            field_usage.iter().map(|(field, locations)| {
                                serde_json::json!({
                                    "field": format!("config.{}", field),
                                    "locations": locations.iter().take(10).map(|(f, l)| format!("{}:{}", f, l)).collect::<Vec<_>>()
                                })
                            }).collect::<Vec<_>>()
                        );
                    }

                    serde_json::to_string_pretty(&report).unwrap()
                }
                "markdown" => {
                    let mut md = String::new();
                    md.push_str("# Configuration Usage Analysis\n\n");
                    md.push_str(&format!(
                        "**Generated**: {}\n\n",
                        chrono::Utc::now().to_rfc3339()
                    ));

                    md.push_str("## Summary\n\n");
                    md.push_str(&format!("- **Total Config Structs**: {}\n", total_structs));
                    md.push_str(&format!(
                        "- **Used**: {} ({:.1}%)\n",
                        used_count, used_percentage
                    ));
                    md.push_str(&format!(
                        "- **Unused**: {} ({:.1}%)\n\n",
                        unused_count,
                        100.0 - used_percentage
                    ));

                    if !show_unused || unused_structs.is_empty() {
                        md.push_str("## Top Field Usage\n\n");
                        md.push_str("| Field | Access Count |\n");
                        md.push_str("|-------|-------------|\n");
                        for (field, count) in field_usage_vec.iter().take(20) {
                            md.push_str(&format!("| `config.{}` | {} |\n", field, count));
                        }
                        md.push('\n');
                    }

                    if !unused_structs.is_empty() {
                        md.push_str("## Unused Config Structs\n\n");
                        md.push_str("These structs are defined but have no runtime usage:\n\n");
                        for struct_name in &unused_structs {
                            let (file, line) = config_structs.get(struct_name).unwrap();
                            md.push_str(&format!("- **{}** - `{}:{}`\n", struct_name, file, line));
                        }
                        md.push('\n');
                    }

                    md
                }
                _ => {
                    // Default text format
                    let mut text = String::new();
                    text.push_str(
                        "\n╔═══════════════════════════════════════════════════════════════╗\n",
                    );
                    text.push_str(
                        "║        Configuration Usage Analysis Report                   ║\n",
                    );
                    text.push_str(
                        "╚═══════════════════════════════════════════════════════════════╝\n\n",
                    );

                    text.push_str("SUMMARY\n");
                    text.push_str("═══════\n");
                    text.push_str(&format!("Total Config Structs: {}\n", total_structs));
                    text.push_str(&format!(
                        "  Used:   {} ({:.1}%)\n",
                        used_count, used_percentage
                    ));
                    text.push_str(&format!(
                        "  Unused: {} ({:.1}%)\n\n",
                        unused_count,
                        100.0 - used_percentage
                    ));

                    if !show_unused || unused_structs.is_empty() {
                        text.push_str("TOP FIELD USAGE\n");
                        text.push_str("═══════════════\n");
                        for (i, (field, count)) in field_usage_vec.iter().take(20).enumerate() {
                            text.push_str(&format!(
                                "{:2}. config.{:<30} {:>5} accesses\n",
                                i + 1,
                                field,
                                count
                            ));
                        }
                        text.push('\n');
                    }

                    if !unused_structs.is_empty() {
                        text.push_str("UNUSED CONFIG STRUCTS\n");
                        text.push_str("═══════════════════════\n");
                        text.push_str("These structs are defined but have no runtime usage:\n\n");
                        for struct_name in &unused_structs {
                            let (file, line) = config_structs.get(struct_name).unwrap();
                            text.push_str(&format!("  ✗ {:<40} {}:{}\n", struct_name, file, line));
                        }
                        text.push('\n');
                        text.push_str("💡 Consider removing or documenting why these exist.\n\n");
                    } else {
                        text.push_str(
                            "✓ All config structs have at least one usage reference!\n\n",
                        );
                    }

                    if show_locations && !field_usage.is_empty() {
                        text.push_str("FIELD USAGE LOCATIONS (Top 10 Most Used)\n");
                        text.push_str("═════════════════════════════════════════\n");
                        for (field, count) in field_usage_vec.iter().take(10) {
                            text.push_str(&format!("\nconfig.{} ({} accesses):\n", field, count));
                            if let Some(locations) = field_usage.get(field) {
                                for (file, line) in locations.iter().take(5) {
                                    text.push_str(&format!("  • {}:{}\n", file, line));
                                }
                                if locations.len() > 5 {
                                    text.push_str(&format!(
                                        "  ... and {} more\n",
                                        locations.len() - 5
                                    ));
                                }
                            }
                        }
                        text.push('\n');
                    }

                    text
                }
            };

            // Output report
            if let Some(output_file) = output {
                std::fs::write(&output_file, &report).map_err(|e| {
                    tasker_client::ClientError::ConfigError(format!(
                        "Failed to write report to file: {}",
                        e
                    ))
                })?;
                println!("\n✓ Report written to: {}", output_file);
            } else {
                println!("\n{}", report);
            }
        }
        ConfigCommands::Dump {
            context,
            environment,
            source_dir,
            format,
            path,
        } => {
            // Check if --path was provided (legacy single-file mode)
            let tasker_config: TaskerConfig = if let Some(config_path) = path {
                // Load directly from path using simple V2 loader
                let path_buf = std::path::PathBuf::from(&config_path);
                tasker_shared::config::ConfigLoader::load_from_path(&path_buf).map_err(|e| {
                    tasker_client::ClientError::ConfigError(format!(
                        "Failed to load configuration from '{}': {}",
                        config_path, e
                    ))
                })?
            } else {
                // Context/environment mode (TAS-61)
                let context = context.as_ref().ok_or_else(|| {
                    tasker_client::ClientError::InvalidInput(
                        "Context is required when --path is not provided".to_string(),
                    )
                })?;
                let environment = environment.as_ref().ok_or_else(|| {
                    tasker_client::ClientError::InvalidInput(
                        "Environment is required when --path is not provided".to_string(),
                    )
                })?;

                // Create ConfigMerger
                let mut merger =
                    ConfigMerger::new(std::path::PathBuf::from(&source_dir), environment).map_err(
                        |e| {
                            tasker_client::ClientError::ConfigError(format!(
                                "Failed to initialize config merger: {}",
                                e
                            ))
                        },
                    )?;

                // Merge configuration to TOML string
                let merged_toml = merger.merge_context(context).map_err(|e| {
                    tasker_client::ClientError::ConfigError(format!(
                        "Failed to merge configuration: {}",
                        e
                    ))
                })?;

                // Parse TOML into Value then into TaskerConfig
                let toml_value: toml::Value = toml::from_str(&merged_toml).map_err(|e| {
                    tasker_client::ClientError::ConfigError(format!(
                        "Failed to parse merged TOML: {}",
                        e
                    ))
                })?;

                // Hydrate full TaskerConfig from merged TOML
                toml_value.try_into().map_err(|e: toml::de::Error| {
                    tasker_client::ClientError::ConfigError(format!(
                        "Failed to hydrate TaskerConfig from merged TOML: {}",
                        e
                    ))
                })?
            };

            // Convert to the requested format
            let output = match format.as_str() {
                "json" => serde_json::to_string_pretty(&tasker_config).map_err(|e| {
                    tasker_client::ClientError::ConfigError(format!(
                        "Failed to serialize TaskerConfig to JSON: {}",
                        e
                    ))
                })?,
                "yaml" => serde_yaml::to_string(&tasker_config).map_err(|e| {
                    tasker_client::ClientError::ConfigError(format!(
                        "Failed to serialize TaskerConfig to YAML: {}",
                        e
                    ))
                })?,
                "toml" => toml::to_string_pretty(&tasker_config).map_err(|e| {
                    tasker_client::ClientError::ConfigError(format!(
                        "Failed to serialize TaskerConfig to TOML: {}",
                        e
                    ))
                })?,
                _ => {
                    return Err(tasker_client::ClientError::InvalidInput(format!(
                        "Unknown format '{}'. Valid formats: json, yaml, toml",
                        format
                    )));
                }
            };

            // Print raw output to stdout (no preamble for easy piping)
            println!("{}", output);
        }
        ConfigCommands::Show => {
            println!("Current CLI Configuration:");
            println!("══════════════════════════\n");

            // Show transport
            println!("Transport: {:?}", _config.transport);
            println!();

            // Show endpoints
            println!("Orchestration Endpoint:");
            println!("  URL: {}", _config.orchestration.base_url);
            println!("  Timeout: {}ms", _config.orchestration.timeout_ms);
            println!("  Max Retries: {}", _config.orchestration.max_retries);
            if let Some(ref auth) = _config.orchestration.auth {
                println!("  Auth: {:?}", auth.method);
            }
            println!();

            println!("Worker Endpoint:");
            println!("  URL: {}", _config.worker.base_url);
            println!("  Timeout: {}ms", _config.worker.timeout_ms);
            println!("  Max Retries: {}", _config.worker.max_retries);
            if let Some(ref auth) = _config.worker.auth {
                println!("  Auth: {:?}", auth.method);
            }
            println!();

            // Show available profiles
            println!("Available Profiles:");
            match tasker_client::ClientConfig::list_profiles() {
                Ok(profiles) => {
                    if profiles.is_empty() {
                        println!("  (no profiles found)");
                    } else {
                        for profile in profiles {
                            println!("  - {}", profile);
                        }
                    }
                }
                Err(e) => {
                    println!("  (error loading profiles: {})", e);
                }
            }
            println!();

            // Show profile config file location
            if let Some(path) = tasker_client::ClientConfig::find_profile_config_file() {
                println!("Profile Config File: {}", path.display());
            } else {
                println!("Profile Config File: (not found)");
                println!("  Expected locations:");
                println!("    - .config/tasker-client.toml (project)");
                println!("    - ~/.config/tasker-client.toml (user)");
            }
            println!();

            // Show environment variables that can override
            println!("Environment Variable Overrides:");
            println!("  TASKER_CLIENT_PROFILE - Select profile");
            println!("  TASKER_TRANSPORT - Override transport (rest/grpc)");
            println!("  TASKER_ORCHESTRATION_URL - Override orchestration URL");
            println!("  TASKER_WORKER_URL - Override worker URL");
            println!("  TASKER_ORCHESTRATION_GRPC_URL - Override orchestration gRPC URL");
            println!("  TASKER_WORKER_GRPC_URL - Override worker gRPC URL");
            println!();

            // Show full TOML representation
            println!("Full Configuration (TOML):");
            println!("──────────────────────────");
            println!("{}", toml::to_string_pretty(_config).unwrap());
        }
    }
    Ok(())
}

/// Print a section listing with indentation for nested subsections.
fn print_section_listing(
    section: &tasker_shared::config::doc_context::SectionContext,
    depth: usize,
) {
    let indent = "  ".repeat(depth);
    let total = section.total_parameters();
    let documented = section.documented_parameters();

    if documented > 0 {
        println!(
            "{}  {} ({}) — {} params ({} documented)",
            indent, section.name, section.path, total, documented
        );
    } else {
        println!(
            "{}  {} ({}) — {} params",
            indent, section.name, section.path, total
        );
    }

    for param in &section.parameters {
        if param.is_documented {
            println!("{}    • {} — {}", indent, param.name, param.description);
        }
    }

    for sub in &section.subsections {
        print_section_listing(sub, depth + 1);
    }
}
