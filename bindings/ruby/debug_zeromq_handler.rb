#!/usr/bin/env ruby
# frozen_string_literal: true

# Debug script to understand why ZeroMQ handler is not processing messages

require_relative 'lib/tasker_core'

# Explicitly require the execution module to ensure it's loaded
require_relative 'lib/tasker_core/execution'

puts "🔍 Debugging ZeroMQ Handler Message Processing"
puts "=" * 60

begin
  # Test 1: Check if handler receives messages at all
  puts "1. Testing basic handler setup..."
  
  handler = TaskerCore::Execution::ZeroMQHandler.new(
    step_sub_endpoint: 'inproc://debug_steps',
    result_pub_endpoint: 'inproc://debug_results',
    logger: Logger.new($stdout, level: Logger::DEBUG)
  )
  
  # Start handler
  puts "   Starting handler..."
  handler.start
  puts "   Handler running: #{handler.running?}"
  
  # Give it time to start
  sleep(0.2)
  
  # Test 2: Send a message from another socket
  puts "\n2. Testing message sending..."
  
  context = ZMQ::Context.new
  publisher = context.socket(ZMQ::PUB)
  publisher.bind('inproc://debug_steps')
  
  # Wait for connection
  sleep(0.1)
  
  test_message = {
    batch_id: 'debug-test',
    protocol_version: '1.0',
    steps: [{ step_id: 999, step_name: 'debug_step', task_id: 888 }]
  }
  
  message = "steps #{test_message.to_json}"
  puts "   Sending: #{message[0..50]}..."
  
  rc = publisher.send_string(message)
  puts "   Send result: #{ZMQ::Util.resultcode_ok?(rc) ? 'SUCCESS' : 'FAILED'}"
  
  # Test 3: Check if handler processes the message
  puts "\n3. Waiting for handler to process message..."
  
  # Set up result listener
  result_subscriber = context.socket(ZMQ::SUB)
  result_subscriber.connect('inproc://debug_results')
  result_subscriber.setsockopt(ZMQ::SUBSCRIBE, 'results')
  
  sleep(0.1)
  
  # Wait for response
  timeout_count = 0
  max_timeout = 30 # 3 seconds
  
  while timeout_count < max_timeout
    message = String.new
    rc = result_subscriber.recv_string(message, ZMQ::DONTWAIT)
    
    if ZMQ::Util.resultcode_ok?(rc)
      puts "   ✅ Received response: #{message[0..100]}..."
      break
    elsif ZMQ::Util.errno == ZMQ::EAGAIN
      # No message yet, keep waiting
      timeout_count += 1
      sleep(0.1)
    else
      puts "   ❌ Receive error: #{ZMQ::Util.error_string}"
      break
    end
  end
  
  if timeout_count >= max_timeout
    puts "   ⏰ Timeout - no response received after 3 seconds"
    puts "   This suggests the handler is not processing messages"
  end
  
  # Test 4: Check handler internals
  puts "\n4. Handler status check..."
  puts "   Running: #{handler.running?}"
  puts "   Thread alive: #{handler.instance_variable_get(:@thread)&.alive?}"
  
  # Cleanup
  result_subscriber.close
  publisher.close
  context.terminate
  
  puts "\n5. Stopping handler..."
  stopped = handler.stop(timeout: 2)
  puts "   Stopped gracefully: #{stopped}"
  
rescue LoadError => e
  puts "❌ ZeroMQ not available: #{e.message}"
rescue => e
  puts "❌ Error: #{e.class}: #{e.message}"
  puts e.backtrace.join("\n")
end

puts "\n🔍 Debug complete"