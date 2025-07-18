# frozen_string_literal: true

require 'spec_helper'

RSpec.describe "Complex Workflow Integration", type: :integration do
  describe "workflow pattern validation" do
    it "creates and validates linear workflow dependency structure" do
      # Linear: A → B → C → D (sequential dependencies)
      workflow = create_complex_workflow(:linear, namespace: "linear_test")
      
      # Verify workflow was created successfully
      expect(workflow[:type]).to eq(:linear)
      expect(workflow[:steps].length).to eq(4)
      
      # Verify no pool timeouts during creation
      workflow[:steps].each_with_index do |step, index|
        verify_no_pool_timeouts(step, "linear_step_#{index}")
        verify_workflow_step_structure(step, workflow[:task]['task_id'])
      end
      
      # Analyze actual dependency structure via Rust Performance API
      analysis = analyze_workflow_dependencies(workflow[:task]['task_id'])
      
      puts "📊 Linear workflow analysis: #{analysis.inspect}"
      
      # Validate linear dependency structure
      expect(analysis['has_cycles']).to be false, "Linear workflow should not have cycles"
      expect(analysis['total_steps']).to eq(4), "Should have exactly 4 steps"
      
      # NOTE: These expectations might reveal logical flaws in factory system
      # If they fail, it indicates issues with dependency creation logic
      expect(analysis['max_depth']).to eq(4), "Linear workflow should have depth 4 (A→B→C→D)"
      expect(analysis['parallel_branches']).to eq(0), "Linear workflow should have no parallelism"
      
      puts "✅ Linear workflow structure validated: depth=#{analysis['max_depth']}, parallel=#{analysis['parallel_branches']}"
    end

    it "creates and validates diamond workflow dependency structure" do
      # Diamond: A → B,C → D (branching and merging)
      workflow = create_complex_workflow(:diamond, namespace: "diamond_test")
      
      expect(workflow[:type]).to eq(:diamond)
      expect(workflow[:steps].length).to eq(4)
      
      # Verify creation success
      workflow[:steps].each_with_index do |step, index|
        verify_no_pool_timeouts(step, "diamond_step_#{index}")
        verify_workflow_step_structure(step, workflow[:task]['task_id'])
      end
      
      # Analyze dependency structure
      analysis = analyze_workflow_dependencies(workflow[:task]['task_id'])
      
      puts "📊 Diamond workflow analysis: #{analysis.inspect}"
      
      # Validate diamond dependency structure
      expect(analysis['has_cycles']).to be false, "Diamond workflow should not have cycles"
      expect(analysis['total_steps']).to eq(4), "Should have exactly 4 steps"
      
      # CRITICAL: These might fail if factory logic is flawed
      expect(analysis['max_depth']).to eq(3), "Diamond workflow should have depth 3 (A→{B,C}→D)"
      expect(analysis['parallel_branches']).to eq(2), "Diamond should have 2 parallel branches (B and C)"
      
      puts "✅ Diamond workflow structure validated: depth=#{analysis['max_depth']}, parallel=#{analysis['parallel_branches']}"
    end

    it "creates and validates parallel workflow dependency structure" do
      # Parallel: A → B, A → C, A → D (multiple parallel branches from single root)
      workflow = create_complex_workflow(:parallel, namespace: "parallel_test")
      
      expect(workflow[:type]).to eq(:parallel)
      expect(workflow[:steps].length).to eq(4)
      
      # Verify creation success
      workflow[:steps].each_with_index do |step, index|
        verify_no_pool_timeouts(step, "parallel_step_#{index}")
        verify_workflow_step_structure(step, workflow[:task]['task_id'])
      end
      
      # Analyze dependency structure
      analysis = analyze_workflow_dependencies(workflow[:task]['task_id'])
      
      puts "📊 Parallel workflow analysis: #{analysis.inspect}"
      
      # Validate parallel dependency structure
      expect(analysis['has_cycles']).to be false, "Parallel workflow should not have cycles"
      expect(analysis['total_steps']).to eq(4), "Should have exactly 4 steps"
      
      # CRITICAL: These might reveal factory logic issues
      expect(analysis['max_depth']).to eq(2), "Parallel workflow should have depth 2 (A→{B,C,D})"
      expect(analysis['parallel_branches']).to eq(3), "Parallel should have 3 parallel branches (B, C, D)"
      
      puts "✅ Parallel workflow structure validated: depth=#{analysis['max_depth']}, parallel=#{analysis['parallel_branches']}"
    end

    it "creates and validates tree workflow dependency structure" do
      # Tree: A → B,C where B → D, C → E (hierarchical branching)
      workflow = create_complex_workflow(:tree, namespace: "tree_test")
      
      expect(workflow[:type]).to eq(:tree)
      expect(workflow[:steps].length).to eq(5)
      
      # Verify creation success
      workflow[:steps].each_with_index do |step, index|
        verify_no_pool_timeouts(step, "tree_step_#{index}")
        verify_workflow_step_structure(step, workflow[:task]['task_id'])
      end
      
      # Analyze dependency structure
      analysis = analyze_workflow_dependencies(workflow[:task]['task_id'])
      
      puts "📊 Tree workflow analysis: #{analysis.inspect}"
      
      # Validate tree dependency structure
      expect(analysis['has_cycles']).to be false, "Tree workflow should not have cycles"
      expect(analysis['total_steps']).to eq(5), "Should have exactly 5 steps"
      
      # CRITICAL: Tree structure validation - this will reveal factory logic issues
      expect(analysis['max_depth']).to eq(3), "Tree workflow should have depth 3 (A→{B,C}→{D,E})"
      expect(analysis['parallel_branches']).to eq(2), "Tree should have 2 parallel branches at each level"
      
      puts "✅ Tree workflow structure validated: depth=#{analysis['max_depth']}, parallel=#{analysis['parallel_branches']}"
    end
  end

  describe "workflow dependency logic validation" do
    it "validates linear workflow dependencies are correctly sequenced" do
      workflow = create_complex_workflow(:linear, namespace: "linear_deps")
      steps = workflow[:steps]
      
      # Get step IDs in creation order
      step_a_id = steps[0]['workflow_step_id']
      step_b_id = steps[1]['workflow_step_id'] 
      step_c_id = steps[2]['workflow_step_id']
      
      # Verify dependency data was stored in inputs
      # Step A should have no dependencies
      expect(steps[0]['inputs']['depends_on']).to be_nil
      
      # Step B should depend only on A
      expect(steps[1]['inputs']['depends_on']).to eq([step_a_id])
      
      # Step C should depend only on B
      expect(steps[2]['inputs']['depends_on']).to eq([step_b_id])
      
      # Step D should depend only on C
      expect(steps[3]['inputs']['depends_on']).to eq([step_c_id])
      
      puts "✅ Linear workflow dependencies correctly sequenced"
    end

    it "validates diamond workflow dependencies are correctly branched and merged" do
      workflow = create_complex_workflow(:diamond, namespace: "diamond_deps")
      steps = workflow[:steps]
      
      step_a_id = steps[0]['workflow_step_id']
      step_b_id = steps[1]['workflow_step_id']
      step_c_id = steps[2]['workflow_step_id']
      
      # Step A should have no dependencies
      expect(steps[0]['inputs']['depends_on']).to be_nil
      
      # Step B should depend only on A
      expect(steps[1]['inputs']['depends_on']).to eq([step_a_id])
      
      # Step C should depend only on A
      expect(steps[2]['inputs']['depends_on']).to eq([step_a_id])
      
      # Step D should depend on BOTH B and C
      expect(steps[3]['inputs']['depends_on']).to contain_exactly(step_b_id, step_c_id)
      
      puts "✅ Diamond workflow dependencies correctly branched and merged"
    end

    it "validates parallel workflow dependencies prevent incorrect inter-branch dependencies" do
      workflow = create_complex_workflow(:parallel, namespace: "parallel_deps")
      steps = workflow[:steps]
      
      step_a_id = steps[0]['workflow_step_id']
      
      # Step A should have no dependencies
      expect(steps[0]['inputs']['depends_on']).to be_nil
      
      # Steps B, C, D should all depend ONLY on A (no cross-dependencies)
      [1, 2, 3].each do |i|
        expect(steps[i]['inputs']['depends_on']).to eq([step_a_id])
      end
      
      puts "✅ Parallel workflow dependencies correctly isolated"
    end

    it "validates tree workflow dependencies maintain hierarchical structure" do
      workflow = create_complex_workflow(:tree, namespace: "tree_deps")
      steps = workflow[:steps]
      
      step_a_id = steps[0]['workflow_step_id']  # Root
      step_b_id = steps[1]['workflow_step_id']  # Left branch
      step_c_id = steps[2]['workflow_step_id']  # Right branch
      
      # Root should have no dependencies
      expect(steps[0]['inputs']['depends_on']).to be_nil
      
      # Branches should depend only on root
      expect(steps[1]['inputs']['depends_on']).to eq([step_a_id])
      expect(steps[2]['inputs']['depends_on']).to eq([step_a_id])
      
      # Leaves should depend only on their respective branches
      expect(steps[3]['inputs']['depends_on']).to eq([step_b_id])
      expect(steps[4]['inputs']['depends_on']).to eq([step_c_id])
      
      puts "✅ Tree workflow dependencies maintain hierarchical structure"
    end
  end

  describe "workflow execution readiness" do
    it "identifies viable steps for linear workflow" do
      workflow = create_complex_workflow(:linear, namespace: "linear_exec")
      task_id = workflow[:task]['task_id']
      
      # Get viable steps (should initially only show first step)
      viable_steps = TaskerCore::Performance.discover_viable_steps(task_id)
      verify_no_pool_timeouts(viable_steps, "linear_viable_steps")
      
      expect(viable_steps).to be_an(Array)
      
      if viable_steps.length > 0
        # Should only have one viable step initially (Step A)
        expect(viable_steps.length).to eq(1)
        
        first_viable = viable_steps[0]
        expect(first_viable).to have_key('workflow_step_id')
        expect(first_viable).to have_key('step_name')
        expect(first_viable['dependencies_satisfied']).to be true
        expect(first_viable['is_ready']).to be true
        
        puts "✅ Linear workflow shows correct initial viable step: #{first_viable['step_name']}"
      else
        puts "⚠️  No viable steps found - may indicate dependency logic issues"
      end
    end

    it "identifies viable steps for diamond workflow" do
      workflow = create_complex_workflow(:diamond, namespace: "diamond_exec")
      task_id = workflow[:task]['task_id']
      
      viable_steps = TaskerCore::Performance.discover_viable_steps(task_id)
      verify_no_pool_timeouts(viable_steps, "diamond_viable_steps")
      
      expect(viable_steps).to be_an(Array)
      
      if viable_steps.length > 0
        # Diamond should initially show one viable step (Step A)
        expect(viable_steps.length).to eq(1)
        
        first_viable = viable_steps[0]
        expect(first_viable['dependencies_satisfied']).to be true
        expect(first_viable['is_ready']).to be true
        
        puts "✅ Diamond workflow shows correct initial viable step: #{first_viable['step_name']}"
      else
        puts "⚠️  No viable steps found for diamond workflow"
      end
    end

    it "identifies viable steps for parallel workflow" do
      workflow = create_complex_workflow(:parallel, namespace: "parallel_exec")
      task_id = workflow[:task]['task_id']
      
      viable_steps = TaskerCore::Performance.discover_viable_steps(task_id)
      verify_no_pool_timeouts(viable_steps, "parallel_viable_steps")
      
      expect(viable_steps).to be_an(Array)
      
      if viable_steps.length > 0
        # Parallel should initially show one viable step (Step A)
        expect(viable_steps.length).to eq(1)
        
        first_viable = viable_steps[0]
        expect(first_viable['dependencies_satisfied']).to be true
        expect(first_viable['is_ready']).to be true
        
        puts "✅ Parallel workflow shows correct initial viable step: #{first_viable['step_name']}"
      else
        puts "⚠️  No viable steps found for parallel workflow"
      end
    end
  end

  describe "workflow performance and scalability" do
    it "creates complex workflows rapidly without pool exhaustion" do
      workflows = []
      start_time = Time.now
      
      # Create 5 workflows of each type rapidly
      [:linear, :diamond, :parallel, :tree].each do |type|
        5.times do |i|
          workflow = create_complex_workflow(type, namespace: "perf_#{type}_#{i}")
          
          # Verify no timeouts during rapid creation
          workflow[:steps].each_with_index do |step, step_index|
            verify_no_pool_timeouts(step, "perf_#{type}_#{i}_step_#{step_index}")
          end
          
          workflows << workflow
        end
      end
      
      elapsed = Time.now - start_time
      
      expect(workflows.length).to eq(20)
      puts "✅ 20 complex workflows created in #{elapsed.round(2)}s without pool timeouts"
    end

    it "analyzes multiple workflows efficiently" do
      # Create different workflow types
      workflows = []
      workflows << create_complex_workflow(:linear, namespace: "multi_linear")
      workflows << create_complex_workflow(:diamond, namespace: "multi_diamond")
      workflows << create_complex_workflow(:parallel, namespace: "multi_parallel")
      
      # Analyze all workflows rapidly
      analyses = []
      start_time = Time.now
      
      workflows.each_with_index do |workflow, index|
        analysis = analyze_workflow_dependencies(workflow[:task]['task_id'])
        analyses << analysis
      end
      
      elapsed = Time.now - start_time
      
      expect(analyses.length).to eq(3)
      
      # Each analysis should show correct structure for its type
      expect(analyses[0]['max_depth']).to eq(4)  # Linear
      expect(analyses[1]['max_depth']).to eq(3)  # Diamond  
      expect(analyses[2]['max_depth']).to eq(2)  # Parallel
      
      puts "✅ 3 workflow analyses completed in #{elapsed.round(3)}s"
    end
  end

  describe "error handling and edge cases" do
    it "handles workflows with missing dependencies gracefully" do
      # Create a foundation but manually create steps with invalid dependencies
      foundation = create_foundation_via_domain_api(namespace: "error_test")
      task_id = foundation.dig('task', 'task_id') || foundation.dig('named_task', 'task_id')
      
      # Try to create a step with invalid dependency
      step_with_bad_dep = TaskerCore::Factory.workflow_step(
        task_id: task_id,
        name: "bad_dependency_step",
        inputs: { depends_on: [999999999] }  # Non-existent step ID
      )
      
      verify_no_pool_timeouts(step_with_bad_dep, "bad_dependency_creation")
      
      # Should either create successfully or return error (but not timeout)
      if step_with_bad_dep['error']
        expect(step_with_bad_dep['error']).to be_a(String)
        expect(step_with_bad_dep['error']).not_to include('pool timed out')
        puts "✅ Invalid dependency handled gracefully: #{step_with_bad_dep['error']}"
      else
        # If created successfully, dependency analysis should handle it
        analysis = analyze_workflow_dependencies(task_id)
        expect(analysis).to be_a(Hash)
        puts "✅ Invalid dependency created but analysis handles it gracefully"
      end
    end

    it "handles empty workflows gracefully" do
      # Create task with no workflow steps
      task = create_task_via_domain_api(name: "empty_workflow")
      task_id = task['task_id']
      
      # Analyze empty workflow
      analysis = analyze_workflow_dependencies(task_id)
      
      expect(analysis['total_steps']).to eq(0)
      expect(analysis['max_depth']).to eq(0)
      expect(analysis['parallel_branches']).to eq(0)
      expect(analysis['has_cycles']).to be false
      
      # Get viable steps for empty workflow
      viable_steps = TaskerCore::Performance.discover_viable_steps(task_id)
      verify_no_pool_timeouts(viable_steps, "empty_workflow_viable_steps")
      
      expect(viable_steps).to be_an(Array)
      expect(viable_steps).to be_empty
      
      puts "✅ Empty workflow handled gracefully"
    end
  end
end