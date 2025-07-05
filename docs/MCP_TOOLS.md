# MCP Tools Documentation

This document outlines the Model Context Protocol (MCP) servers and tools available for the tasker-core-rs project development workflow.

## Currently Configured MCP Servers

The following MCP servers are currently configured and active:

### PostgreSQL MCP Server (`crystaldba/postgres-mcp`)
**Container**: `docker run -i --rm -e DATABASE_URI=postgresql://tasker:tasker@localhost:5432/tasker_rust_development crystaldba/postgres-mcp --access-mode=unrestricted`
**Purpose**: Database operations, performance analysis, and migration management
**Available Tools**:
- Schema and object management (`list_schemas`, `list_objects`, `get_object_details`)
- Query execution and performance analysis (`execute_sql`, `explain_query`)
- Database health monitoring (`analyze_db_health`)
- Index optimization recommendations (`analyze_workload_indexes`, `analyze_query_indexes`)
- Top query analysis (`get_top_queries`)
- Connection and vacuum monitoring

### GitHub MCP Server (`github/github-mcp-server`)
**Container**: `docker run -i --rm -e GITHUB_PERSONAL_ACCESS_TOKEN=${GITHUB_PERSONAL_ACCESS_TOKEN} ghcr.io/github/github-mcp-server`
**Purpose**: Repository operations, PR management, and CI/CD integration
**Available Tools**:
- Issue management (create, update, comment, assign)
- Pull request operations (create, review, merge, update)
- Repository management (create, fork, branch operations)
- Workflow automation (run, cancel, rerun workflows)
- Code scanning and security alerts
- Notification management
- Search capabilities across repositories, issues, and PRs

### Memory MCP Server (`@modelcontextprotocol/server-memory`)
**Runtime**: `npx @modelcontextprotocol/server-memory`
**Purpose**: Knowledge graph and context management
**Available Tools**:
- Entity creation and management (`create_entities`, `delete_entities`)
- Relationship mapping (`create_relations`, `delete_relations`)
- Observation tracking (`add_observations`, `delete_observations`)
- Knowledge graph querying and search (`read_graph`, `search_nodes`, `open_nodes`)

### Docker MCP Server (`mcp-server-docker`)
**Runtime**: `mcp-server-docker`
**Purpose**: Containerized testing and deployment automation
**Available Tools**:
- Container command execution (`run_command`)
- Docker Compose service management
- Development environment orchestration

### OpenAPI MCP Server (`mcp-openapi-schema-explorer`)
**Configuration**: `mcp-openapi-schema-explorer docs/tasker_engine_openapi_30_v1.yaml`
**Purpose**: API schema exploration and documentation
**Available Tools**:
- OpenAPI schema analysis
- API endpoint documentation
- Schema validation and exploration

### Context7 MCP Server (`@upstash/context7-mcp`)
**Runtime**: `npx -y @upstash/context7-mcp`
**Purpose**: Enhanced development context and library documentation
**Available Tools**:
- Library ID resolution (`resolve-library-id`)
- Up-to-date documentation retrieval (`get-library-docs`)
- Framework-specific guidance

## Development Integration Patterns

### Database Development
- Use PostgreSQL MCP for schema analysis and query optimization
- Leverage health monitoring for performance tuning
- Utilize index analysis for query performance improvements

### CI/CD Workflow
- GitHub MCP handles PR creation, review, and merge operations
- Workflow automation for testing and deployment
- Integration with repository notifications and alerts

### Containerized Development
- Docker MCP enables consistent development environments
- Service management for multi-container applications
- Development workflow automation

### Code Quality and Analysis
- IDE MCP provides real-time diagnostics and feedback
- Memory MCP tracks development context and decisions
- Context7 MCP offers framework-specific best practices

## Usage Examples

### Database Operations
```bash
# Analyze database health
mcp__postgres__analyze_db_health

# Get top performing queries
mcp__postgres__get_top_queries

# Explain query execution plan
mcp__postgres__explain_query
```

### GitHub Operations
```bash
# Create a pull request
mcp__github__create_pull_request

# Review PR status
mcp__github__get_pull_request_status

# Merge approved PR
mcp__github__merge_pull_request
```

### Docker Operations
```bash
# Run command in container
mcp__docker__run_command

# Execute development tasks
mcp__docker__run_command --service laravel_app
```

## Configuration

MCP servers are configured through Claude Code's MCP integration system. The servers provide enhanced development capabilities including:

- **Enhanced database development**: Real-time query analysis and optimization
- **Automated dependency management**: Package security and compatibility checks
- **Streamlined CI/CD workflows**: Automated testing and deployment processes
- **Real-time development guidance**: Framework-specific best practices and documentation

## Benefits

- **Performance**: Leverage MCP servers for computationally intensive operations
- **Automation**: Reduce manual workflow overhead through automated processes
- **Quality**: Enhanced code quality through real-time analysis and feedback
- **Integration**: Seamless integration with existing development tools and workflows

## Troubleshooting

If MCP tools are not responding:
1. Verify MCP server configuration
2. Check network connectivity
3. Review MCP server logs
4. Restart MCP services if necessary

For specific server issues, consult the individual MCP server documentation and support channels.