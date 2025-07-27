#!/usr/bin/env ruby
# frozen_string_literal: true

# Example: Ruby Worker Registration with Rust Command Executor
#
# This example demonstrates how to use the new Ruby Command Client
# to register workers with the Rust TCP Executor, replacing the
# previous ZeroMQ-based communication.
#
# Prerequisites:
# 1. Start the Rust TCP Executor:
#    cd /path/to/tasker-core-rs
#    cargo run --bin tcp_executor
#
# 2. Run this example:
#    ruby examples/command_integration_example.rb

require_relative '../lib/tasker_core/execution'

class CommandIntegrationExample
  def initialize
    @logger = Logger.new(STDOUT)
    @logger.level = Logger::INFO
    @logger.formatter = proc do |severity, datetime, progname, msg|
      "[#{datetime.strftime('%H:%M:%S')}] #{severity}: #{msg}\n"
    end
  end

  def run
    @logger.info("=== Ruby-Rust Command Integration Example ===")
    
    # Configure execution settings
    TaskerCore::Execution.configure do |config|
      config.default_executor_host = 'localhost'
      config.default_executor_port = 8080
      config.default_heartbeat_interval = 10
      config.default_logger = @logger
    end

    # Example 1: Health Check
    @logger.info("\n1. Checking Rust executor health...")
    health_result = TaskerCore::Execution.check_executor_health
    
    if health_result[:healthy]
      @logger.info("✅ Executor is healthy!")
      @logger.info("   Response: #{health_result[:response]}")
    else
      @logger.error("❌ Executor health check failed: #{health_result[:message]}")
      @logger.info("   Make sure the Rust TCP executor is running:")
      @logger.info("   cd tasker-core-rs && cargo run --bin tcp_executor")
      return
    end

    # Example 2: Manual Command Client Usage
    @logger.info("\n2. Manual command client example...")
    demonstrate_command_client

    # Example 3: High-Level Worker Manager
    @logger.info("\n3. High-level worker manager example...")
    demonstrate_worker_manager

    # Example 4: Multiple Workers
    @logger.info("\n4. Multiple workers example...")
    demonstrate_multiple_workers

    @logger.info("\n=== Example completed successfully! ===")
  end

  private

  def demonstrate_command_client
    client = TaskerCore::Execution.create_command_client
    
    begin
      # Connect
      @logger.info("   Connecting to Rust executor...")
      client.connect
      
      # Register worker
      @logger.info("   Registering worker...")
      worker_id = "example_worker_#{Process.pid}"
      
      response = client.register_worker(
        worker_id: worker_id,
        max_concurrent_steps: 8,
        supported_namespaces: ['orders', 'payments', 'inventory'],
        step_timeout_ms: 30000,
        supports_retries: true,
        language_runtime: 'ruby',
        version: RUBY_VERSION,
        custom_capabilities: {
          'example_mode' => true,
          'features' => ['async_processing', 'batch_operations']
        }
      )
      
      @logger.info("   ✅ Worker registered: #{response[:payload][:data][:worker_id]}")
      @logger.info("   📊 Queue position: #{response[:payload][:data][:queue_position]}")
      
      # Send heartbeat
      @logger.info("   Sending heartbeat...")
      heartbeat_response = client.send_heartbeat(
        worker_id: worker_id,
        current_load: 3,
        system_stats: {
          cpu_usage_percent: 45.0,
          memory_usage_mb: 1024,
          active_connections: 2,
          uptime_seconds: 300
        }
      )
      
      @logger.info("   ✅ Heartbeat acknowledged: #{heartbeat_response[:payload][:data] || heartbeat_response[:payload]}")
      
      # Unregister worker
      @logger.info("   Unregistering worker...")
      unregister_response = client.unregister_worker(
        worker_id: worker_id,
        reason: 'Example completion'
      )
      
      @logger.info("   ✅ Worker unregistered: #{unregister_response[:payload][:data] || unregister_response[:payload]}")
      
    ensure
      client.disconnect
    end
  end

  def demonstrate_worker_manager
    worker_id = "manager_worker_#{Process.pid}"
    
    # Create worker manager
    manager = TaskerCore::Execution.create_worker_manager(
      worker_id: worker_id,
      supported_namespaces: ['manager_example', 'background_tasks'],
      max_concurrent_steps: 12,
      heartbeat_interval: 5 # Short interval for demo
    )
    
    begin
      # Start worker (auto-registration and heartbeats)
      @logger.info("   Starting worker manager...")
      manager.start
      
      @logger.info("   ✅ Worker started and registered")
      @logger.info("   📊 Stats: #{manager.stats}")
      
      # Simulate some work
      @logger.info("   Simulating step execution...")
      manager.report_step_start(3)
      @logger.info("   📈 Load increased: #{manager.current_load}/#{manager.max_concurrent_steps}")
      
      sleep(2) # Let heartbeat run
      
      manager.report_step_completion(2)
      @logger.info("   📉 Steps completed: #{manager.current_load}/#{manager.max_concurrent_steps}")
      
      # Send manual heartbeat
      heartbeat_response = manager.send_heartbeat
      @logger.info("   💓 Manual heartbeat: #{heartbeat_response[:payload][:data] || heartbeat_response[:payload]}")
      
    ensure
      # Stop worker (auto-unregistration)
      @logger.info("   Stopping worker manager...")
      manager.stop
      @logger.info("   ✅ Worker stopped and unregistered")
    end
  end

  def demonstrate_multiple_workers
    workers = []
    
    begin
      # Start multiple workers for different namespaces
      worker_configs = [
        { namespace: 'orders', steps: 5 },
        { namespace: 'payments', steps: 8 },
        { namespace: 'inventory', steps: 12 }
      ]
      
      @logger.info("   Starting #{worker_configs.length} workers...")
      
      worker_configs.each_with_index do |config, index|
        worker = TaskerCore::Execution.start_worker(
          worker_id: "multi_worker_#{config[:namespace]}_#{Process.pid}",
          supported_namespaces: [config[:namespace]],
          max_concurrent_steps: config[:steps],
          heartbeat_interval: 8
        )
        
        workers << worker
        @logger.info("   ✅ Started #{config[:namespace]} worker (#{config[:steps]} steps capacity)")
      end
      
      # Show all worker stats
      @logger.info("   📊 Worker stats:")
      workers.each do |worker|
        stats = worker.stats
        @logger.info("     #{stats[:worker_id]}: #{stats[:current_load]}/#{stats[:max_concurrent_steps]} load")
      end
      
      # Simulate load on each worker
      @logger.info("   Simulating distributed load...")
      workers.each_with_index do |worker, index|
        load = (index + 1) * 2
        worker.report_step_start(load)
        @logger.info("     #{worker.worker_id}: added #{load} steps")
      end
      
      sleep(3) # Let heartbeats run with load
      
      @logger.info("   📊 Updated stats:")
      workers.each do |worker|
        stats = worker.stats
        @logger.info("     #{stats[:worker_id]}: #{stats[:load_percentage]}% loaded")
      end
      
    ensure
      # Stop all workers
      @logger.info("   Stopping all workers...")
      workers.each do |worker|
        worker.stop
        @logger.info("   ✅ Stopped #{worker.worker_id}")
      end
    end
  end
end

# Run the example
if __FILE__ == $0
  begin
    example = CommandIntegrationExample.new
    example.run
  rescue Interrupt
    puts "\n\nExample interrupted by user"
  rescue StandardError => e
    puts "\nExample failed: #{e.message}"
    puts "Make sure the Rust TCP executor is running:"
    puts "  cd tasker-core-rs && cargo run --bin tcp_executor"
    exit 1
  end
end