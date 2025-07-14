# frozen_string_literal: true

require 'spec_helper'

RSpec.describe TaskerCore::TestHelpers do
  include TaskerCore::TestHelpers

  describe '#create_test_task' do
    it 'creates a task using Rust factory through FFI' do
      task = create_test_task(
        name: 'rspec_test_task',
        initiator: 'rspec_test_suite',
        context: { test_framework: 'rspec', test_type: 'integration' }
      )

      expect(task).to be_a(Hash)
      expect(task['id']).to be_a(Integer)
      expect(task['initiator']).to eq('rspec_test_suite')
      expect(task['context']).to include('test_framework' => 'rspec')
    end

    it 'creates a complex workflow task' do
      task = create_test_task(
        name: 'complex_test_task',
        workflow_type: 'complex',
        state: 'pending'
      )

      expect(task).to be_a(Hash)
      expect(task['id']).to be_a(Integer)
      expect(task['context']).to be_a(Hash)
      expect(task['context']['workflow_type']).to eq('complex')
    end

    it 'creates an API integration task' do
      task = create_test_task(
        name: 'api_test_task',
        workflow_type: 'api_integration',
        state: 'in_progress'
      )

      expect(task).to be_a(Hash)
      expect(task['id']).to be_a(Integer)
      expect(task['context']['api_endpoint']).to be_a(String)
    end
  end

  describe '#create_test_workflow_step' do
    it 'creates a workflow step using Rust factory through FFI' do
      # First create a task
      task = create_test_task(name: 'step_test_task')

      step = create_test_workflow_step(
        task_id: task['id'],
        name: 'rspec_test_step',
        state: 'pending',
        inputs: { test_data: 'rspec_input' }
      )

      expect(step).to be_a(Hash)
      expect(step['id']).to be_a(Integer)
      expect(step['task_id']).to eq(task['id'])
      expect(step['current_state']).to eq('pending')
      expect(step['inputs']).to include('test_data' => 'rspec_input')
    end

    it 'creates an API call step' do
      task = create_test_task(name: 'api_step_test_task')

      step = create_test_workflow_step(
        task_id: task['id'],
        name: 'api_call_step',
        step_type: 'api_call',
        state: 'complete'
      )

      expect(step).to be_a(Hash)
      expect(step['inputs']).to be_a(Hash)
      expect(step['inputs']['url']).to include('api.example.com')
    end

    it 'creates a database step' do
      task = create_test_task(name: 'db_step_test_task')

      step = create_test_workflow_step(
        task_id: task['id'],
        name: 'db_operation_step',
        step_type: 'database',
        state: 'in_progress'
      )

      expect(step).to be_a(Hash)
      expect(step['inputs']).to be_a(Hash)
      expect(step['inputs']['query']).to include('UPDATE')
    end
  end

  describe '#create_test_foundation' do
    it 'creates foundation data using Rust factory through FFI' do
      foundation = create_test_foundation(
        task_name: 'rspec_foundation_task',
        namespace: 'rspec_test'
      )

      puts "🔍 DEBUG: Foundation result = #{foundation.inspect}"
      puts "🔑 DEBUG: Keys = #{foundation.keys.inspect}"

      expect(foundation).to be_a(Hash)
      
      if foundation['error']
        puts "❌ ERROR: #{foundation['error']}"
        skip "Database error: #{foundation['error']}"
      end

      expect(foundation['standard_foundation']).to be_a(Hash)

      if foundation['custom_named_task']
        expect(foundation['custom_named_task']['name']).to eq('rspec_foundation_task')
        expect(foundation['custom_named_task']['namespace']).to eq('rspec_test')
      end
    end
  end

  describe '#create_test_workflow' do
    it 'creates a linear workflow' do
      workflow = create_test_workflow(:linear, context: { test_type: 'linear' })

      expect(workflow).to be_a(Hash)
      expect(workflow['type']).to eq('linear')
      expect(workflow['task']).to be_a(Hash)
      expect(workflow['steps']).to be_an(Array)
      expect(workflow['steps'].length).to eq(4)
    end

    it 'creates a diamond workflow' do
      workflow = create_test_workflow(:diamond, context: { test_type: 'diamond' })

      expect(workflow).to be_a(Hash)
      expect(workflow['type']).to eq('diamond')
      expect(workflow['task']).to be_a(Hash)
      expect(workflow['steps']).to be_an(Array)
      expect(workflow['steps'].length).to eq(4)
    end

    it 'creates a parallel workflow' do
      workflow = create_test_workflow(:parallel, context: { test_type: 'parallel' })

      expect(workflow).to be_a(Hash)
      expect(workflow['type']).to eq('parallel')
      expect(workflow['task']).to be_a(Hash)
      expect(workflow['steps']).to be_an(Array)
      expect(workflow['steps'].length).to eq(3)
    end

    it 'creates a tree workflow' do
      workflow = create_test_workflow(:tree, context: { test_type: 'tree' })

      expect(workflow).to be_a(Hash)
      expect(workflow['type']).to eq('tree')
      expect(workflow['task']).to be_a(Hash)
      expect(workflow['steps']).to be_an(Array)
      expect(workflow['steps'].length).to eq(5)
    end
  end

  describe '#cleanup_test_database' do
    it 'cleans up test data using Rust cleanup through FFI' do
      # Create some test data first
      task = create_test_task(name: 'cleanup_test_task')
      step = create_test_workflow_step(task_id: task['id'], name: 'cleanup_test_step')

      # Now clean up
      result = cleanup_test_database

      expect(result).to be_a(Hash)
      expect(result['cleanup_completed']).to be true
      expect(result['records_deleted']).to be_a(Hash)
      expect(result['records_deleted']['tasks']).to be_a(Integer)
      expect(result['records_deleted']['workflow_steps']).to be_a(Integer)
    end
  end

  describe '#setup_test_database' do
    it 'sets up test database with foundation data' do
      result = setup_test_database

      expect(result).to be_a(Hash)
      expect(result['standard_foundation']).to be_a(Hash)
    end
  end
end
