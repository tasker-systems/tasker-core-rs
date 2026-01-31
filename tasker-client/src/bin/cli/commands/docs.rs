//! Documentation generation command handlers (TAS-175)
//!
//! CLI commands for generating configuration documentation using Askama templates.

use std::path::PathBuf;
use tasker_client::ClientResult;
use tasker_shared::config::doc_context::ConfigContext;
use tasker_shared::config::doc_context_builder::DocContextBuilder;

#[cfg(feature = "docs-gen")]
use askama::Template;
#[cfg(feature = "docs-gen")]
use tasker_client::docs::{
    AnnotatedConfigTemplate, ConfigReferenceTemplate, DocIndexTemplate, ParameterExplainTemplate,
    SectionDetailTemplate,
};

use crate::DocsCommands;

pub async fn handle_docs_command(cmd: DocsCommands) -> ClientResult<()> {
    match cmd {
        DocsCommands::Reference {
            context,
            format: _format,
            output,
            base_dir,
        } => {
            handle_reference(&context, output.as_deref(), &base_dir).await
        }
        DocsCommands::Annotated {
            context,
            environment,
            output,
            base_dir,
        } => {
            handle_annotated(&context, &environment, output.as_deref(), &base_dir).await
        }
        DocsCommands::Section {
            path,
            environment,
            output,
            base_dir,
        } => {
            handle_section(&path, environment.as_deref(), output.as_deref(), &base_dir).await
        }
        DocsCommands::Coverage { base_dir } => handle_coverage(&base_dir).await,
        DocsCommands::Explain {
            parameter,
            environment,
            base_dir,
        } => handle_explain(&parameter, environment.as_deref(), &base_dir).await,
        DocsCommands::Index { output, base_dir } => {
            handle_index(output.as_deref(), &base_dir).await
        }
    }
}

async fn handle_reference(
    context: &str,
    output: Option<&str>,
    base_dir: &str,
) -> ClientResult<()> {
    let builder = create_builder(base_dir)?;
    let timestamp = chrono::Utc::now().to_rfc3339();

    let rendered = if context == "all" {
        let mut parts = Vec::new();
        for ctx in &[
            ConfigContext::Common,
            ConfigContext::Orchestration,
            ConfigContext::Worker,
        ] {
            let sections = builder.build_sections(*ctx);
            let total: usize = sections.iter().map(|s| s.total_parameters()).sum();
            let documented: usize = sections.iter().map(|s| s.documented_parameters()).sum();

            let part = render_reference(
                &ctx.to_string(),
                &sections,
                total,
                documented,
                &timestamp,
            )?;
            parts.push(part);
        }
        parts.join("\n---\n\n")
    } else {
        let ctx = parse_context(context)?;
        let sections = builder.build_sections(ctx);
        let total: usize = sections.iter().map(|s| s.total_parameters()).sum();
        let documented: usize = sections.iter().map(|s| s.documented_parameters()).sum();

        render_reference(&ctx.to_string(), &sections, total, documented, &timestamp)?
    };

    write_output(&rendered, output)?;
    Ok(())
}

async fn handle_annotated(
    context: &str,
    environment: &str,
    output: Option<&str>,
    base_dir: &str,
) -> ClientResult<()> {
    let builder = create_builder(base_dir)?;

    let rendered = if context == "all" || context == "complete" {
        let mut parts = Vec::new();
        for ctx in &[
            ConfigContext::Common,
            ConfigContext::Orchestration,
            ConfigContext::Worker,
        ] {
            let sections = builder.build_sections(*ctx);
            let part = render_annotated(&ctx.to_string(), environment, &sections)?;
            parts.push(part);
        }
        parts.join("\n")
    } else {
        let ctx = parse_context(context)?;
        let sections = builder.build_sections(ctx);
        render_annotated(&ctx.to_string(), environment, &sections)?
    };

    write_output(&rendered, output)?;
    Ok(())
}

async fn handle_section(
    path: &str,
    environment: Option<&str>,
    output: Option<&str>,
    base_dir: &str,
) -> ClientResult<()> {
    let builder = create_builder(base_dir)?;

    // Determine context from path prefix
    let ctx = if path.starts_with("common.") {
        ConfigContext::Common
    } else if path.starts_with("orchestration.") {
        ConfigContext::Orchestration
    } else if path.starts_with("worker.") {
        ConfigContext::Worker
    } else {
        return Err(tasker_client::ClientError::ConfigError(format!(
            "Path must start with 'common.', 'orchestration.', or 'worker.': {}",
            path
        )));
    };

    let sections = builder.build_sections(ctx);

    // Find the matching section
    let section = find_section_by_path(&sections, path);

    match section {
        Some(section) => {
            let rendered = render_section_detail(section, &ctx.to_string(), environment)?;
            write_output(&rendered, output)?;
        }
        None => {
            eprintln!("Section not found: {}", path);
            eprintln!("Available sections:");
            for s in &sections {
                eprintln!("  {}", s.path);
                for sub in &s.subsections {
                    eprintln!("    {}", sub.path);
                }
            }
        }
    }

    Ok(())
}

async fn handle_coverage(base_dir: &str) -> ClientResult<()> {
    let builder = create_builder(base_dir)?;
    let (total, documented, per_context) = builder.coverage_stats();

    let percent = if total > 0 {
        (documented * 100) / total
    } else {
        0
    };

    println!("Configuration Documentation Coverage");
    println!("====================================");
    println!();
    println!(
        "  Total:      {}/{} parameters ({}%)",
        documented, total, percent
    );
    println!();

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
    println!(
        "Run `tasker-cli docs reference` to generate full documentation."
    );

    Ok(())
}

async fn handle_explain(
    parameter: &str,
    environment: Option<&str>,
    base_dir: &str,
) -> ClientResult<()> {
    let builder = create_builder(base_dir)?;

    match builder.build_parameter(parameter) {
        Some(param) => {
            let rendered = render_parameter_explain(&param, environment)?;
            print!("{}", rendered);
        }
        None => {
            eprintln!("Parameter not found: {}", parameter);
            eprintln!();
            eprintln!("Hint: Use dotted path including context prefix, e.g.:");
            eprintln!("  common.database.pool.max_connections");
            eprintln!("  orchestration.dlq.enabled");
            eprintln!("  worker.step_processing.max_retries");
        }
    }

    Ok(())
}

async fn handle_index(output: Option<&str>, base_dir: &str) -> ClientResult<()> {
    let builder = create_builder(base_dir)?;

    let common_sections = builder.build_sections(ConfigContext::Common);
    let orchestration_sections = builder.build_sections(ConfigContext::Orchestration);
    let worker_sections = builder.build_sections(ConfigContext::Worker);

    let (total, documented, _) = builder.coverage_stats();
    let coverage_percent = if total > 0 {
        (documented * 100) / total
    } else {
        0
    };

    let rendered = render_index(
        &common_sections,
        &orchestration_sections,
        &worker_sections,
        total,
        documented,
        coverage_percent,
    )?;

    write_output(&rendered, output)?;
    Ok(())
}

// ── Rendering helpers ────────────────────────────────────────────────────
// pub(crate) so `config explain` can delegate to the same Askama rendering.

#[cfg(feature = "docs-gen")]
pub(crate) fn render_reference(
    context_name: &str,
    sections: &[tasker_shared::config::doc_context::SectionContext],
    total_parameters: usize,
    documented_parameters: usize,
    generation_timestamp: &str,
) -> ClientResult<String> {
    let template = ConfigReferenceTemplate {
        context_name,
        sections,
        total_parameters,
        documented_parameters,
        generation_timestamp,
    };
    template.render().map_err(|e| {
        tasker_client::ClientError::ConfigError(format!("Template rendering failed: {}", e))
    })
}

#[cfg(not(feature = "docs-gen"))]
pub(crate) fn render_reference(
    context_name: &str,
    sections: &[tasker_shared::config::doc_context::SectionContext],
    total_parameters: usize,
    documented_parameters: usize,
    _generation_timestamp: &str,
) -> ClientResult<String> {
    Ok(format!(
        "# Configuration Reference: {}\n\n{}/{} parameters documented\n\n{} sections",
        context_name,
        documented_parameters,
        total_parameters,
        sections.len()
    ))
}

#[cfg(feature = "docs-gen")]
pub(crate) fn render_annotated(
    context_name: &str,
    environment: &str,
    sections: &[tasker_shared::config::doc_context::SectionContext],
) -> ClientResult<String> {
    let template = AnnotatedConfigTemplate {
        context_name,
        environment,
        sections,
    };
    template.render().map_err(|e| {
        tasker_client::ClientError::ConfigError(format!("Template rendering failed: {}", e))
    })
}

#[cfg(not(feature = "docs-gen"))]
pub(crate) fn render_annotated(
    context_name: &str,
    environment: &str,
    sections: &[tasker_shared::config::doc_context::SectionContext],
) -> ClientResult<String> {
    Ok(format!(
        "# Annotated config: {} (env: {})\n# {} sections",
        context_name,
        environment,
        sections.len()
    ))
}

#[cfg(feature = "docs-gen")]
pub(crate) fn render_section_detail(
    section: &tasker_shared::config::doc_context::SectionContext,
    context_name: &str,
    environment: Option<&str>,
) -> ClientResult<String> {
    let template = SectionDetailTemplate {
        section,
        context_name,
        environment,
    };
    template.render().map_err(|e| {
        tasker_client::ClientError::ConfigError(format!("Template rendering failed: {}", e))
    })
}

#[cfg(not(feature = "docs-gen"))]
pub(crate) fn render_section_detail(
    section: &tasker_shared::config::doc_context::SectionContext,
    context_name: &str,
    _environment: Option<&str>,
) -> ClientResult<String> {
    Ok(format!(
        "# Section: {} ({})\n{} parameters",
        section.name,
        context_name,
        section.parameters.len()
    ))
}

#[cfg(feature = "docs-gen")]
pub(crate) fn render_parameter_explain(
    parameter: &tasker_shared::config::doc_context::ParameterContext,
    environment: Option<&str>,
) -> ClientResult<String> {
    let template = ParameterExplainTemplate {
        parameter,
        environment,
    };
    template.render().map_err(|e| {
        tasker_client::ClientError::ConfigError(format!("Template rendering failed: {}", e))
    })
}

#[cfg(not(feature = "docs-gen"))]
pub(crate) fn render_parameter_explain(
    parameter: &tasker_shared::config::doc_context::ParameterContext,
    _environment: Option<&str>,
) -> ClientResult<String> {
    Ok(format!(
        "Parameter: {}\n  Type: {}\n  Default: {}\n  {}",
        parameter.path, parameter.rust_type, parameter.default_value, parameter.description
    ))
}

#[cfg(feature = "docs-gen")]
pub(crate) fn render_index(
    common_sections: &[tasker_shared::config::doc_context::SectionContext],
    orchestration_sections: &[tasker_shared::config::doc_context::SectionContext],
    worker_sections: &[tasker_shared::config::doc_context::SectionContext],
    total_parameters: usize,
    documented_parameters: usize,
    coverage_percent: usize,
) -> ClientResult<String> {
    let template = DocIndexTemplate {
        common_sections,
        orchestration_sections,
        worker_sections,
        total_parameters,
        documented_parameters,
        coverage_percent,
    };
    template.render().map_err(|e| {
        tasker_client::ClientError::ConfigError(format!("Template rendering failed: {}", e))
    })
}

#[cfg(not(feature = "docs-gen"))]
pub(crate) fn render_index(
    common_sections: &[tasker_shared::config::doc_context::SectionContext],
    orchestration_sections: &[tasker_shared::config::doc_context::SectionContext],
    worker_sections: &[tasker_shared::config::doc_context::SectionContext],
    total_parameters: usize,
    documented_parameters: usize,
    _coverage_percent: usize,
) -> ClientResult<String> {
    Ok(format!(
        "# Documentation Index\n\n{}/{} parameters documented\n\nCommon: {} sections\nOrchestration: {} sections\nWorker: {} sections",
        documented_parameters,
        total_parameters,
        common_sections.len(),
        orchestration_sections.len(),
        worker_sections.len()
    ))
}

// ── Utility helpers ──────────────────────────────────────────────────────

pub(crate) fn create_builder(base_dir: &str) -> ClientResult<DocContextBuilder> {
    DocContextBuilder::new(PathBuf::from(base_dir)).map_err(|e| {
        tasker_client::ClientError::ConfigError(format!(
            "Failed to load configuration documentation from '{}': {}",
            base_dir, e
        ))
    })
}

pub(crate) fn parse_context(context: &str) -> ClientResult<ConfigContext> {
    ConfigContext::from_str_loose(context).ok_or_else(|| {
        tasker_client::ClientError::ConfigError(format!(
            "Unknown context '{}'. Expected: common, orchestration, worker",
            context
        ))
    })
}

pub(crate) fn write_output(content: &str, output: Option<&str>) -> ClientResult<()> {
    if let Some(path) = output {
        // Ensure parent directory exists
        if let Some(parent) = std::path::Path::new(path).parent() {
            std::fs::create_dir_all(parent).map_err(|e| {
                tasker_client::ClientError::ConfigError(format!(
                    "Failed to create output directory: {}",
                    e
                ))
            })?;
        }
        std::fs::write(path, content).map_err(|e| {
            tasker_client::ClientError::ConfigError(format!(
                "Failed to write to '{}': {}",
                path, e
            ))
        })?;
        println!("Written to: {}", path);
    } else {
        println!("{}", content);
    }
    Ok(())
}

pub(crate) fn find_section_by_path<'a>(
    sections: &'a [tasker_shared::config::doc_context::SectionContext],
    path: &str,
) -> Option<&'a tasker_shared::config::doc_context::SectionContext> {
    for section in sections {
        if section.path == path {
            return Some(section);
        }
        if let Some(found) = find_section_by_path(&section.subsections, path) {
            return Some(found);
        }
    }
    None
}
