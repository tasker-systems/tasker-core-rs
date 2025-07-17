# Testing Guide

## Test Database Setup

This project uses a simplified testing approach where database migrations are handled separately from test execution.

### Quick Start

1. **Setup the test database** (run once):
   ```bash
   bundle exec rake test:setup
   ```

2. **Run the tests**:
   ```bash
   bundle exec rspec
   ```

### Available Rake Tasks

- `bundle exec rake test:setup` - Setup test database with migrations
- `bundle exec rake test:clean` - Clean test database  
- `bundle exec rake test:reset` - Reset test database (clean + setup)
- `bundle exec rake test:status` - Check test database status

### Testing Architecture

The testing infrastructure uses a **TestingFramework** that:

1. **Separates concerns**: Database migrations are handled by rake tasks, not test setup
2. **Shared resources**: Uses a single database pool for all test operations
3. **Lightweight setup**: Tests only create foundation data as needed
4. **Clear lifecycle**: Explicit setup/teardown phases

### TestingFramework Benefits

- **No schema reset**: Avoids expensive database operations
- **Pool management**: Single shared pool prevents connection exhaustion
- **Explicit dependencies**: Clear separation between migration setup and test execution
- **Better performance**: Faster test startup and execution

### Troubleshooting

If tests fail with "relation does not exist" errors:

1. **Run database setup**:
   ```bash
   bundle exec rake test:setup
   ```

2. **Check database status**:
   ```bash
   bundle exec rake test:status
   ```

3. **Reset if needed**:
   ```bash
   bundle exec rake test:reset
   ```

### Example Test Structure

```ruby
RSpec.describe 'My Feature' do
  include TaskerCore::TestHelpers

  it 'creates test data' do
    # Database is already migrated by rake task
    # Create test data as needed
    task = create_test_task(name: 'my_task')
    step = create_test_workflow_step(task_id: task['task_id'])
    
    # Test your feature
    expect(task['status']).to eq('pending')
  end
end
```

This approach eliminates the complex migration checking that was causing pool timeouts and provides a cleaner separation of concerns.