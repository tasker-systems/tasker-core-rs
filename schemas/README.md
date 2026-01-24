# Schemas

API contracts and validation schemas for the Tasker system.

## Directory Structure

```
schemas/
├── generated/                     # Auto-generated from code (do not edit)
│   ├── orchestration-openapi.json # Orchestration API (from utoipa annotations)
│   └── worker-openapi.json        # Worker API (from utoipa annotations)
├── task-template.json             # JSON Schema for TaskTemplate YAML validation
└── README.md
```

## Generated Schemas

The `generated/` directory contains OpenAPI specs derived from utoipa annotations in the source code. These are committed to the repo so consumers can reference them without building.

### Regenerating

After modifying API handlers, response types, or utoipa annotations:

```bash
cargo make generate-schemas   # or: cargo make gs
```

### CI Validation

To verify generated schemas match the source code:

```bash
cargo make check-schemas
```

This compares the committed schemas against freshly generated output and fails if they differ.

## Task Template Schema

`task-template.json` is a JSON Schema (Draft 7) for validating TaskTemplate YAML configuration files. It validates:

- Required fields (namespace, name, version, steps)
- Step structure (handler callables, dependencies, retry config)
- Semantic version format
- Environment-specific overrides

```bash
# Validate a template YAML
jsonschema -i your-template.yaml schemas/task-template.json
```

## Future Work

- Auto-generate `task-template.json` from the Rust `TaskTemplate` struct (schemars or similar)
- Add schema diff reporting to PR checks
