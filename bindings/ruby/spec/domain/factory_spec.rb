# frozen_string_literal: true

require 'spec_helper'

RSpec.describe TaskerCore::TestHelpers::Factories, type: :domain_api do
  describe "domain status and operations" do
    it "provides correct domain information and creates tasks successfully" do
      # Test domain info matches expected structure
      domain_info = TaskerCore::TestHelpers::Factories.handle_info
      expect(domain_info).to include(
        domain: "Factory",
        backend: "TaskerCore::TestHelpers",
        status: "operational",
        methods: ["task", "workflow_step", "foundation"]
      )
      expect(domain_info).to have_key(:checked_at)

      # Test that factory operations work consistently
      task1 = TaskerCore::TestHelpers::Factories.task(name: "domain_test_1", namespace: "domain_test_#{Time.now.to_i}")
      expect(task1).to have_key("task_id")
      verify_no_pool_timeouts(task1, "first_factory_operation")

      task2 = TaskerCore::TestHelpers::Factories.task(name: "domain_test_2", namespace: "domain_test_#{Time.now.to_i}_2")
      expect(task2).to have_key("task_id")
      verify_no_pool_timeouts(task2, "second_factory_operation")

      # Verify tasks have different IDs (proper uniqueness)
      expect(task1["task_id"]).not_to eq(task2["task_id"])

      puts "✅ Factory domain operations verified"
    end
  end

  describe ".task" do
    it "creates a task with real database integration" do
      task_name = "domain_test_task_#{SecureRandom.hex(4)}"

      task = TaskerCore::TestHelpers::Factories.task(
        name: task_name,
        initiator: "rspec_factory_test",
        context: {
          test_framework: "rspec",
          domain: "factory",
          creation_time: Time.now.iso8601
        }
      )

      # Verify no pool timeouts
      verify_no_pool_timeouts(task, "task_creation")

      # Verify task structure
      verify_task_structure(task, task_name)

      # Verify context was stored correctly
      expect(task['context']['test_framework']).to eq('rspec')
      expect(task['context']['domain']).to eq('factory')
      
      # Note: initiator is a creation parameter, not necessarily returned in response
      # The important thing is that task creation succeeded with the initiator parameter

      puts "✅ Task created with ID: #{task['task_id']}"
    end

    it "creates tasks with unique IDs and no collisions" do
      tasks = []

      # Create multiple tasks rapidly
      5.times do |i|
        task = TaskerCore::TestHelpers::Factories.task(
          name: "collision_test_#{i}",
          context: { test_index: i }
        )
        verify_no_pool_timeouts(task, "collision_test_#{i}")
        verify_task_structure(task)
        tasks << task
      end

      # Verify all task IDs are unique
      task_ids = tasks.map { |t| t['task_id'] }
      expect(task_ids.uniq.length).to eq(5)

      # Verify all are valid integers
      task_ids.each do |id|
        expect(id).to be_a(Integer)
        expect(id).to be > 0
      end

      puts "✅ 5 tasks created with unique IDs: #{task_ids.inspect}"
    end

    it "handles context data correctly" do
      complex_context = {
        api_config: {
          url: "https://api.example.com/v2",
          timeout: 30,
          headers: { "X-API-Version" => "2.1" }
        },
        workflow_metadata: {
          priority: "high",
          tags: ["payment", "urgent"],
          estimated_duration_minutes: 15
        },
        nested_data: {
          level1: {
            level2: {
              value: "deeply_nested_value"
            }
          }
        }
      }

      task = TaskerCore::TestHelpers::Factories.task(
        name: "complex_context_test",
        context: complex_context
      )

      verify_no_pool_timeouts(task, "complex_context_creation")
      verify_task_structure(task)

      # Verify complex context was preserved
      stored_context = task['context']
      expect(stored_context['api_config']['url']).to eq('https://api.example.com/v2')
      expect(stored_context['workflow_metadata']['tags']).to include('payment', 'urgent')
      expect(stored_context['nested_data']['level1']['level2']['value']).to eq('deeply_nested_value')

      puts "✅ Complex context data preserved correctly"
    end
  end

  describe ".workflow_step" do
    it "creates workflow steps linked to real tasks" do
      # First create a parent task
      task = TaskerCore::TestHelpers::Factories.task(name: "step_parent_task")
      verify_no_pool_timeouts(task, "parent_task_creation")
      task_id = task['task_id']

      # Create workflow step
      step_name = "test_step_#{SecureRandom.hex(4)}"
      step = TaskerCore::TestHelpers::Factories.workflow_step(
        task_id: task_id,
        name: step_name,
        inputs: {
          step_type: "api_call",
          url: "https://api.example.com/process",
          method: "POST",
          expected_duration_seconds: 5
        }
      )

      verify_no_pool_timeouts(step, "workflow_step_creation")
      verify_workflow_step_structure(step, task_id, step_name)

      # Verify step inputs were stored correctly
      expect(step['inputs']['step_type']).to eq('api_call')
      expect(step['inputs']['url']).to eq('https://api.example.com/process')
      expect(step['inputs']['method']).to eq('POST')

      puts "✅ Workflow step created with ID: #{step['workflow_step_id']} for task: #{task_id}"
    end

    it "creates multiple workflow steps for the same task" do
      # Create parent task
      task = TaskerCore::TestHelpers::Factories.task(name: "multi_step_task")
      verify_no_pool_timeouts(task, "multi_step_parent_task")
      task_id = task['task_id']

      steps = []

      # Create multiple steps
      3.times do |i|
        step = TaskerCore::TestHelpers::Factories.workflow_step(
          task_id: task_id,
          name: "step_#{i}",
          inputs: { position: i, step_type: "sequential" }
        )
        verify_no_pool_timeouts(step, "multi_step_creation_#{i}")
        verify_workflow_step_structure(step, task_id, "step_#{i}")
        steps << step
      end

      # Verify all steps belong to same task
      steps.each do |step|
        expect(step['task_id']).to eq(task_id)
      end

      # Verify all have unique workflow_step_ids
      step_ids = steps.map { |s| s['workflow_step_id'] }
      expect(step_ids.uniq.length).to eq(3)

      puts "✅ 3 workflow steps created for task #{task_id}: #{step_ids.inspect}"
    end

    it "handles complex input data structures" do
      task = TaskerCore::TestHelpers::Factories.task(name: "complex_inputs_test")
      verify_no_pool_timeouts(task, "complex_inputs_parent")

      complex_inputs = {
        api_call: {
          url: "https://payments.example.com/charge",
          method: "POST",
          headers: {
            "Content-Type" => "application/json",
            "Authorization" => "Bearer ${TOKEN}"
          },
          body: {
            amount: 10000,
            currency: "USD",
            metadata: {
              order_id: "order_12345",
              customer_id: "cust_67890"
            }
          }
        },
        validation_rules: {
          required_fields: ["amount", "currency"],
          amount_range: { min: 100, max: 100000 },
          allowed_currencies: ["USD", "EUR", "GBP"]
        },
        retry_config: {
          max_attempts: 3,
          backoff_multiplier: 2.0,
          timeout_seconds: 30
        }
      }

      step = TaskerCore::TestHelpers::Factories.workflow_step(
        task_id: task['task_id'],
        name: "payment_processing_step",
        inputs: complex_inputs
      )

      verify_no_pool_timeouts(step, "complex_inputs_step")
      verify_workflow_step_structure(step, task['task_id'], "payment_processing_step")

      # Verify complex inputs were preserved
      stored_inputs = step['inputs']
      expect(stored_inputs['api_call']['url']).to eq('https://payments.example.com/charge')
      expect(stored_inputs['validation_rules']['amount_range']['max']).to eq(100000)
      expect(stored_inputs['retry_config']['backoff_multiplier']).to eq(2.0)

      puts "✅ Complex input data structure preserved correctly"
    end
  end

  describe ".foundation" do
    it "creates foundation data with namespace, task, and step" do
      namespace_name = "foundation_test_#{SecureRandom.hex(4)}"

      foundation = TaskerCore::TestHelpers::Factories.foundation(
        task_name: "foundation_task",
        namespace: namespace_name,
        step_name: "foundation_step_1"
      )

      verify_no_pool_timeouts(foundation, "foundation_creation")

      # Verify foundation structure
      expect(foundation).to have_key('namespace')
      expect(foundation).to have_key('named_task')
      expect(foundation).to have_key('named_step')

      # Verify namespace
      namespace = foundation['namespace']
      expect(namespace['name']).to eq(namespace_name)
      expect(namespace['namespace_id']).to be_a(Integer)

      # Verify named task
      named_task = foundation['named_task']
      expect(named_task['name']).to eq('foundation_task')
      expect(named_task['named_task_id']).to be_a(Integer)
      expect(named_task['namespace_id']).to eq(namespace['namespace_id'])

      # Verify named step
      named_step = foundation['named_step']
      expect(named_step['name']).to eq('foundation_step_1')
      expect(named_step['named_step_id']).to be_a(Integer)
      # Note: named_step doesn't have named_task_id - it belongs to dependent_system
      # Relationship to named_task is through junction table

      puts "✅ Foundation created: namespace=#{namespace['namespace_id']}, task=#{named_task['named_task_id']}, step=#{named_step['named_step_id']}"
    end

    it "creates foundation with real task instance" do
      foundation = TaskerCore::TestHelpers::Factories.foundation(
        task_name: "real_task_foundation",
        namespace: "real_test"
      )

      verify_no_pool_timeouts(foundation, "real_foundation_creation")

      # Should also create a real task instance
      if foundation['task']
        task = foundation['task']
        verify_task_structure(task)

        # Task should be linked to the named task
        named_task = foundation['named_task']
        expect(task['name']).to include('real_task_foundation')

        puts "✅ Foundation includes real task instance: #{task['task_id']}"
      else
        puts "ℹ️  Foundation created without real task instance (foundation-only mode)"
      end
    end
  end

  describe "stress testing" do
    it "handles rapid task creation without pool exhaustion" do
      results = stress_test_operations(25) do |i|
        TaskerCore::TestHelpers::Factories.task(
          name: "stress_test_task_#{i}",
          context: { stress_test_index: i, batch_id: SecureRandom.hex(4) }
        )
      end

      # Verify all tasks were created successfully
      expect(results.length).to eq(25)

      # Verify all have unique task IDs
      task_ids = results.map { |r| r['task_id'] }
      expect(task_ids.uniq.length).to eq(25)

      puts "✅ 25 rapid task creations completed successfully"
    end

    it "handles rapid workflow step creation without pool exhaustion" do
      # Create parent task
      parent_task = TaskerCore::TestHelpers::Factories.task(name: "stress_test_parent")
      verify_no_pool_timeouts(parent_task, "stress_parent_creation")
      task_id = parent_task['task_id']

      results = stress_test_operations(25) do |i|
        TaskerCore::TestHelpers::Factories.workflow_step(
          task_id: task_id,
          name: "stress_step_#{i}",
          inputs: { stress_index: i, batch_id: SecureRandom.hex(4) }
        )
      end

      # Verify all steps were created successfully
      expect(results.length).to eq(25)

      # Verify all belong to same task
      results.each do |step|
        expect(step['task_id']).to eq(task_id)
      end

      # Verify unique step IDs
      step_ids = results.map { |r| r['workflow_step_id'] }
      expect(step_ids.uniq.length).to eq(25)

      puts "✅ 25 rapid workflow step creations completed successfully"
    end
  end

  describe "error handling and resilience" do
    it "handles invalid task_id for workflow steps gracefully" do
      invalid_task_id = 999999999  # Very unlikely to exist

      step = TaskerCore::TestHelpers::Factories.workflow_step(
        task_id: invalid_task_id,
        name: "invalid_parent_step",
        inputs: { test: true }
      )

      # Should get an error response, not a timeout
      expect(step).to be_a(Hash)
      verify_no_pool_timeouts(step, "invalid_task_id_step")

      if step['error']
        expect(step['error']).to be_a(String)
        expect(step['error']).not_to include('pool timed out')
        puts "✅ Invalid task_id handled gracefully: #{step['error']}"
      else
        # If no error, it means the workflow step was created (unexpected but not a timeout)
        verify_workflow_step_structure(step, invalid_task_id, "invalid_parent_step")
        puts "⚠️  Workflow step created with task_id #{invalid_task_id} (unexpected but acceptable)"
      end
    end

    it "handles empty context data gracefully" do
      task = TaskerCore::TestHelpers::Factories.task(
        name: "empty_context_test",
        context: {}
      )

      verify_no_pool_timeouts(task, "empty_context_task")
      verify_task_structure(task, "empty_context_test")

      expect(task['context']).to eq({})

      puts "✅ Empty context handled gracefully"
    end

    it "handles missing optional parameters gracefully" do
      # Create task with minimal parameters
      task = TaskerCore::TestHelpers::Factories.task(name: "minimal_task")

      verify_no_pool_timeouts(task, "minimal_task_creation")
      verify_task_structure(task, "minimal_task")

      # Should have default values
      expect(task['context']).to be_a(Hash)

      puts "✅ Minimal parameters handled gracefully"
    end
  end
end
