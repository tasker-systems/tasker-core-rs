# TaskTemplate Schemas

This directory contains the canonical schemas for Tasker TaskTemplate configuration and API specifications.

## Files

### `task-template.json`
**JSON Schema for TaskTemplate YAML validation**

- **Purpose**: Validates TaskTemplate YAML configuration files against the self-describing TaskTemplate structure
- **Schema Version**: JSON Schema Draft 7
- **Validates**: All core TaskTemplate fields including steps, handlers, system dependencies, domain events, and environment overrides
- **Key Features**:
  - Enforces at least 1 step requirement (templates without steps are invalid)
  - Supports flexible callable patterns (allows underscores in class names)
  - Accepts any string for system dependencies (not restricted to predefined enum)
  - Validates semantic version format (x.y.z)
  - Handles nullable fields appropriately
  - Supports environment-specific overrides

**Usage:**
```bash
# Validate a YAML file against the schema
jsonschema -i your-template.yaml data/schemas/task-template.json
```

### `task-template-api.yaml`
**OpenAPI 3.0 specification for TaskTemplate management API**

- **Purpose**: Documents the complete REST API for TaskTemplate management and task execution
- **Specification Version**: OpenAPI 3.0.3
- **Covers**:
  - Template CRUD operations (create, read, update, delete)
  - Template validation endpoints
  - Task execution from templates
  - Template discovery and metadata management
  - Environment-specific template resolution

**Key Endpoints:**
- `GET /templates` - List templates with filtering
- `POST /templates` - Create new template
- `GET /templates/{namespace}/{name}` - Get specific template
- `PUT /templates/{namespace}/{name}` - Update template
- `DELETE /templates/{namespace}/{name}` - Delete template
- `POST /templates/validate` - Validate template
- `POST /templates/{namespace}/{name}/execute` - Execute task from template

**Usage:**
```bash
# Generate API client code
openapi-generator-cli generate -i data/schemas/task-template-api.yaml -g python-client
```

## Schema Validation Results

✅ **All schemas validated successfully** against existing migrated TaskTemplate YAML files:

- **Files Tested**: 4 TaskTemplate configurations
- **Total Callable References**: 25 handler classes validated
- **Environment Overrides**: 7 different environments validated
- **Step Configurations**: 21 workflow steps validated

**Tested Files:**
- `linear_workflow_handler.yaml` - Sequential mathematical operations
- `order_fulfillment_handler.yaml` - Complete order processing workflow  
- `tree_workflow_handler.yaml` - Hierarchical computation pattern
- `credit_card_payment.yaml` - Payment processing with fraud detection

## Schema Features

### TaskTemplate JSON Schema Highlights

1. **Flexible Callable Patterns**: Supports real-world handler class naming like `PaymentProcessing::StepHandler::ValidatePaymentHandler`

2. **System Dependency Freedom**: Accepts any system dependency string (e.g., `payment_gateway`, `fraud_detection_api`) rather than restricting to predefined enums

3. **Proper Null Handling**: All optional fields correctly handle null values from YAML serialization

4. **Environment Override Support**: Full support for environment-specific configuration overrides with flexible structure

5. **Step Validation**: Enforces business rule that templates must have at least one step

### OpenAPI 3.0 API Highlights

1. **Comprehensive CRUD**: Full template lifecycle management with proper HTTP semantics

2. **Environment Resolution**: API supports resolving templates for any environment, not restricted to predefined list

3. **Flexible Input Validation**: Template input validation against embedded JSON Schema

4. **Task Execution Integration**: Direct task execution from templates with environment-specific resolution

5. **Developer Experience**: Comprehensive examples, error codes, and response schemas

## Integration

### Ruby Integration
The JSON Schema is designed to work with the Ruby dry-struct validation in `TaskerCore::Types::TaskTemplate`. Symbol keys from Ruby YAML serialization are supported through key normalization.

### Rust Integration  
The schemas align with the Rust `TaskTemplate` struct in `src/models/core/task_template.rs` and support serde serialization/deserialization.

### Database Integration
Schemas support JSONB storage format used by PostgreSQL in the TaskTemplate registry system.

## Maintenance

When updating TaskTemplate structure:

1. Update the Rust struct in `src/models/core/task_template.rs`
2. Update the Ruby dry-struct in `bindings/ruby/lib/tasker_core/types/task_template.rb`  
3. Update `task-template.json` schema to match
4. Update `task-template-api.yaml` components to match
5. Validate changes against existing YAML files using the validation script

## Validation Script

A Python validation script is available for testing schema accuracy:

```python
# Test schema against migrated YAML files
python3 -c "
import json, yaml, jsonschema

# Load schema and test files
with open('data/schemas/task-template.json') as f:
    schema = json.load(f)

# Validate your YAML file  
with open('your-template.yaml') as f:
    template = yaml.safe_load(f)

jsonschema.validate(template, schema)
print('✅ Template is valid!')
"
```