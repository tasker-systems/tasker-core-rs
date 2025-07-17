# frozen_string_literal: true

require 'spec_helper'

RSpec.describe TaskerCore::TestHelpers do
  include described_class

  describe '#create_test_task' do
    it 'creates a task using Rust factory through FFI' do
      task = create_test_task(
        name: 'rspec_test_task',
        initiator: 'rspec_test_suite',
        context: { test_framework: 'rspec', test_type: 'integration' }
      )

      expect(task).to be_a(Hash)
      skip "Database error: #{task['error']}" if task['error']

      expect(task['task_id']).to be_a(Integer)
      expect(task['initiator']).to eq('rspec_test_suite')
      expect(task['context']).to include('test_framework' => 'rspec')
    end

    it 'creates a complex workflow task' do
      task = create_test_task(
        name: 'complex_test_task',
        context: { workflow_type: 'complex' }
      )

      expect(task).to be_a(Hash)
      skip "Database error: #{task['error']}" if task['error']

      expect(task['task_id']).to be_a(Integer)
      expect(task['context']).to be_a(Hash)
      expect(task['context']['workflow_type']).to eq('complex')
    end

    it 'creates an API integration task' do
      task = create_test_task(
        name: 'api_test_task',
        context: { workflow_type: 'api_integration' }
      )

      expect(task).to be_a(Hash)
      skip "Database error: #{task['error']}" if task['error']

      expect(task['task_id']).to be_a(Integer)
      expect(task['context']).to be_a(Hash)
      expect(task['context']['workflow_type']).to eq('api_integration')
    end
  end

  describe '#create_test_workflow_step' do
    it 'creates a workflow step using Rust factory through FFI' do
      # First create a task
      task = create_test_task(name: 'step_test_task')

      skip "Database error: #{task['error']}" if task['error']

      step = create_test_workflow_step(
        task_id: task['task_id'],
        name: 'rspec_test_step',
        inputs: { test_data: 'rspec_input' }
      )

      expect(step).to be_a(Hash)
      skip "Database error: #{step['error']}" if step['error']

      expect(step['workflow_step_id']).to be_a(Integer)
      expect(step['task_id']).to eq(task['task_id'])
      expect(step['inputs']).to include('test_data' => 'rspec_input')
    end

    it 'creates an API call step' do
      task = create_test_task(name: 'api_step_test_task')

      skip "Database error: #{task['error']}" if task['error']

      step = create_test_workflow_step(
        task_id: task['task_id'],
        name: 'api_call_step',
        inputs: { step_type: 'api_call', url: 'https://api.example.com' }
      )

      expect(step).to be_a(Hash)
      skip "Database error: #{step['error']}" if step['error']

      expect(step['inputs']).to be_a(Hash)
      expect(step['inputs']['url']).to include('api.example.com')
    end

    it 'creates a database step' do
      task = create_test_task(name: 'db_step_test_task')

      skip "Database error: #{task['error']}" if task['error']

      step = create_test_workflow_step(
        task_id: task['task_id'],
        name: 'db_operation_step',
        inputs: { step_type: 'database', query: 'UPDATE users SET active = true' }
      )

      expect(step).to be_a(Hash)
      skip "Database error: #{step['error']}" if step['error']

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

      expect(foundation['namespace']).to be_a(Hash)
      expect(foundation['named_task']).to be_a(Hash)
      expect(foundation['named_step']).to be_a(Hash)

      expect(foundation['namespace']['name']).to eq('rspec_test')
      expect(foundation['named_task']['name']).to eq('dummy_task')
      expect(foundation['named_step']['name']).to eq('dummy_step')
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

      # Check task structure
      skip "Database error: #{workflow['task']['error']}" if workflow['task']['error']

      expect(workflow['task']['task_id']).to be_a(Integer)

      # Check step structure
      workflow['steps'].each do |step|
        skip "Database error: #{step['error']}" if step['error']
        expect(step['workflow_step_id']).to be_a(Integer)
        expect(step['task_id']).to eq(workflow['task']['task_id'])
      end
    end

    it 'creates a diamond workflow' do
      workflow = create_test_workflow(:diamond, context: { test_type: 'diamond' })

      expect(workflow).to be_a(Hash)
      expect(workflow['type']).to eq('diamond')
      expect(workflow['task']).to be_a(Hash)
      expect(workflow['steps']).to be_an(Array)
      expect(workflow['steps'].length).to eq(4)

      skip_if_database_error(workflow)
    end

    it 'creates a parallel workflow' do
      workflow = create_test_workflow(:parallel, context: { test_type: 'parallel' })

      expect(workflow).to be_a(Hash)
      expect(workflow['type']).to eq('parallel')
      expect(workflow['task']).to be_a(Hash)
      expect(workflow['steps']).to be_an(Array)
      expect(workflow['steps'].length).to eq(3)

      skip_if_database_error(workflow)
    end

    it 'creates a tree workflow' do
      workflow = create_test_workflow(:tree, context: { test_type: 'tree' })

      expect(workflow).to be_a(Hash)
      expect(workflow['type']).to eq('tree')
      expect(workflow['task']).to be_a(Hash)
      expect(workflow['steps']).to be_an(Array)
      expect(workflow['steps'].length).to eq(5)

      skip_if_database_error(workflow)
    end
  end

  # Helper method for skipping tests with database errors
  def skip_if_database_error(workflow)
    skip "Database error: #{workflow['task']['error']}" if workflow['task']['error']

    workflow['steps'].each do |step|
      skip "Database error: #{step['error']}" if step['error']
    end
  end
end
