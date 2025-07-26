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

  describe '#get_steps_in_dependency_order (MODERNIZED - ZeroMQ Architecture)' do
    it 'returns deprecation warning and compatibility data for modern ZeroMQ architecture' do
      steps = get_steps_in_dependency_order(task_id)
      
      # Should return minimal compatibility array with modern architecture guidance
      expect(steps).to be_an(Array)
      
      # In modern architecture, we get minimal compatibility data with step mock
      if steps.any?
        step = steps.first
        expect(step).to be_a(Hash)
        expect(step).to have_key(:id)
        expect(step).to have_key(:name)
        expect(step).to have_key(:state)
        expect(step[:name]).to eq("workflow_batch_execution")
        expect(step[:state]).to eq("zeromq_managed")
      end
    end

    it 'handles invalid task_id gracefully with empty array' do
      expect {
        steps = get_steps_in_dependency_order(99999)
        expect(steps).to be_an(Array)
        expect(steps).to be_empty
      }.not_to raise_error
    end

    it 'logs deprecation warning about obsolete step-by-step inspection' do
      expect(TaskerCore::Logging::Logger.instance).to receive(:warn)
        .with(/get_steps_in_dependency_order is obsolete in ZeroMQ architecture/)
      
      get_steps_in_dependency_order(task_id)
    end
  end

  describe '#execute_steps_up_to (MODERNIZED - ZeroMQ Architecture)' do
    it 'returns compatibility response indicating ZeroMQ batch management' do
      result = execute_steps_up_to(task_id, "validate_order", handler_instance)
      
      expect(result).to be_a(Hash)
      expect(result).to have_key(:executed_steps)
      expect(result).to have_key(:already_completed)
      expect(result).to have_key(:failed_steps)
      expect(result).to have_key(:target_ready)
      expect(result).to have_key(:error)
      expect(result).to have_key(:modern_approach)
      
      expect(result[:target_ready]).to be(false)
      expect(result[:error]).to include("ZeroMQ batch orchestration")
      expect(result[:modern_approach]).to have_key(:architecture)
      expect(result[:modern_approach][:architecture]).to include("ZeroMQ batch execution")
    end

    it 'logs deprecation warning about obsolete manual step execution' do
      expect(TaskerCore::Logging::Logger.instance).to receive(:warn)
        .with(/execute_steps_up_to is obsolete in ZeroMQ architecture/)
      
      execute_steps_up_to(task_id, "some_step")
    end

    it 'logs debug information about legacy method usage' do
      expect(TaskerCore::Logging::Logger.instance).to receive(:debug)
        .with(/Legacy execute_steps_up_to called: task_id=\d+, target_step=validate_order/)
      
      execute_steps_up_to(task_id, "validate_order")
    end
  end

  describe '#step_ready_for_execution? (MODERNIZED - ZeroMQ Architecture)' do
    it 'returns compatibility response indicating ZeroMQ management' do
      result = step_ready_for_execution?(123)
      
      expect(result).to be_a(Hash)
      expect(result).to have_key(:ready)
      expect(result).to have_key(:missing_dependencies)
      expect(result).to have_key(:completed_dependencies)
      expect(result).to have_key(:step_state)
      expect(result).to have_key(:error)
      expect(result).to have_key(:modern_approach)
      
      expect(result[:ready]).to be(false)
      expect(result[:step_state]).to eq("zeromq_managed")
      expect(result[:error]).to include("ZeroMQ batch orchestration")
    end

    it 'logs deprecation warning about obsolete step readiness checking' do
      expect(TaskerCore::Logging::Logger.instance).to receive(:warn)
        .with(/step_ready_for_execution\? is obsolete in ZeroMQ architecture/)
      
      step_ready_for_execution?(123)
    end
  end

  describe '#find_step_by_name (MODERNIZED - ZeroMQ Architecture)' do
    it 'always returns nil with deprecation warning in modern architecture' do
      step = find_step_by_name(task_id, "validate_order")
      expect(step).to be_nil
    end

    it 'logs deprecation warning about obsolete individual step lookup' do
      expect(TaskerCore::Logging::Logger.instance).to receive(:warn)
        .with(/find_step_by_name is obsolete in ZeroMQ architecture/)
      
      find_step_by_name(task_id, "validate_order")
    end

    it 'logs debug information about legacy method usage' do
      expect(TaskerCore::Logging::Logger.instance).to receive(:debug)
        .with(/Legacy find_step_by_name called: task_id=\d+, step_name=validate_order/)
      
      find_step_by_name(task_id, "validate_order")
    end
  end

  describe '#get_task_step_summary (MODERNIZED - ZeroMQ Architecture)' do
    it 'returns compatibility summary indicating ZeroMQ management' do
      summary = get_task_step_summary(task_id)
      
      expect(summary).to be_a(Hash)
      expect(summary).to have_key(:total_steps)
      expect(summary).to have_key(:completed_steps)
      expect(summary).to have_key(:pending_steps)
      expect(summary).to have_key(:failed_steps)
      expect(summary).to have_key(:ready_steps)
      expect(summary).to have_key(:blocked_steps)
      expect(summary).to have_key(:zeromq_managed)
      expect(summary).to have_key(:error)
      expect(summary).to have_key(:modern_approach)
      
      # In modern architecture, all counts are 0 (obsolete concept)
      expect(summary[:total_steps]).to eq(0)
      expect(summary[:completed_steps]).to eq(0)
      expect(summary[:pending_steps]).to eq(0)
      expect(summary[:failed_steps]).to eq(0)
      expect(summary[:ready_steps]).to be_empty
      expect(summary[:blocked_steps]).to be_empty
      expect(summary[:zeromq_managed]).to be(true)
      expect(summary[:error]).to include("ZeroMQ batch orchestration")
    end

    it 'logs deprecation warning about obsolete step summaries' do
      expect(TaskerCore::Logging::Logger.instance).to receive(:warn)
        .with(/get_task_step_summary is obsolete in ZeroMQ architecture/)
      
      get_task_step_summary(task_id)
    end
  end

  describe 'private helper methods (MODERNIZED - ZeroMQ Architecture)' do
    describe '#topological_sort (OBSOLETE)' do
      it 'returns input as-is with debug logging in modern architecture' do
        circular_steps = [
          { name: 'step_a', dependencies: ['step_b'] },
          { name: 'step_b', dependencies: ['step_c'] },
          { name: 'step_c', dependencies: ['step_a'] }
        ]
        
        expect(TaskerCore::Logging::Logger.instance).to receive(:debug)
          .with(/topological_sort is obsolete/)
        
        result = send(:topological_sort, circular_steps)
        expect(result).to eq(circular_steps) # Returns input as-is
      end

      it 'handles any input gracefully in modern architecture' do
        steps = [
          { name: 'step_c', dependencies: ['step_a', 'step_b'] },
          { name: 'step_a', dependencies: [] },
          { name: 'step_b', dependencies: ['step_a'] }
        ]
        
        expect(TaskerCore::Logging::Logger.instance).to receive(:debug)
          .with(/topological_sort is obsolete/)
        
        result = send(:topological_sort, steps)
        expect(result).to eq(steps) # Returns input as-is in modern architecture
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

  describe 'Modern ZeroMQ Architecture Guidance' do
    it 'demonstrates modern task creation and batch execution approach' do
      # MODERN APPROACH: Use orchestration handle for task creation
      handle = TaskerCore.create_orchestration_handle
      expect(handle).not_to be_nil
      
      # Task creation with batch execution happens automatically
      # Individual step inspection is obsolete - use batch results instead
      task_result = handle.create_test_task({
        "task_name" => "OrderFulfillmentTaskHandler",
        "step_count" => 4,
        "pattern" => "linear"
      })
      
      # Verify modern approach provides task-level information
      expect(task_result.task_id).to be > 0
      expect(task_result.step_count).to be > 0
    end

    it 'shows ZeroMQ integration status and capabilities' do
      # Check ZeroMQ integration status
      manager = TaskerCore::Internal::OrchestrationManager.instance
      status = manager.zeromq_integration_status
      
      expect(status).to have_key(:enabled)
      # ZeroMQ integration provides modern batch processing capabilities
    end
  end
end