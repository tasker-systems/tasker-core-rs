# frozen_string_literal: true

require 'spec_helper'

RSpec.describe 'Step Results Persistence', type: :integration do
  let(:handle) { TaskerCore.create_orchestration_handle }
  let(:factory_options) do
    {
      "task_name" => "OrderFulfillmentTaskHandler", 
      "step_count" => 4,
      "pattern" => "linear"
    }
  end

  before do
    handle.register_step_handlers
  end

  describe 'Task and Step Creation' do
    it 'initializes orchestration system successfully' do
      expect(handle).not_to be_nil
      expect(handle).to respond_to(:create_test_task)
    end

    it 'registers step handlers successfully' do
      expect { handle.register_step_handlers }.not_to raise_error
    end

    it 'creates test task with linear pattern' do
      result = handle.create_test_task(factory_options)
      
      expect(result).to respond_to(:task_id)
      expect(result.task_id).to be > 0
    end
  end

  describe 'Step Execution and Results Persistence' do
    let(:task_result) { handle.create_test_task(factory_options) }
    let(:task_id) { task_result.task_id }

    context 'first step execution' do
      let(:step_result) { handle.handle_one_step_by_name(task_id, "validate_order") }

      it 'executes first step successfully' do
        skip "handle_one_step_by_name not yet returning proper status - planned for Phase 3"
        expect(step_result).to respond_to(:status)
        expect(step_result).to respond_to(:step_id)
        
        expect(step_result.step_id).to be > 0
        expect(step_result.status).to be_in(%w[complete completed success])
      end

      it 'persists step results to database' do
        skip "handle_one_step_by_name not yet returning proper status - planned for Phase 3"
        # Execute first step
        first_result = step_result
        expect(first_result.status).to be_in(%w[complete completed success])
        
        # Results should be accessible for subsequent steps
        expect(first_result.step_id).to be_a(Integer)
      end
    end

    context 'subsequent step execution' do
      before do
        # Execute first step to create baseline results
        handle.handle_one_step_by_name(task_id, "validate_order")
      end

      it 'accesses persisted results from previous steps' do
        skip "handle_one_step_by_name dependency access not yet implemented - planned for Phase 3"
        # Execute second step which should have access to first step results
        next_step_result = handle.handle_one_step_by_name(task_id, "reserve_inventory")
        
        expect(next_step_result).to respond_to(:status)
        expect(next_step_result.status).to be_in(%w[complete completed success error failed])
      end

      it 'maintains step execution sequence' do
        skip "handle_one_step_by_name dependency sequencing not yet implemented - planned for Phase 3"
        # In a linear pattern, steps should execute in dependency order
        second_result = handle.handle_one_step_by_name(task_id, "reserve_inventory")
        expect(second_result.step_id).to be > 0
        
        # Third step should also be executable
        third_result = handle.handle_one_step_by_name(task_id, "process_payment")
        expect(third_result.step_id).to be > 0
      end
    end
  end

  describe 'Results Chain Validation' do
    let(:task_result) { handle.create_test_task(factory_options) }
    let(:task_id) { task_result.task_id }

    it 'maintains results across step execution chain' do
      skip "handle_one_step_by_name chain execution not yet implemented - planned for Phase 3"
      # Execute steps in sequence and verify each can access previous results
      steps = ["validate_order", "reserve_inventory", "process_payment", "ship_order"]
      
      executed_steps = []
      
      steps.each do |step_name|
        begin
          result = handle.handle_one_step_by_name(task_id, step_name)
          executed_steps << {
            step_name: step_name,
            step_id: result.step_id,
            status: result.status
          }
          
          expect(result.step_id).to be > 0
          expect(result.status).to be_in(%w[complete completed success error failed])
        rescue => e
          # Some steps may fail due to dependencies or business logic
          # but they should fail gracefully, not due to missing results
          expect(e.message).not_to include("results not found")
          expect(e.message).not_to include("persistence")
        end
      end
      
      # At least the first step should have executed successfully
      expect(executed_steps.size).to be >= 1
      expect(executed_steps.first[:status]).to be_in(%w[complete completed success])
    end
  end

  describe 'Database Persistence Validation' do
    let(:task_result) { handle.create_test_task(factory_options) }
    let(:task_id) { task_result.task_id }

    it 'persists step results in database for cross-step access' do
      skip "handle_one_step_by_name database persistence not yet implemented - planned for Phase 3"
      # Execute first step
      first_result = handle.handle_one_step_by_name(task_id, "validate_order")
      first_step_id = first_result.step_id
      
      # Verify results are persisted by executing dependent step
      expect {
        handle.handle_one_step_by_name(task_id, "reserve_inventory")
      }.not_to raise_error(/results.*not.*found|persistence.*error/i)
    end

    it 'supports step result retrieval across execution context' do
      skip "handle_one_step_by_name cross-execution retrieval not yet implemented - planned for Phase 3"
      # This test ensures that step results are properly stored
      # and accessible to subsequent steps in the workflow
      
      first_result = handle.handle_one_step_by_name(task_id, "validate_order")
      expect(first_result.status).to be_in(%w[complete completed success])
      
      # Second step should be able to proceed (accessing first step results)
      second_result = handle.handle_one_step_by_name(task_id, "reserve_inventory") 
      expect(second_result.step_id).to be > 0
    end
  end

  describe 'Error Handling for Missing Results' do
    it 'handles gracefully when results are not available' do
      skip "handle_one_step_by_name error handling not yet implemented - planned for Phase 3"
      # This test ensures proper error handling rather than silent failures
      # when step results are not properly persisted
      
      invalid_task_id = -1
      
      expect {
        handle.handle_one_step_by_name(invalid_task_id, "validate_order")
      }.to raise_error(an_instance_of(StandardError))
    end
  end
end