# frozen_string_literal: true

require 'spec_helper'

RSpec.describe "Foundation Debug", type: :debug do
  it "creates foundation and shows structure" do
    puts "🔍 Testing foundation creation..."
    
    foundation = TaskerCore::Factory.foundation(
      task_name: "debug_task",
      namespace: "debug_namespace"
    )
    
    puts "✅ Foundation created successfully"
    puts "📋 Keys: #{foundation.keys}"
    puts "🔍 Full structure: #{foundation.inspect}"
    
    # Test our extraction logic
    task_id = foundation.dig('task', 'task_id') || foundation.dig('named_task', 'task_id')
    puts "🎯 Extracted task_id: #{task_id}"
    
    expect(task_id).not_to be_nil
  end
end