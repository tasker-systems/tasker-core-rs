#!/bin/bash

echo "🔧 Fixing remaining compilation errors in tasker-orchestration..."

# Fix 1: Fix typo in import paths - "errorss" should be "errors"
echo "Fixing errorss typo..."
find tasker-orchestration/src -name "*.rs" -type f -exec sed -i '' 's/use tasker_shared::errorss/use tasker_shared::errors/g' {} \;

# Fix 2: Fix error import path - "error" should be "errors"  
echo "Fixing error vs errors import..."
find tasker-orchestration/src -name "*.rs" -type f -exec sed -i '' 's/use tasker_shared::error/use tasker_shared::errors/g' {} \;

# Fix 3: Fix config field references - config_source_system should be config_source
echo "Fixing config field names..."
find tasker-orchestration/src -name "*.rs" -type f -exec sed -i '' 's/\.config_source_system/.config_source/g' {} \;

# Fix 4: Fix old crate:: imports that should be absolute
echo "Fixing remaining crate:: imports..."
find tasker-orchestration/src -name "*.rs" -type f -exec sed -i '' -e 's/use crate::orchestration::errors::/use tasker_orchestration::orchestration::errors::/g' {} \;
find tasker-orchestration/src -name "*.rs" -type f -exec sed -i '' -e 's/use crate::orchestration::orchestration_loop::/use tasker_orchestration::orchestration::orchestration_loop::/g' {} \;
find tasker-orchestration/src -name "*.rs" -type f -exec sed -i '' -e 's/use crate::orchestration::types::/use tasker_orchestration::orchestration::types::/g' {} \;

# Fix 5: Fix actual source variable references in code  
echo "Fixing source variable references..."
find tasker-orchestration/src -name "*.rs" -type f -exec sed -i '' 's/\bsource\b/config_source/g' {} \;

# Fix 6: Replace TaskerError::Configuration with TaskerError::ConfigurationError  
echo "Fixing Configuration enum variant..."
find tasker-orchestration/src -name "*.rs" -type f -exec sed -i '' 's/TaskerError::Configuration/TaskerError::ConfigurationError/g' {} \;

# Fix 7: Fix task_claimer config import
echo "Fixing task_claimer config import..."
find tasker-orchestration/src -name "*.rs" -type f -exec sed -i '' 's/use tasker_shared::config::task_claimer/use tasker_shared::config::TaskClaimerConfig/g' {} \;

echo "✅ Applied remaining error fixes"
echo "🧪 Testing compilation..."

cargo check -p tasker-orchestration 2>&1 | grep "error\[E" | wc -l | xargs echo "Remaining errors:"