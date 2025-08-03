# frozen_string_literal: true

require 'spec_helper'

RSpec.describe TaskerCore::Types::StepMessage do
  describe 'StepDependencyResult' do
    let(:dependency_result) do
      TaskerCore::Types::StepDependencyResult.new(
        step_name: 'validate_order',
        step_id: 123,
        named_step_id: 1,
        results: { status: 'validated', order_id: 1001 },
        processed_at: Time.now.utc,
        metadata: { attempts: 2, retryable: true }
      )
    end

    it 'creates dependency result with proper attributes' do
      expect(dependency_result.step_name).to eq('validate_order')
      expect(dependency_result.step_id).to eq(123)
      expect(dependency_result.named_step_id).to eq(1)
      expect(dependency_result.results).to eq(status: 'validated', order_id: 1001)
      expect(dependency_result.metadata).to eq(attempts: 2, retryable: true)
    end

    it 'adds metadata with with_metadata method' do
      updated = dependency_result.with_metadata('execution_time_ms', 1500)
      expect(updated.metadata).to include(
        attempts: 2,
        retryable: true,
        'execution_time_ms' => 1500
      )
    end
  end

  describe 'DependencyChain' do
    let(:dep1) do
      TaskerCore::Types::StepDependencyResult.new(
        step_name: 'validate_order',
        step_id: 123,
        named_step_id: 1,
        results: { status: 'validated', order_id: 1001 }
      )
    end

    let(:dep2) do
      TaskerCore::Types::StepDependencyResult.new(
        step_name: 'check_inventory',
        step_id: 124,
        named_step_id: 2,
        results: { status: 'available', quantity: 5 }
      )
    end

    let(:dependency_chain) { TaskerCore::Types::DependencyChain.new([dep1, dep2]) }

    it 'provides convenient access to dependencies by name' do
      validate_step = dependency_chain.get('validate_order')
      expect(validate_step).to eq(dep1)
      expect(validate_step.results).to eq(status: 'validated', order_id: 1001)

      inventory_step = dependency_chain['check_inventory']
      expect(inventory_step).to eq(dep2)
      expect(inventory_step.results).to eq(status: 'available', quantity: 5)
    end

    it 'returns nil for non-existent dependencies' do
      expect(dependency_chain.get('non_existent')).to be_nil
      expect(dependency_chain['not_found']).to be_nil
    end

    it 'provides collection methods' do
      expect(dependency_chain.count).to eq(2)
      expect(dependency_chain.names).to eq(['validate_order', 'check_inventory'])
      expect(dependency_chain.has?('validate_order')).to be true
      expect(dependency_chain.include?('not_found')).to be false
      expect(dependency_chain.empty?).to be false
    end

    it 'supports iteration' do
      names = []
      dependency_chain.each { |dep| names << dep.step_name }
      expect(names).to eq(['validate_order', 'check_inventory'])
    end

    it 'converts to array and hash' do
      expect(dependency_chain.to_a).to eq([dep1, dep2])
      expect(dependency_chain.to_h).to eq(
        dependencies: [dep1.to_h, dep2.to_h],
        count: 2
      )
    end

    it 'handles hash input' do
      hash_deps = [
        { step_name: 'hash_step', step_id: 999, named_step_id: 3, results: { data: 'test' } }
      ]
      chain = TaskerCore::Types::DependencyChain.new(hash_deps)
      
      step = chain.get('hash_step')
      expect(step).to be_a(TaskerCore::Types::StepDependencyResult)
      expect(step.step_name).to eq('hash_step')
      expect(step.results).to eq(data: 'test')
    end
  end

  describe 'StepExecutionContext' do
    let(:task_data) { { task_id: 67890, namespace: 'fulfillment' } }
    let(:step_data) { { step_id: 12345, step_name: 'process_payment' } }
    
    let(:dependencies) do
      [
        TaskerCore::Types::StepDependencyResult.new(
          step_name: 'validate_order',
          step_id: 123,
          named_step_id: 1,
          results: { status: 'validated' }
        ),
        TaskerCore::Types::StepDependencyResult.new(
          step_name: 'check_inventory',
          step_id: 124,
          named_step_id: 2,
          results: { status: 'available' }
        )
      ]
    end

    context 'with dependencies' do
      let(:context) do
        TaskerCore::Types::StepExecutionContext.new(
          task: task_data,
          sequence: dependencies,
          step: step_data
        )
      end

      it 'provides access to task, step, and dependencies' do
        expect(context.task).to eq(task_data)
        expect(context.step).to eq(step_data)
        expect(context.sequence).to eq(dependencies)
      end

      it 'provides convenient dependency access via wrapper' do
        deps = context.dependencies
        expect(deps).to be_a(TaskerCore::Types::DependencyChain)
        expect(deps.count).to eq(2)
        
        validate_step = deps.get('validate_order')
        expect(validate_step.results).to eq(status: 'validated')
        
        inventory_step = deps['check_inventory']
        expect(inventory_step.results).to eq(status: 'available')
      end

      it 'adds additional context' do
        updated = context.with_context('custom_key', 'custom_value')
        expect(updated.additional_context).to eq('custom_key' => 'custom_value')
      end
    end

    context 'root step (no dependencies)' do
      let(:root_context) do
        TaskerCore::Types::StepExecutionContext.new_root_step(
          task: task_data,
          step: step_data
        )
      end

      it 'creates context with empty dependencies' do
        expect(root_context.task).to eq(task_data)
        expect(root_context.step).to eq(step_data)
        expect(root_context.sequence).to be_empty
        expect(root_context.dependencies.empty?).to be true
      end
    end
  end

  describe 'StepMessage with execution context' do
    let(:task_data) { { task_id: 67890, namespace: 'fulfillment' } }
    let(:step_data) { { step_id: 12345, step_name: 'process_payment' } }
    
    let(:execution_context) do
      TaskerCore::Types::StepExecutionContext.new_root_step(
        task: task_data,
        step: step_data
      )
    end

    let(:step_message) do
      TaskerCore::Types::StepMessage.build_test(
        step_id: 12345,
        task_id: 67890,
        namespace: 'fulfillment',
        task_name: 'process_order',
        step_name: 'process_payment',
        execution_context: execution_context
      )
    end

    it 'includes execution context in the message' do
      expect(step_message.execution_context).to eq(execution_context)
      expect(step_message.execution_context.task).to eq(task_data)
      expect(step_message.execution_context.step).to eq(step_data)
    end

    it 'serializes to hash with execution context' do
      hash = step_message.to_h
      expect(hash).to include(:execution_context)
      expect(hash[:execution_context]).to be_a(Hash)
    end

    it 'deserializes from hash with execution context' do
      hash = step_message.to_h
      deserialized = TaskerCore::Types::StepMessage.from_hash(hash)
      
      expect(deserialized.execution_context).to be_a(TaskerCore::Types::StepExecutionContext)
      expect(deserialized.execution_context.task).to eq(task_data)
      expect(deserialized.execution_context.step).to eq(step_data)
    end

    it 'provides convenient dependency access' do
      deps = step_message.execution_context.dependencies
      expect(deps).to be_a(TaskerCore::Types::DependencyChain)
      expect(deps.empty?).to be true
    end
  end

  describe 'handler interface pattern' do
    let(:order_validation) do
      TaskerCore::Types::StepDependencyResult.new(
        step_name: 'validate_order',
        step_id: 123,
        named_step_id: 1,
        results: { status: 'validated', order_id: 1001, total: 99.99 }
      )
    end

    let(:inventory_check) do
      TaskerCore::Types::StepDependencyResult.new(
        step_name: 'check_inventory',
        step_id: 124,
        named_step_id: 2,
        results: { status: 'available', quantity: 5, reserved: 1 }
      )
    end

    let(:execution_context) do
      TaskerCore::Types::StepExecutionContext.new(
        task: { task_id: 67890, customer_id: 12345 },
        sequence: [order_validation, inventory_check],
        step: { step_id: 125, step_name: 'process_payment' }
      )
    end

    it 'demonstrates the intended handler interface' do
      task = execution_context.task
      sequence = execution_context.dependencies
      step = execution_context.step

      # Handler can access specific dependency results
      order_data = sequence.get('validate_order')
      expect(order_data.results[:order_id]).to eq(1001)
      expect(order_data.results[:total]).to eq(99.99)

      inventory_data = sequence['check_inventory']
      expect(inventory_data.results[:quantity]).to eq(5)
      expect(inventory_data.results[:reserved]).to eq(1)

      # Handler has access to task and step context
      expect(task[:customer_id]).to eq(12345)
      expect(step[:step_name]).to eq('process_payment')
    end
  end
end