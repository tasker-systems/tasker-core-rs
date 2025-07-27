# frozen_string_literal: true

require 'spec_helper'
require 'tasker_core/execution'
require 'tasker_core/embedded_server'
require 'timeout'

RSpec.describe 'Ruby-Rust Command Integration', :integration do
  # These integration tests use our embedded TCP executor
  # No external dependencies required!

  before(:all) do
    # Start embedded server and wait for it to be ready
    server_config = {
      bind_address: '127.0.0.1:0', # Use port 0 for automatic assignment
      command_queue_size: 100,
      connection_timeout_ms: 5000,
      graceful_shutdown_timeout_ms: 2000,
      max_connections: 10,
      background: true
    }

    @embedded_server = TaskerCore::EmbeddedServer.new(server_config)
    @embedded_server.start(block_until_ready: true, ready_timeout: 10)

    puts "✅ Embedded TCP executor started on #{@embedded_server.bind_address}"
  end

  after(:all) do
    # Clean shutdown of embedded server
    if @embedded_server&.running?
      @embedded_server.stop(timeout: 5)
      puts "✅ Embedded TCP executor stopped"
    end
  end

  # Helper method to get the actual server port for command client
  def executor_port
    @embedded_server.bind_address.split(':').last.to_i
  end

  let(:command_client) do
    TaskerCore::Execution::CommandClient.new(
      host: '127.0.0.1',
      port: executor_port,
      timeout: 3  # Short timeout for tests - if it can't connect in 3 seconds, there's a problem
    )
  end

  describe 'TCP Connection' do
    it 'connects to Rust TCP executor' do
      expect { command_client.connect }.not_to raise_error
      expect(command_client.connected?).to be true

      command_client.disconnect
    end

    it 'handles connection failures gracefully' do
      bad_client = TaskerCore::Execution::CommandClient.new(
        host: 'nonexistent-host',
        port: 99999
      )

      expect { bad_client.connect }.to raise_error(
        TaskerCore::Execution::ConnectionError
      )
    end
  end

  describe 'Health Check' do
    before { command_client.connect }
    after { command_client.disconnect }

    it 'performs basic health check' do
      response = command_client.health_check

      expect(response).to be_a(TaskerCore::Types::HealthCheckResponse)
      expect(response.command_type).to eq('HealthCheckResult')
      expect(response.response_type).to eq('HealthCheckResult')
      expect(response).to be_success
      expect(response).to be_healthy
      expect(response.uptime_seconds).to be >= 0
      expect(response.total_workers).to be >= 0
      expect(response.active_commands).to be >= 0
    end

    it 'performs detailed health check' do
      response = command_client.health_check(diagnostic_level: 'Detailed')

      expect(response).to be_a(TaskerCore::Types::HealthCheckResponse)
      expect(response.command_type).to eq('HealthCheckResult')
      expect(response.response_type).to eq('HealthCheckResult')
      expect(response).to be_healthy
      expect(response.diagnostics).to respond_to(:current_load)
      expect(response.diagnostics).to respond_to(:status)
    end
  end

  describe 'Worker Registration' do
    let(:worker_id) { "integration_test_worker_#{Process.pid}_#{Time.now.to_i}" }

    before { command_client.connect }
    after do
      # Cleanup - unregister worker
      begin
        command_client.unregister_worker(
          worker_id: worker_id,
          reason: 'Test cleanup'
        )
      rescue StandardError => e
        puts "Warning: Failed to cleanup worker #{worker_id}: #{e.message}"
      end
      command_client.disconnect
    end

    it 'registers a Ruby worker successfully' do
      response = command_client.register_worker(
        worker_id: worker_id,
        max_concurrent_steps: 8,
        supported_namespaces: ['integration_test', 'orders'],
        step_timeout_ms: 25000,
        supports_retries: true,
        language_runtime: 'ruby',
        version: RUBY_VERSION,
        custom_capabilities: { 'test_mode' => true }
      )

      expect(response).to be_a(TaskerCore::Types::WorkerRegistrationResponse)
      expect(response.command_type).to eq('WorkerRegistered')
      expect(response).to be_worker_registered

      expect(response.worker_id).to eq(worker_id)
      expect(response.assigned_pool).to eq('default')
      expect(response.queue_position).to be_a(Integer)
    end

    it 'sends heartbeat after registration' do
      # Register worker first
      command_client.register_worker(
        worker_id: worker_id,
        max_concurrent_steps: 5,
        supported_namespaces: ['integration_test'],
        step_timeout_ms: 20000,
        supports_retries: false,
        language_runtime: 'ruby',
        version: RUBY_VERSION
      )

      # Send heartbeat
      response = command_client.send_heartbeat(
        worker_id: worker_id,
        current_load: 3,
        system_stats: {
          cpu_usage_percent: 50.0,
          memory_usage_mb: 1024,
          active_connections: 2,
          uptime_seconds: 300
        }
      )

      expect(response).to be_a(TaskerCore::Types::HeartbeatResponse)
      expect(response.command_type).to eq('HeartbeatAcknowledged')
      expect(response).to be_heartbeat_acknowledged

      expect(response.worker_id).to eq(worker_id)
      expect(response.status).to eq('healthy')
    end

    it 'unregisters worker successfully' do
      # Register worker first
      command_client.register_worker(
        worker_id: worker_id,
        max_concurrent_steps: 5,
        supported_namespaces: ['integration_test'],
        step_timeout_ms: 20000,
        supports_retries: false,
        language_runtime: 'ruby',
        version: RUBY_VERSION
      )

      # Unregister worker
      response = command_client.unregister_worker(
        worker_id: worker_id,
        reason: 'Integration test completion'
      )

      expect(response).to be_a(TaskerCore::Types::WorkerUnregistrationResponse)
      expect(response.command_type).to eq('WorkerUnregistered')
      expect(response).to be_worker_unregistered

      expect(response.worker_id).to eq(worker_id)
      expect(response.reason).to eq('Integration test completion')
    end
  end

  describe 'WorkerManager Integration' do
    let(:worker_id) { "manager_test_worker_#{Process.pid}_#{Time.now.to_i}" }
    let(:worker_manager) do
      TaskerCore::Execution::WorkerManager.new(
        worker_id: worker_id,
        supported_namespaces: ['manager_integration_test'],
        max_concurrent_steps: 6,
        heartbeat_interval: 2, # Short interval for testing
        executor_host: '127.0.0.1',
        executor_port: executor_port,
        timeout: 3 # Short timeout for tests
      )
    end

    after do
      worker_manager.stop if worker_manager.running?
    end

    it 'starts and stops worker with automatic heartbeats' do
      # Start worker
      expect(worker_manager.start).to be true
      expect(worker_manager.running?).to be true
      expect(worker_manager.healthy?).to be true

      # Wait for at least one heartbeat cycle
      sleep(3)
      expect(worker_manager.running?).to be true

      # Test load reporting
      worker_manager.report_step_start(2)
      expect(worker_manager.current_load).to eq(2)

      worker_manager.report_step_completion(1)
      expect(worker_manager.current_load).to eq(1)

      # Check stats
      stats = worker_manager.stats
      expect(stats[:worker_id]).to eq(worker_id)
      expect(stats[:current_load]).to eq(1)
      expect(stats[:available_capacity]).to eq(5)

      # Stop worker
      expect(worker_manager.stop).to be true
      expect(worker_manager.running?).to be false
    end

    it 'handles multiple workers sequentially' do
      worker_ids = []

      # Test multiple workers sequentially (more realistic usage pattern)
      3.times do |i|
        worker_id = "sequential_worker_#{i}_#{Process.pid}"
        worker_ids << worker_id

        worker = TaskerCore::Execution::WorkerManager.new(
          worker_id: worker_id,
          supported_namespaces: ["namespace_#{i}"],
          executor_host: '127.0.0.1',
          executor_port: executor_port,
          timeout: 3
        )

        # Start worker
        expect(worker.start).to be true
        expect(worker.running?).to be true

        # Verify it's working with a heartbeat cycle
        sleep(0.5) # Allow heartbeat to process
        expect(worker.running?).to be true

        # Stop worker
        expect(worker.stop).to be true
        expect(worker.running?).to be false
      end

      # Test shows we can reliably start and stop multiple workers
      expect(worker_ids.length).to eq(3)
    end
  end

  describe 'Error Handling' do
    before { command_client.connect }
    after { command_client.disconnect }

    it 'handles duplicate worker registration' do
      worker_id = "duplicate_test_#{Process.pid}"

      # Register worker first time
      response1 = command_client.register_worker(
        worker_id: worker_id,
        max_concurrent_steps: 5,
        supported_namespaces: ['test'],
        step_timeout_ms: 20000,
        supports_retries: false,
        language_runtime: 'ruby',
        version: RUBY_VERSION
      )
      expect(response1[:command_type]).to eq('WorkerRegistered')

      # Try to register same worker again - should handle gracefully
      response2 = command_client.register_worker(
        worker_id: worker_id,
        max_concurrent_steps: 5,
        supported_namespaces: ['test'],
        step_timeout_ms: 20000,
        supports_retries: false,
        language_runtime: 'ruby',
        version: RUBY_VERSION
      )

      # Rust executor should handle this gracefully (update registration)
      expect(response2).to be_a(TaskerCore::Types::WorkerRegistrationResponse)

      # Cleanup
      command_client.unregister_worker(worker_id: worker_id, reason: 'Test cleanup')
    end

    it 'handles heartbeat for non-existent worker' do
      # This should not crash, but may return an error response
      response = command_client.send_heartbeat(
        worker_id: 'nonexistent_worker',
        current_load: 0
      )

      expect(response).to respond_to(:command_type) # Can be either error or success response
      # Response may indicate error or handle gracefully
    end
  end

  describe 'Performance' do
    before { command_client.connect }
    after { command_client.disconnect }

    it 'handles rapid command sequences' do
      worker_id = "perf_test_#{Process.pid}"

      # Register worker
      register_start = Process.clock_gettime(Process::CLOCK_MONOTONIC)
      command_client.register_worker(
        worker_id: worker_id,
        max_concurrent_steps: 5,
        supported_namespaces: ['perf_test'],
        step_timeout_ms: 20000,
        supports_retries: false,
        language_runtime: 'ruby',
        version: RUBY_VERSION
      )
      register_time = Process.clock_gettime(Process::CLOCK_MONOTONIC) - register_start

      # Send multiple heartbeats rapidly
      heartbeat_times = []
      10.times do |i|
        start = Process.clock_gettime(Process::CLOCK_MONOTONIC)
        command_client.send_heartbeat(
          worker_id: worker_id,
          current_load: i % 3
        )
        heartbeat_times << (Process.clock_gettime(Process::CLOCK_MONOTONIC) - start)
      end

      # Unregister worker
      unregister_start = Process.clock_gettime(Process::CLOCK_MONOTONIC)
      command_client.unregister_worker(worker_id: worker_id, reason: 'Perf test')
      unregister_time = Process.clock_gettime(Process::CLOCK_MONOTONIC) - unregister_start

      # Verify reasonable performance
      expect(register_time).to be < 1.0 # Registration under 1 second
      expect(heartbeat_times.max).to be < 0.5 # Each heartbeat under 500ms
      expect(unregister_time).to be < 1.0 # Unregistration under 1 second

      puts "Performance results:"
      puts "  Registration: #{(register_time * 1000).round(1)}ms"
      puts "  Heartbeat avg: #{(heartbeat_times.sum / heartbeat_times.length * 1000).round(1)}ms"
      puts "  Heartbeat max: #{(heartbeat_times.max * 1000).round(1)}ms"
      puts "  Unregistration: #{(unregister_time * 1000).round(1)}ms"
    end
  end
end
