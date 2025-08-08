# Workflow Pattern Examples with Mathematical Business Logic

This directory contains Ruby handler implementations for various workflow patterns, each demonstrating different dependency structures and mathematical operations.

## Business Logic Rules

All workflows follow these mathematical transformation rules:

1. **Task Context**: Must contain an even number (`even_number`)
2. **Initial Steps**: Square the even number from task context
3. **Single Parent Steps**: Square the input from the parent step
4. **Multiple Parent Steps**: Multiply all parent results together, then square the product

## Workflow Patterns

### 1. Linear Workflow (`linear_workflow/`)
**Pattern**: A → B → C → D
**Mathematical Flow**: 
- A: n² 
- B: (n²)² = n⁴
- C: (n⁴)² = n⁸  
- D: (n⁸)² = n¹⁶

**Final Result**: `original_number^16`

### 2. Diamond Workflow (`diamond_workflow/`)
**Pattern**: A → (B, C) → D
**Mathematical Flow**:
- A: n²
- B: (n²)² = n⁴ (parallel)
- C: (n²)² = n⁴ (parallel)  
- D: (n⁴ × n⁴)² = (n⁸)² = n¹⁶

**Final Result**: `original_number^16`

### 3. Tree Workflow (`tree_workflow/`)
**Pattern**: A → (B → (D, E), C → (F, G)) → H
**Mathematical Flow**:
- A: n²
- B: (n²)² = n⁴, C: (n²)² = n⁴
- D: (n⁴)² = n⁸, E: (n⁴)² = n⁸, F: (n⁴)² = n⁸, G: (n⁴)² = n⁸
- H: (n⁸ × n⁸ × n⁸ × n⁸)² = (n³²)² = n⁶⁴

**Final Result**: `original_number^64`

### 4. Mixed DAG Workflow (`mixed_dag_workflow/`)
**Pattern**: A → B, A → C, B → D, C → D, B → E, C → F, (D,E,F) → G
**Mathematical Flow**:
- A: n²
- B: (n²)² = n⁴, C: (n²)² = n⁴
- D: (n⁴ × n⁴)² = n¹⁶, E: (n⁴)² = n⁸, F: (n⁴)² = n⁸
- G: (n¹⁶ × n⁸ × n⁸)² = (n³²)² = n⁶⁴

**Final Result**: `original_number^64`

## Handler Structure

Each workflow follows this structure:

```
workflow_name/
├── handlers/
│   └── workflow_name_handler.rb     # Main task handler
└── step_handlers/
    ├── step_1_handler.rb            # Individual step handlers
    ├── step_2_handler.rb
    └── ...
```

## Key Features

### Verification Logic
Each final step includes verification that calculates the expected mathematical result and compares it with the actual workflow output.

### Dependency Resolution
Handlers use `sequence.get(step_name)` to access results from parent steps, demonstrating proper dependency chain handling.

### Comprehensive Logging
Each step logs its mathematical operation, making it easy to trace the computation path.

### Error Handling
Handlers validate that required parent results are available before processing.

## Example Usage

```ruby
# Task context for all workflows
task_context = {
  "even_number" => 4
}

# Expected results:
# Linear & Diamond: 4^16 = 4,294,967,296
# Tree & Mixed DAG: 4^64 = 340,282,366,920,938,463,463,374,607,431,768,211,456
```

## Testing the Patterns

These handlers are designed to work with the corresponding YAML workflow configurations in `/config/tasks/integration/` and can be used to validate:

1. Proper dependency resolution
2. Mathematical correctness of sequential operations
3. Multiple parent convergence logic
4. Complex DAG coordination
5. Result propagation through workflow chains

The mathematical operations provide a deterministic way to verify that each step in the workflow executed correctly and received the proper inputs from its dependencies.