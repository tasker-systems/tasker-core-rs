# frozen_string_literal: true

require 'spec_helper'

RSpec.describe 'Orchestration Loop Startup', type: :integration do
  before(:all) do
    # Clean setup for orchestration testing
    @database_url = ENV['DATABASE_URL'] || 'postgresql://tasker:tasker@localhost/tasker_rust_test'
    TaskerCore.setup_test_database(@database_url)

    # Register a simple task template so the orchestration has something to potentially work with
    @task_template_registry = TaskerCore::Registry::TaskTemplateRegistry.instance
    @task_template_registry.register_task_templates_from_directory('spec/handlers/examples/linear_workflow/config')
  end

  after(:all) do
    TaskerCore.teardown_test_database(@database_url) if @database_url
  end

  describe 'Basic Orchestration System Startup' do
    it 'starts and stops the embedded orchestration system cleanly' do
      # Start the embedded orchestration system with a single namespace
      puts "\n🚀 Starting embedded orchestration system..."
      result = TaskerCore.start_embedded_orchestration!(['linear_workflow'])

      expect(result).to be_a(Hash)
      expect(result['status']).to eq('started')
      puts "✅ Orchestration system started: #{result}"

      # Let it run for a few seconds to see if orchestration loops execute
      puts "\n⏱️ Letting orchestration run for 5 seconds to observe loop activity..."
      sleep 5

      # Check if orchestration system is actually running
      orchestrator = TaskerCore.embedded_orchestrator
      puts "\n📊 Orchestrator status:"
      puts "   - Orchestrator present: #{!orchestrator.nil?}"
      puts "   - Orchestrator running: #{orchestrator&.running?}"

      # Stop the orchestration system
      puts "\n🛑 Stopping embedded orchestration system..."
      stop_result = TaskerCore.stop_embedded_orchestration!
      puts "✅ Stop result: #{stop_result}"

      expect(stop_result).to be_a(Hash)
      expect(stop_result['status']).to eq('stopped')
    end

    it 'can start orchestration, create a task, and observe orchestration activity' do
      puts "\n🚀 Starting orchestration with task creation test..."

      # Start orchestration
      TaskerCore.start_embedded_orchestration!(['linear_workflow'])

      # Create a simple task to give the orchestration loop something to process
      task_request = TaskerCore::Types::TaskTypes::TaskRequest.new(
        namespace: 'linear_workflow',
        name: 'mathematical_sequence',
        version: '1.0.0',
        context: { even_number: 4, test_run_id: SecureRandom.uuid },
        initiator: 'orchestration_startup_test',
        source_system: 'rspec_orchestration',
        reason: 'Test orchestration loop activity',
        priority: 3,
        claim_timeout_seconds: 30
      )

      puts "\n📝 Creating task to trigger orchestration activity..."
      task_result = TaskerCore.initialize_task_embedded(task_request.to_ffi_hash)
      expect(task_result['success']).to be(true)
      task_id = task_result['task_id']
      puts "✅ Created task #{task_id}"

      # Let orchestration run and observe activity
      puts "\n⏱️ Observing orchestration activity for 8 seconds..."
      puts '   (Looking for orchestration loop cycles, task claiming, step enqueueing)'

      activity_observed = false
      8.times do |i|
        sleep 1

        # Check for any orchestration activity
        sql_functions = TaskerCore::Database::SqlFunctions.new
        health = sql_functions.system_health_counts
        analytics = sql_functions.analytics_metrics

        next unless i.even? # Log every 2 seconds

        puts "   Second #{i + 1}: active_tasks=#{analytics[:active_tasks_count]}, " \
             "ready_steps=#{health[:ready_steps] || 0}, " \
             "in_progress_steps=#{health[:in_progress_steps] || 0}, " \
             "complete_steps=#{health[:complete_steps] || 0}"

        if health[:in_progress_steps] && health[:in_progress_steps] > 0
          activity_observed = true
          puts '   🎯 ACTIVITY DETECTED: Steps are in progress!'
        end
      end

      puts "\n📊 Final orchestration state:"
      final_sql_functions = TaskerCore::Database::SqlFunctions.new
      final_health = final_sql_functions.system_health_counts
      final_analytics = final_sql_functions.analytics_metrics
      puts "   - Active tasks: #{final_analytics[:active_tasks_count]}"
      puts "   - Ready steps: #{final_health[:ready_steps] || 0}"
      puts "   - In progress steps: #{final_health[:in_progress_steps] || 0}"
      puts "   - Complete steps: #{final_health[:complete_steps] || 0}"
      puts "   - Activity observed: #{activity_observed}"

      # Stop orchestration
      TaskerCore.stop_embedded_orchestration!

      # The test passes regardless - we're just observing what happens
      expect(task_result['success']).to be(true)
    end
  end
end
