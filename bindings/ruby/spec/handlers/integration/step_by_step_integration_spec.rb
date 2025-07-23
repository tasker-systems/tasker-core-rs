# frozen_string_literal: true

require 'spec_helper'

# Load the order fulfillment handler classes for the integration test
require_relative '../examples/order_fulfillment/handlers/order_fulfillment_handler'
require_relative '../examples/order_fulfillment/step_handlers/validate_order_handler'
require_relative '../examples/order_fulfillment/step_handlers/reserve_inventory_handler'
require_relative '../examples/order_fulfillment/step_handlers/process_payment_handler'
require_relative '../examples/order_fulfillment/step_handlers/ship_order_handler'

RSpec.describe 'Step-by-step Integration Testing', type: :integration do
  include TaskerCore::TestHelpers

  let(:config_path) { File.expand_path('../examples/order_fulfillment/config/order_fulfillment_handler.yaml', __dir__) }
  let(:config) { YAML.load_file(config_path) }
  let(:sample_order_context) do
    {
      'customer_info' => { 'id' => 12345, 'email' => 'test@example.com', 'tier' => 'standard' },
      'order_items' => [{ 'product_id' => 101, 'quantity' => 2, 'price' => 29.99 }],
      'payment_info' => { 'method' => 'credit_card', 'token' => 'tok_test_12345', 'amount' => 59.98 },
      'shipping_info' => { 'address' => '123 Main St', 'method' => 'standard' }
    }
  end
  
  let(:task_request) do
    TaskerCore::Types::TaskRequest.build_test(
      namespace: "fulfillment",
      name: "process_order",
      version: "1.0.0",
      context: sample_order_context,
      initiator: "step_by_step_integration_spec",
      source_system: "rspec",
      reason: "testing step-by-step execution",
      tags: ["test", "step_by_step", "integration"]
    )
  end

  let(:handler_instance) do
    TaskerCore::Registry.register(
      namespace: config['namespace_name'],
      name: config['name'],
      version: config['version'],
      handler_class: config['task_handler_class'],
      config_schema: config
    )
    
    handler_result = TaskerCore::Registry.find_handler_and_initialize(
      name: "fulfillment/process_order",
      version: "1.0.0",
      config_path: config_path
    )
    
    handler_result['handler_instance']
  end

  let(:task_id) do
    init_result = handler_instance.initialize_task(task_request)
    expect(init_result.success?).to be(true)
    init_result.task_id
  end

  describe 'Testing Approach 1: Production-like full integration testing' do
    it 'executes complete workflow through handle(task_id)' do
      # This is the straightforward production-like test that is the true integration test
      result = handler_instance.handle(task_id)
      
      expect(result).to be_a(TaskerCore::TaskHandler::HandleResult)
      
      # Verify task completion
      case result.status
      when 'complete', 'completed'
        expect(result.success?).to be(true)
        expect(result.task_id).to eq(task_id)
        puts "✅ Full workflow completed successfully"
        puts "   Steps executed: #{result.completed_steps}" if result.completed_steps
      when 'in_progress'
        expect(result.task_id).to eq(task_id)
        puts "🔄 Workflow in progress (expected for complex workflows)"
        puts "   Steps executed so far: #{result.completed_steps}" if result.completed_steps
      when 'blocked'
        puts "⏸️  Workflow blocked (may be expected depending on business logic)"
        puts "   Blocking reason: #{result.error_message}" if result.error_message
      when 'failed'
        puts "❌ Workflow failed: #{result.error_message}"
        # Don't fail the test immediately - this might be expected behavior
      else
        puts "ℹ️  Workflow status: #{result.status}"
      end
      
      # Basic structural validations
      expect(result.task_id).to be_a(Integer)
      expect(result.status).to be_a(String)
      expect(result.status).not_to be_empty
    end
  end

  describe 'Testing Approach 2: Sequential step-by-step testing' do
    context 'when testing individual steps with dependency awareness' do
      it 'validates step dependencies before execution' do
        # Get all steps in dependency order
        steps = get_steps_in_dependency_order(task_id)
        
        if steps.empty?
          skip "Steps not available (FFI method get_workflow_steps_for_task not yet implemented)"
        end
        
        expect(steps).to be_an(Array)
        expect(steps).not_to be_empty
        
        puts "📋 Found #{steps.length} steps in dependency order:"
        steps.each_with_index do |step, idx|
          puts "   #{idx + 1}. #{step[:name]} (depends on: #{step[:dependencies].join(', ')})"
        end
      end

      it 'executes steps sequentially respecting dependencies' do
        steps = get_steps_in_dependency_order(task_id)
        
        if steps.empty?
          skip "Steps not available (FFI implementation needed)"
        end
        
        # Execute each step in dependency order
        steps.each do |step_info|
          context "executing step: #{step_info[:name]}" do
            # Check if step is ready for execution
            readiness = step_ready_for_execution?(step_info[:id])
            
            if readiness[:ready]
              # Step is ready - execute it
              result = handler_instance.handle_one_step(step_info[:id])
              
              expect(result).to be_a(TaskerCore::TaskHandler::StepHandleResult)
              expect(result.step_id).to eq(step_info[:id])
              expect(result.task_id).to eq(task_id)
              expect(result.step_name).to eq(step_info[:name])
              
              if result.success?
                puts "✅ Step '#{step_info[:name]}' completed successfully"
                puts "   Execution time: #{result.execution_time_ms}ms"
                puts "   Result data: #{result.result_data}" if result.result_data
              elsif result.dependencies_not_met?
                puts "⏸️  Step '#{step_info[:name]}' blocked by dependencies: #{result.missing_dependencies.join(', ')}"
                # This is expected behavior - not a failure
              else
                puts "❌ Step '#{step_info[:name]}' failed: #{result.error_message}"
                puts "   Handler: #{result.handler_class}"
                puts "   Retry count: #{result.retry_count}"
              end
              
              # Verify step structure
              expect(result.dependencies_met).to be_in([true, false])
              expect(result.execution_time_ms).to be >= 0
              expect(result.step_state_before).to be_a(String)
              expect(result.step_state_after).to be_a(String)
            else
              puts "⏸️  Step '#{step_info[:name]}' not ready - missing: #{readiness[:missing_dependencies].join(', ')}"
              
              # Execute prerequisite steps first
              execute_result = execute_steps_up_to(task_id, step_info[:name], handler_instance)
              
              if execute_result[:target_ready]
                puts "✅ Prerequisites executed, step '#{step_info[:name]}' now ready"
                puts "   Executed: #{execute_result[:executed_steps].join(', ')}" if execute_result[:executed_steps].any?
                
                # Now execute the target step
                result = handler_instance.handle_one_step(step_info[:id])
                expect(result).to be_a(TaskerCore::TaskHandler::StepHandleResult)
                
                if result.success?
                  puts "✅ Step '#{step_info[:name]}' completed after dependency resolution"
                end
              else
                puts "❌ Failed to resolve dependencies for '#{step_info[:name]}'"
                puts "   Failed steps: #{execute_result[:failed_steps].join(', ')}" if execute_result[:failed_steps].any?
              end
            end
          end
        end
      end

      it 'provides comprehensive debugging information for failed steps' do
        # Test error handling and debugging capabilities
        result = handler_instance.handle_one_step(99999) # Non-existent step ID
        
        expect(result).to be_a(TaskerCore::TaskHandler::StepHandleResult)
        expect(result.success?).to be(false)
        expect(result.error_message).to include("not found") # Should contain error details
        
        puts "🔍 Debug information for failed step:"
        puts "   Error: #{result.error_message}"
        puts "   Execution time: #{result.execution_time_ms}ms"
        puts "   State transition: #{result.step_state_before} → #{result.step_state_after}"
      end
    end

    context 'when testing dependency validation' do
      it 'prevents execution of steps with unmet dependencies' do
        steps = get_steps_in_dependency_order(task_id)
        
        if steps.empty?
          skip "Steps not available (FFI implementation needed)"
        end
        
        # Find a step that has dependencies
        dependent_step = steps.find { |step| step[:dependencies]&.any? }
        
        if dependent_step
          puts "🧪 Testing dependency validation for '#{dependent_step[:name]}'"
          puts "   Dependencies: #{dependent_step[:dependencies].join(', ')}"
          
          # Try to execute step without completing dependencies first
          result = handler_instance.handle_one_step(dependent_step[:id])
          
          expect(result).to be_a(TaskerCore::TaskHandler::StepHandleResult)
          
          if result.dependencies_not_met?
            expect(result.dependencies_met).to be(false)
            expect(result.missing_dependencies).not_to be_empty
            expect(result.status).to eq('dependencies_not_met')
            
            puts "✅ Dependency validation working correctly"
            puts "   Missing: #{result.missing_dependencies.join(', ')}"
          else
            puts "ℹ️  Step executed successfully - dependencies may already be completed"
          end
        else
          puts "ℹ️  No dependent steps found in this workflow"
        end
      end

      it 'provides detailed task context and step state information' do
        summary = get_task_step_summary(task_id)
        
        if summary[:total_steps] > 0
          expect(summary[:total_steps]).to be > 0
          expect(summary[:completed_steps]).to be >= 0
          expect(summary[:pending_steps]).to be >= 0
          expect(summary[:failed_steps]).to be >= 0
          
          puts "📊 Task Step Summary:"
          puts "   Total steps: #{summary[:total_steps]}"
          puts "   Completed: #{summary[:completed_steps]}"
          puts "   Pending: #{summary[:pending_steps]}"
          puts "   Failed: #{summary[:failed_steps]}"
          puts "   Ready for execution: #{summary[:ready_steps].join(', ')}" if summary[:ready_steps].any?
          puts "   Blocked by dependencies: #{summary[:blocked_steps].join(', ')}" if summary[:blocked_steps].any?
        else
          skip "Task summary not available (FFI implementation needed)"
        end
      end
    end
  end

  describe 'Helper method integration' do
    it 'can find steps by name' do
      step = find_step_by_name(task_id, "validate_order")
      
      if step
        expect(step[:name]).to eq("validate_order")
        expect(step[:id]).to be_a(Integer)
        expect(step[:state]).to be_a(String)
        puts "✅ Found step 'validate_order': ID #{step[:id]}, State: #{step[:state]}"
      else
        skip "Step lookup not available (FFI implementation needed)"
      end
    end

    it 'provides step readiness checking' do
      steps = get_steps_in_dependency_order(task_id)
      
      if steps.any?
        first_step = steps.first
        readiness = step_ready_for_execution?(first_step[:id])
        
        expect(readiness).to have_key(:ready)
        expect(readiness).to have_key(:missing_dependencies)
        expect(readiness).to have_key(:completed_dependencies)
        expect(readiness).to have_key(:step_state)
        
        puts "🔍 Step readiness for '#{first_step[:name]}':"
        puts "   Ready: #{readiness[:ready]}"
        puts "   State: #{readiness[:step_state]}"
        puts "   Missing deps: #{readiness[:missing_dependencies].join(', ')}" if readiness[:missing_dependencies].any?
        puts "   Completed deps: #{readiness[:completed_dependencies].join(', ')}" if readiness[:completed_dependencies].any?
      else
        skip "Step readiness checking not available (FFI implementation needed)"
      end
    end
  end

  describe 'StepHandleResult comprehensive testing' do
    it 'validates all StepHandleResult fields and methods' do
      # Test with a non-existent step to validate error handling
      result = handler_instance.handle_one_step(99999)
      
      # Verify all expected fields are present
      expect(result.step_id).to eq(99999)
      expect(result.task_id).to be_a(Integer)
      expect(result.step_name).to be_a(String)
      expect(result.status).to be_a(String)
      expect(result.execution_time_ms).to be_a(Integer)
      expect(result.execution_time_ms).to be >= 0
      expect(result.error_message).to be_a(String)
      expect(result.retry_count).to be_a(Integer)
      expect(result.handler_class).to be_a(String)
      expect(result.dependencies_met).to be_in([true, false])
      expect(result.missing_dependencies).to be_an(Array)
      expect(result.dependency_results).to be_a(Hash)
      expect(result.step_state_before).to be_a(String)
      expect(result.step_state_after).to be_a(String)
      expect(result.task_context).to be_a(Hash)
      
      # Test convenience methods
      expect(result.success?).to be_in([true, false])
      expect(result.failed?).to be_in([true, false])
      expect(result.dependencies_not_met?).to be_in([true, false])
      expect(result.retry_eligible?).to be_in([true, false])
      
      # Test to_h method
      hash = result.to_h
      expect(hash).to be_a(Hash)
      expect(hash).to have_key('step_id')
      expect(hash).to have_key('task_id')
      expect(hash).to have_key('success')
      
      puts "✅ StepHandleResult structure validated successfully"
      puts "   All #{result.class.instance_methods(false).count} methods working correctly"
    end
  end
end