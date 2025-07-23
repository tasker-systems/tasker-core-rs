# frozen_string_literal: true

require 'spec_helper'

RSpec.describe 'Magnus TaskHandlerInitializeResult wrapper' do
  let(:handler) { TaskerCore::BaseTaskHandler.new }
  let(:task_request) do
    {
      'namespace' => 'test',
      'name' => 'simple_task',
      'version' => '1.0.0',
      'status' => 'PENDING',
      'initiator' => 'spec_test',
      'source_system' => 'rspec',
      'reason' => 'Magnus wrapper test',
      'complete' => false,
      'tags' => ['test'],
      'bypass_steps' => [],
      'requested_at' => Time.now.strftime('%Y-%m-%dT%H:%M:%S'),
      'context' => { 'test' => 'data' }
    }
  end

  it 'returns a hash with working data access' do
    result = handler.initialize_task(task_request)

    # Test object class - now returns Hash for simplified FFI
    expect(result.class.name).to eq('Hash')

    # Test hash key access - these should contain real data
    expect(result['task_id']).to be_a(Integer)
    expect(result['task_id']).to be > 0
    expect(result['step_count']).to be_a(Integer)
    expect(result['step_count']).to eq(0) # No handlers registered, so 0 steps
    expect(result['step_mapping']).to be_a(Hash)
    expect(result['step_mapping']).to be_empty
    expect(result['handler_config_name']).to be_nil
    
    # Test workflow_steps key - should return empty array for tasks without configuration
    expect(result['workflow_steps']).to be_a(Array)
    expect(result['workflow_steps']).to be_empty

    puts "✅ SUCCESS: Hash-based FFI working correctly!"
    puts "   Task ID: #{result['task_id']}"
    puts "   Step count: #{result['step_count']}"
    puts "   Step mapping: #{result['step_mapping']}"
    puts "   Handler config: #{result['handler_config_name'].inspect}"
    puts "   Workflow steps: #{result['workflow_steps'].inspect}"
  end

  it 'creates real database records' do
    result1 = handler.initialize_task(task_request)
    result2 = handler.initialize_task(task_request)

    # Should create different task IDs
    expect(result1['task_id']).to_not eq(result2['task_id'])
    expect(result1['task_id']).to be < result2['task_id'] # Task IDs should increment

    puts "✅ SUCCESS: Database integration working!"
    puts "   First task ID: #{result1['task_id']}"
    puts "   Second task ID: #{result2['task_id']}"
  end
end