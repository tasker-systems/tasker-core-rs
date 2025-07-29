#!/usr/bin/env ruby
# frozen_string_literal: true

# Migration Guide: Ruby to Rust-backed Command Components
#
# This example demonstrates how to migrate from Ruby socket-based commands
# to Rust-backed components for optimal performance and reliability.

require_relative '../bindings/ruby/lib/tasker_core'

puts "🎯 TaskerCore Rust-backed Command Components Migration Guide"
puts "=" * 60

# ========================================================================
# EXAMPLE 1: Command Client Migration
# ========================================================================

puts "\n📡 COMMAND CLIENT EXAMPLE"
puts "-" * 30

begin
  # Create Rust-backed command client (replaces Ruby socket client)
  client = TaskerCore::Execution::CommandClient.new(
    host: 'localhost',
    port: 8080,
    timeout: 30
  )

  puts "✅ Created Rust-backed CommandClient"
  
  # Connect to server
  success = client.connect
  if success
    puts "✅ Connected to Rust TCP executor"
    
    # Perform health check - returns typed response
    health_response = client.health_check(diagnostic_level: 'Detailed')
    puts "✅ Health check successful"
    puts "   - Status: #{health_response.status}"
    puts "   - Healthy?: #{health_response.healthy?}"
    puts "   - Workers: #{health_response.total_workers}"
    puts "   - Uptime: #{health_response.uptime_seconds}s"
    
    client.disconnect
    puts "✅ Disconnected from server"
  else
    puts "❌ Failed to connect to server"
  end
rescue StandardError => e
  puts "❌ Command client example failed: #{e.message}"
end

# ========================================================================
# EXAMPLE 2: Worker Manager Migration
# ========================================================================

puts "\n👷 WORKER MANAGER EXAMPLE"
puts "-" * 30

begin
  # Create Rust-backed worker manager (replaces Ruby socket-based manager)
  worker_manager = TaskerCore::Execution::WorkerManager.new(
    worker_id: 'migration_example_worker',
    supported_namespaces: ['payments', 'orders'],
    max_concurrent_steps: 5,
    heartbeat_interval: 30,
    custom_capabilities: {
      'example_worker' => true,
      'migration_demo' => true
    }
  )

  puts "✅ Created Rust-backed WorkerManager"
  puts "   - Worker ID: #{worker_manager.worker_id}"
  puts "   - Namespaces: #{worker_manager.supported_namespaces}"
  puts "   - Max concurrent: #{worker_manager.max_concurrent_steps}"
  
  # Note: Commented out actual registration since it requires running server
  # worker_manager.start
  # puts "✅ Worker registered and started"
  
  # Show stats
  stats = worker_manager.stats
  puts "📊 Worker Statistics:"
  puts "   - Running: #{stats[:running]}"
  puts "   - Current load: #{stats[:current_load]}"
  puts "   - Manager type: #{stats[:manager_type]}"
  
rescue StandardError => e
  puts "❌ Worker manager example failed: #{e.message}"
end

# ========================================================================
# EXAMPLE 3: OrchestrationManager Integration
# ========================================================================

puts "\n🎛️  ORCHESTRATION MANAGER INTEGRATION"
puts "-" * 40

begin
  # Get orchestration manager instance
  manager = TaskerCore::Internal::OrchestrationManager.instance
  
  puts "✅ Got OrchestrationManager instance"
  
  # Check command architecture status
  arch_status = manager.command_architecture_status
  puts "📋 Command Architecture Status:"
  puts "   - Architecture: #{arch_status[:architecture]}"
  puts "   - Rust FFI available: #{arch_status[:rust_ffi_available]}"
  puts "   - Components: #{arch_status[:components].keys.join(', ')}"
  
  # Create command client through orchestration manager
  cmd_client = manager.create_command_client
  puts "✅ Created CommandClient via OrchestrationManager"
  
  # Create worker manager through orchestration manager
  # worker = manager.create_worker_manager(
  #   worker_id: 'orchestration_managed_worker',
  #   custom_capabilities: { 'orchestration_managed' => true }
  # )
  # puts "✅ Created WorkerManager via OrchestrationManager"

rescue StandardError => e
  puts "❌ OrchestrationManager example failed: #{e.message}"
end

# ========================================================================
# EXAMPLE 4: Typed Response Usage
# ========================================================================

puts "\n🏷️  TYPED RESPONSE EXAMPLES"
puts "-" * 30

# Demonstrate typed response usage
puts "📝 Example typed responses:"

# Worker Registration Response
puts "\n1. WorkerRegistrationResponse:"
puts "   response.worker_registered?  # => true/false"
puts "   response.worker_id          # => 'worker-123'"
puts "   response.assigned_pool      # => 'default_pool'"
puts "   response.queue_position     # => 1"

# Heartbeat Response
puts "\n2. HeartbeatResponse:"
puts "   response.heartbeat_acknowledged?  # => true/false"
puts "   response.worker_id               # => 'worker-123'"
puts "   response.status                  # => 'healthy'"
puts "   response.next_heartbeat_in       # => 30"

# Health Check Response  
puts "\n3. HealthCheckResponse:"
puts "   response.healthy?           # => true/false"
puts "   response.status            # => 'healthy'"
puts "   response.total_workers     # => 5"
puts "   response.uptime_seconds    # => 3600"
puts "   response.diagnostics       # => HealthCheckDiagnostics"

# ========================================================================
# MIGRATION SUMMARY
# ========================================================================

puts "\n" + "=" * 60
puts "📝 MIGRATION SUMMARY"
puts "=" * 60

migration_steps = [
  "1. Replace CommandClient.new with Rust-backed version",
  "2. Replace WorkerManager.new with Rust-backed version", 
  "3. Use OrchestrationManager factory methods for integration",
  "4. Leverage typed responses instead of raw hashes",
  "5. Update error handling for typed response validation",
  "6. Remove Ruby socket dependencies and related code"
]

migration_steps.each { |step| puts "✅ #{step}" }

puts "\n🎉 Benefits of Rust-backed Components:"
benefits = [
  "🚀 Superior performance with zero Ruby socket overhead",
  "🔒 Type safety with dry-struct response validation",
  "🛡️  Enhanced error handling and connection management",
  "🔧 Unified configuration and lifecycle management",
  "📊 Built-in metrics and diagnostics capabilities",
  "🏗️  Future-ready architecture for additional protocols"
]

benefits.each { |benefit| puts "   #{benefit}" }

puts "\n" + "=" * 60
puts "✨ Migration complete! Your Ruby application now uses"
puts "   high-performance Rust-backed command components."
puts "=" * 60