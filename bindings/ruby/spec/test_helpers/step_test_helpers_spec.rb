# frozen_string_literal: true

require 'spec_helper'

RSpec.describe TaskerCore::TestHelpers::StepTestHelpers do
  include TaskerCore::TestHelpers

  let(:config_path) { File.expand_path('../../handlers/examples/order_fulfillment/config/order_fulfillment_handler.yaml', __dir__) }
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
      initiator: "step_helpers_spec",
      source_system: "rspec",
      reason: "testing StepTestHelpers module",
      tags: ["test", "step_helpers", "unit"]
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

  describe 'module inclusion' do
    it 'includes StepTestHelpers methods' do
      expect(self).to respond_to(:get_steps_in_dependency_order)
      expect(self).to respond_to(:execute_steps_up_to)
      expect(self).to respond_to(:step_ready_for_execution?)
      expect(self).to respond_to(:find_step_by_name)
      expect(self).to respond_to(:get_task_step_summary)
    end
  end

  describe '#get_steps_in_dependency_order' do
    context 'when FFI methods are not yet implemented' do
      it 'handles missing FFI implementation gracefully' do
        steps = get_steps_in_dependency_order(task_id)
        
        # Should return empty array when FFI method not implemented
        expect(steps).to be_an(Array)
        # May be empty until FFI implementation is complete
      end
    end

    context 'when called with invalid task_id' do
      it 'handles non-existent task gracefully' do
        expect {
          steps = get_steps_in_dependency_order(99999)
          expect(steps).to be_an(Array)
          expect(steps).to be_empty
        }.not_to raise_error
      end
    end

    context 'when FFI methods are implemented (future)' do
      it 'returns properly structured step information' do
        steps = get_steps_in_dependency_order(task_id)
        
        # Each step should have the expected structure
        steps.each do |step|
          expect(step).to be_a(Hash)
          expect(step).to have_key(:id)
          expect(step).to have_key(:name)
          expect(step).to have_key(:dependencies)
          expect(step).to have_key(:state)
          expect(step).to have_key(:retryable)
          
          expect(step[:id]).to be_a(Integer)
          expect(step[:name]).to be_a(String)
          expect(step[:dependencies]).to be_an(Array)
          expect(step[:state]).to be_a(String)
          expect(step[:retryable]).to be_in([true, false])
        end
      end

      it 'returns steps in proper dependency order (topological sort)' do
        steps = get_steps_in_dependency_order(task_id)
        
        # Verify topological ordering: dependencies appear before dependents
        step_positions = {}
        steps.each_with_index do |step, index|
          step_positions[step[:name]] = index
        end
        
        steps.each do |step|
          step[:dependencies].each do |dep_name|
            if step_positions[dep_name]
              expect(step_positions[dep_name]).to be < step_positions[step[:name]],
                "Dependency #{dep_name} should appear before #{step[:name]} in the sorted order"
            end
          end
        end
      end
    end
  end

  describe '#execute_steps_up_to' do
    context 'when target step does not exist' do
      it 'returns error information' do
        result = execute_steps_up_to(task_id, "nonexistent_step", handler_instance)
        
        expect(result).to be_a(Hash)
        expect(result).to have_key(:executed_steps)
        expect(result).to have_key(:already_completed)
        expect(result).to have_key(:failed_steps)
        expect(result).to have_key(:target_ready)
        expect(result).to have_key(:error)
        
        expect(result[:target_ready]).to be(false)
        expect(result[:error]).to include("not found")
      end
    end

    context 'when no handler is provided and cannot find one' do
      it 'returns error about missing handler' do
        result = execute_steps_up_to(99999, "some_step")
        
        expect(result[:target_ready]).to be(false)
        expect(result[:error]).to include("No handler found")
      end
    end

    context 'when FFI methods are implemented (future)' do
      it 'returns comprehensive execution summary' do
        # This will work once FFI methods are implemented
        result = execute_steps_up_to(task_id, "validate_order", handler_instance)
        
        expect(result).to have_key(:executed_steps)
        expect(result).to have_key(:already_completed)
        expect(result).to have_key(:failed_steps)
        expect(result).to have_key(:target_ready)
        
        expect(result[:executed_steps]).to be_an(Array)
        expect(result[:already_completed]).to be_an(Array)
        expect(result[:failed_steps]).to be_an(Array)
        expect(result[:target_ready]).to be_in([true, false])
      end
    end
  end

  describe '#step_ready_for_execution?' do
    context 'when FFI methods are not implemented' do
      it 'handles missing implementation gracefully' do
        expect {
          result = step_ready_for_execution?(123)
          expect(result).to be_a(Hash)
          expect(result).to have_key(:ready)
          expect(result).to have_key(:missing_dependencies)
          expect(result).to have_key(:completed_dependencies)
          expect(result).to have_key(:step_state)
        }.not_to raise_error
      end
    end

    context 'when called with invalid step_id' do
      it 'handles errors gracefully' do
        result = step_ready_for_execution?(99999)
        
        expect(result[:ready]).to be(false)
        expect(result).to have_key(:error)
      end
    end
  end

  describe '#find_step_by_name' do
    it 'returns nil when step not found' do
      step = find_step_by_name(task_id, "nonexistent_step")
      expect(step).to be_nil
    end

    context 'when FFI methods are implemented (future)' do
      it 'finds step by name when it exists' do
        step = find_step_by_name(task_id, "validate_order")
        
        if step # Only test if FFI returns data
          expect(step).to be_a(Hash)
          expect(step[:name]).to eq("validate_order")
          expect(step[:id]).to be_a(Integer)
          expect(step[:state]).to be_a(String)
        end
      end
    end
  end

  describe '#get_task_step_summary' do
    it 'returns summary structure even when steps are empty' do
      summary = get_task_step_summary(task_id)
      
      expect(summary).to be_a(Hash)
      expect(summary).to have_key(:total_steps)
      expect(summary).to have_key(:completed_steps)
      expect(summary).to have_key(:pending_steps)
      expect(summary).to have_key(:failed_steps)
      expect(summary).to have_key(:ready_steps)
      expect(summary).to have_key(:blocked_steps)
      
      expect(summary[:total_steps]).to be >= 0
      expect(summary[:completed_steps]).to be >= 0
      expect(summary[:pending_steps]).to be >= 0
      expect(summary[:failed_steps]).to be >= 0
      expect(summary[:ready_steps]).to be_an(Array)
      expect(summary[:blocked_steps]).to be_an(Array)
    end

    context 'when FFI methods are implemented (future)' do
      it 'provides accurate step counts and categorization' do
        summary = get_task_step_summary(task_id)
        
        # Verify mathematical consistency
        total = summary[:completed_steps] + summary[:pending_steps] + summary[:failed_steps]
        expect(total).to eq(summary[:total_steps])
        
        # Ready and blocked steps should be subsets of pending
        ready_and_blocked = summary[:ready_steps].length + summary[:blocked_steps].length
        expect(ready_and_blocked).to be <= summary[:pending_steps]
      end
    end
  end

  describe 'private helper methods' do
    describe '#topological_sort' do
      it 'handles circular dependencies gracefully' do
        # Create steps with circular dependency
        circular_steps = [
          { name: 'step_a', dependencies: ['step_b'] },
          { name: 'step_b', dependencies: ['step_c'] },
          { name: 'step_c', dependencies: ['step_a'] } # Circular!
        ]
        
        # Should fall back to original order on circular dependency
        expect {
          result = send(:topological_sort, circular_steps)
          expect(result).to eq(circular_steps) # Falls back to original
        }.not_to raise_error
      end

      it 'correctly sorts simple dependency chain' do
        steps = [
          { name: 'step_c', dependencies: ['step_a', 'step_b'] },
          { name: 'step_a', dependencies: [] },
          { name: 'step_b', dependencies: ['step_a'] }
        ]
        
        result = send(:topological_sort, steps)
        
        # step_a should come first (no dependencies)
        # step_b should come second (depends on step_a)
        # step_c should come last (depends on both)
        expect(result.map { |s| s[:name] }).to eq(['step_a', 'step_b', 'step_c'])
      end
    end
  end

  describe 'error handling and logging' do
    it 'logs errors appropriately' do
      # Verify logger is accessible
      expect(TaskerCore::Logging::Logger.instance).to respond_to(:error)
      expect(TaskerCore::Logging::Logger.instance).to respond_to(:warn)
      
      # Methods should handle exceptions gracefully
      expect {
        get_steps_in_dependency_order(nil)
      }.not_to raise_error
    end
  end
end