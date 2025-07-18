# Load our new domain-based TaskerCore architecture
require_relative '../lib/tasker_core'

# Helper module for domain API testing
module DomainTestHelpers
  # Create tasks using our new domain API
  def create_task_via_domain_api(options = {})
    default_options = {
      name: "test_task_#{SecureRandom.hex(4)}",
      initiator: 'rspec_domain_test',
      context: { test_framework: 'rspec', test_type: 'domain_api' }
    }

    TaskerCore::Factory.task(default_options.merge(options))
  end

  # Create workflow steps using our new domain API
  def create_workflow_step_via_domain_api(task_id, options = {})
    default_options = {
      task_id: task_id,
      name: "test_step_#{SecureRandom.hex(4)}",
      inputs: { test_data: 'domain_api_input' }
    }

    TaskerCore::Factory.workflow_step(default_options.merge(options))
  end

  # Create foundation data using our new domain API
  def create_foundation_via_domain_api(options = {})
    default_options = {
      task_name: "foundation_task_#{SecureRandom.hex(4)}",
      namespace: 'rspec_domain_test'
    }

    TaskerCore::Factory.foundation(default_options.merge(options))
  end

  # Verify no pool timeouts in results
  def verify_no_pool_timeouts(result, operation_name = "operation")
    expect(result).to be_a(Hash), "Expected Hash result for #{operation_name}"

    if result['error']
      if result['error'].include?('pool timed out') || result['error'].include?('timeout')
        fail "❌ POOL TIMEOUT in #{operation_name}: #{result['error']}"
      else
        # Other errors are acceptable for testing - log but don't fail
        puts "⚠️  #{operation_name} error (not timeout): #{result['error']}"
      end
    end

    result
  end

  # Perform multiple operations and verify no pool exhaustion
  def stress_test_operations(count = 50, &operation_block)
    results = []

    count.times do |i|
      result = operation_block.call(i)
      verify_no_pool_timeouts(result, "stress_test_operation_#{i}")
      results << result
    end

    results
  end

  # Verify task structure
  def verify_task_structure(task, expected_name = nil)
    expect(task).to have_key('task_id')
    expect(task['task_id']).to be_a(Integer)
    expect(task['task_id']).to be > 0

    if expected_name
      expect(task['name']).to eq(expected_name)
    else
      expect(task).to have_key('name')
    end

    expect(task).to have_key('context')
    expect(task['context']).to be_a(Hash)
  end

  # Verify workflow step structure
  def verify_workflow_step_structure(step, expected_task_id = nil, expected_name = nil)
    expect(step).to have_key('workflow_step_id')
    expect(step['workflow_step_id']).to be_a(Integer)
    expect(step['workflow_step_id']).to be > 0

    if expected_task_id
      expect(step['task_id']).to eq(expected_task_id)
    else
      expect(step).to have_key('task_id')
      expect(step['task_id']).to be_a(Integer)
    end

    if expected_name
      expect(step['name']).to eq(expected_name)
    else
      expect(step).to have_key('name')
    end

    expect(step).to have_key('inputs')
    expect(step['inputs']).to be_a(Hash)
  end
end

# Helper module for complex workflow validation
module WorkflowValidationHelpers
  # Create complex workflow patterns for testing dependency logic
  def create_complex_workflow(type, options = {})
    base_name = "#{type}_workflow_#{SecureRandom.hex(4)}"

    # Create foundation task
    foundation = create_foundation_via_domain_api(
      task_name: base_name,
      namespace: options[:namespace] || 'workflow_test'
    )

    task_id = foundation.dig('task', 'task_id') || foundation.dig('named_task', 'task_id')
    raise "Failed to create foundation task" unless task_id

    # Create steps based on workflow type
    steps = case type
    when :linear
      create_linear_workflow_steps(task_id, base_name)
    when :diamond
      create_diamond_workflow_steps(task_id, base_name)
    when :parallel
      create_parallel_workflow_steps(task_id, base_name)
    when :tree
      create_tree_workflow_steps(task_id, base_name)
    else
      raise "Unknown workflow type: #{type}"
    end

    {
      type: type,
      task: foundation['task'] || foundation['named_task'],
      steps: steps,
      foundation: foundation
    }
  end

  # Linear: A → B → C → D
  def create_linear_workflow_steps(task_id, base_name)
    steps = []

    # Step A (no dependencies)
    step_a = create_workflow_step_via_domain_api(task_id, {
      name: "#{base_name}_step_a",
      inputs: { step_type: 'linear_start', position: 0 }
    })
    steps << step_a

    # Step B (depends on A)
    step_b = create_workflow_step_via_domain_api(task_id, {
      name: "#{base_name}_step_b",
      inputs: { step_type: 'linear_middle', position: 1, depends_on: [step_a['workflow_step_id']] }
    })
    steps << step_b

    # Step C (depends on B)
    step_c = create_workflow_step_via_domain_api(task_id, {
      name: "#{base_name}_step_c",
      inputs: { step_type: 'linear_middle', position: 2, depends_on: [step_b['workflow_step_id']] }
    })
    steps << step_c

    # Step D (depends on C)
    step_d = create_workflow_step_via_domain_api(task_id, {
      name: "#{base_name}_step_d",
      inputs: { step_type: 'linear_end', position: 3, depends_on: [step_c['workflow_step_id']] }
    })
    steps << step_d

    steps
  end

  # Diamond: A → B,C → D
  def create_diamond_workflow_steps(task_id, base_name)
    steps = []

    # Step A (no dependencies)
    step_a = create_workflow_step_via_domain_api(task_id, {
      name: "#{base_name}_step_a",
      inputs: { step_type: 'diamond_start', position: 0 }
    })
    steps << step_a

    # Step B (depends on A)
    step_b = create_workflow_step_via_domain_api(task_id, {
      name: "#{base_name}_step_b",
      inputs: { step_type: 'diamond_branch_left', position: 1, depends_on: [step_a['workflow_step_id']] }
    })
    steps << step_b

    # Step C (depends on A)
    step_c = create_workflow_step_via_domain_api(task_id, {
      name: "#{base_name}_step_c",
      inputs: { step_type: 'diamond_branch_right', position: 1, depends_on: [step_a['workflow_step_id']] }
    })
    steps << step_c

    # Step D (depends on both B and C)
    step_d = create_workflow_step_via_domain_api(task_id, {
      name: "#{base_name}_step_d",
      inputs: { step_type: 'diamond_merge', position: 2, depends_on: [step_b['workflow_step_id'], step_c['workflow_step_id']] }
    })
    steps << step_d

    steps
  end

  # Parallel: A → B, A → C, A → D
  def create_parallel_workflow_steps(task_id, base_name)
    steps = []

    # Step A (no dependencies)
    step_a = create_workflow_step_via_domain_api(task_id, {
      name: "#{base_name}_step_a",
      inputs: { step_type: 'parallel_start', position: 0 }
    })
    steps << step_a

    # Step B (depends on A)
    step_b = create_workflow_step_via_domain_api(task_id, {
      name: "#{base_name}_step_b",
      inputs: { step_type: 'parallel_branch', position: 1, depends_on: [step_a['workflow_step_id']] }
    })
    steps << step_b

    # Step C (depends on A)
    step_c = create_workflow_step_via_domain_api(task_id, {
      name: "#{base_name}_step_c",
      inputs: { step_type: 'parallel_branch', position: 1, depends_on: [step_a['workflow_step_id']] }
    })
    steps << step_c

    # Step D (depends on A)
    step_d = create_workflow_step_via_domain_api(task_id, {
      name: "#{base_name}_step_d",
      inputs: { step_type: 'parallel_branch', position: 1, depends_on: [step_a['workflow_step_id']] }
    })
    steps << step_d

    steps
  end

  # Tree: A → B,C where B → D, C → E
  def create_tree_workflow_steps(task_id, base_name)
    steps = []

    # Step A (root)
    step_a = create_workflow_step_via_domain_api(task_id, {
      name: "#{base_name}_step_a",
      inputs: { step_type: 'tree_root', position: 0 }
    })
    steps << step_a

    # Step B (left branch from A)
    step_b = create_workflow_step_via_domain_api(task_id, {
      name: "#{base_name}_step_b",
      inputs: { step_type: 'tree_branch_left', position: 1, depends_on: [step_a['workflow_step_id']] }
    })
    steps << step_b

    # Step C (right branch from A)
    step_c = create_workflow_step_via_domain_api(task_id, {
      name: "#{base_name}_step_c",
      inputs: { step_type: 'tree_branch_right', position: 1, depends_on: [step_a['workflow_step_id']] }
    })
    steps << step_c

    # Step D (left leaf from B)
    step_d = create_workflow_step_via_domain_api(task_id, {
      name: "#{base_name}_step_d",
      inputs: { step_type: 'tree_leaf_left', position: 2, depends_on: [step_b['workflow_step_id']] }
    })
    steps << step_d

    # Step E (right leaf from C)
    step_e = create_workflow_step_via_domain_api(task_id, {
      name: "#{base_name}_step_e",
      inputs: { step_type: 'tree_leaf_right', position: 2, depends_on: [step_c['workflow_step_id']] }
    })
    steps << step_e

    steps
  end

  # Analyze workflow dependency structure via Rust Performance API
  def analyze_workflow_dependencies(task_id)
    result = TaskerCore::Performance.analyze_dependencies(task_id)
    verify_no_pool_timeouts(result, "dependency_analysis")
    result
  end

  # Validate dependency structure matches expected pattern
  def validate_workflow_structure(workflow, expected_structure)
    analysis = analyze_workflow_dependencies(workflow[:task]['task_id'])

    case expected_structure[:type]
    when :linear
      expect(analysis['has_cycles']).to be false
      expect(analysis['max_depth']).to eq(4) # A → B → C → D
      expect(analysis['parallel_branches']).to eq(0) # No parallelism
      expect(analysis['total_steps']).to eq(4)

    when :diamond
      expect(analysis['has_cycles']).to be false
      expect(analysis['max_depth']).to eq(3) # A → B,C → D
      expect(analysis['parallel_branches']).to eq(2) # B and C can run in parallel
      expect(analysis['total_steps']).to eq(4)

    when :parallel
      expect(analysis['has_cycles']).to be false
      expect(analysis['max_depth']).to eq(2) # A → B,C,D
      expect(analysis['parallel_branches']).to eq(3) # B, C, D can all run in parallel
      expect(analysis['total_steps']).to eq(4)

    when :tree
      expect(analysis['has_cycles']).to be false
      expect(analysis['max_depth']).to eq(3) # A → B,C → D,E
      expect(analysis['parallel_branches']).to eq(2) # B,C parallel and D,E parallel
      expect(analysis['total_steps']).to eq(5)

    else
      raise "Unknown expected workflow structure: #{expected_structure[:type]}"
    end

    analysis
  end
end

# Helper module for handle architecture testing
module HandleArchitectureHelpers
  # Get handle information for debugging
  def get_handle_info(domain = :factory)
    case domain
    when :factory
      TaskerCore::Factory.handle_info
    when :registry
      TaskerCore::Registry.handle_info
    when :performance
      TaskerCore::Performance.handle_info
    when :environment
      TaskerCore::Environment.handle_info
    else
      raise "Unknown domain: #{domain}"
    end
  end

  # Verify handle persistence across operations
  def verify_handle_persistence(domain = :factory, operations_count = 5)
    initial_info = get_handle_info(domain)

    operations_count.times do |i|
      # Perform operation
      case domain
      when :factory
        TaskerCore::Factory.task(name: "persistence_test_#{i}")
      when :registry
        TaskerCore::Registry.list
      when :performance
        TaskerCore::Performance.system_health
      end

      # Check handle is still the same
      current_info = get_handle_info(domain)
      expect(current_info['handle_id']).to eq(initial_info['handle_id'])
    end

    puts "✅ Handle persistence verified for #{domain} domain across #{operations_count} operations"
  end

  # Test rapid operations for pool timeout prevention
  def test_rapid_operations_no_timeouts(operations_count = 100)
    results = []
    start_time = Time.now

    operations_count.times do |i|
      result = TaskerCore::Factory.task(name: "rapid_test_#{i}")
      verify_no_pool_timeouts(result, "rapid_operation_#{i}")
      results << result
    end

    elapsed = Time.now - start_time
    puts "✅ #{operations_count} rapid operations completed in #{elapsed.round(2)}s with no pool timeouts"

    results
  end
end
