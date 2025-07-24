#!/usr/bin/env ruby
# frozen_string_literal: true

# Simplified ZeroMQ message flow test
# Tests basic message communication without full step handler infrastructure

begin
  require 'bundler/setup'
rescue LoadError
end

require 'ffi-rzmq'
require 'json'
require 'timeout'
require 'logger'

puts "🔬 ZeroMQ Simple Message Flow Test"
puts "=" * 50

# Test configuration
STEP_ENDPOINT = 'inproc://simple_test_steps'
RESULT_ENDPOINT = 'inproc://simple_test_results'

# Simple message processor that bypasses step handler infrastructure
class SimpleMessageProcessor
  def initialize(logger: nil)
    @logger = logger || Logger.new($stdout, level: Logger::INFO)
  end

  def process_step_message(message)
    parts = message.split(' ', 2)
    unless parts.length == 2 && parts[0] == 'steps'
      @logger.warn "Invalid message format: #{message[0..50]}"
      return nil
    end

    topic, json_data = parts
    request = JSON.parse(json_data, symbolize_names: true)
    
    @logger.info "Processing simple batch #{request[:batch_id]} with #{request[:steps]&.length || 0} steps"

    # Process each step with a simple mock response
    results = (request[:steps] || []).map do |step|
      {
        step_id: step[:step_id],
        status: 'completed',
        output: { message: "Mock execution of #{step[:step_name]}", timestamp: Time.now.to_i },
        error: nil,
        metadata: {
          execution_time_ms: rand(10..50),
          handler_version: '1.0.0-test',
          retryable: false,
          completed_at: Time.now.utc.strftime('%Y-%m-%dT%H:%M:%S.%LZ')
        }
      }
    end

    response = {
      batch_id: request[:batch_id],
      protocol_version: request[:protocol_version] || '1.0',
      results: results
    }

    @logger.info "Generated mock response for batch #{request[:batch_id]}"
    response
  rescue JSON::ParserError => e
    @logger.error "JSON parse error: #{e.message}"
    nil
  rescue => e
    @logger.error "Processing error: #{e.message}"
    nil
  end
end

begin
  puts "1. Setting up ZeroMQ simple test..."
  
  context = ZMQ::Context.new
  
  # Publisher (simulates Rust)
  pub_socket = context.socket(ZMQ::PUB)
  pub_socket.bind(STEP_ENDPOINT)
  
  # Subscriber for results (simulates Rust)
  sub_socket = context.socket(ZMQ::SUB)
  sub_socket.connect(RESULT_ENDPOINT)
  sub_socket.setsockopt(ZMQ::SUBSCRIBE, 'results')
  
  # Step subscriber (simulates Ruby handler)
  step_sub_socket = context.socket(ZMQ::SUB)
  step_sub_socket.connect(STEP_ENDPOINT)
  step_sub_socket.setsockopt(ZMQ::SUBSCRIBE, 'steps')
  
  # Result publisher (simulates Ruby handler)
  result_pub_socket = context.socket(ZMQ::PUB)
  result_pub_socket.bind(RESULT_ENDPOINT)
  
  puts "   ✅ ZeroMQ sockets configured"
  
  # Give sockets time to connect
  sleep(0.1)

  # Create simple message processor
  processor = SimpleMessageProcessor.new

  puts "\n2. Testing message flow..."
  
  # Create test message
  test_request = {
    batch_id: "simple-test-#{Time.now.to_i}",
    protocol_version: '1.0',
    steps: [
      {
        step_id: 101,
        step_name: 'simple_test_step',
        task_id: 201,
        task_context: { test: true },
        handler_config: {},
        previous_results: {},
        metadata: { simple_test: true }
      }
    ]
  }
  
  message = "steps #{test_request.to_json}"
  
  puts "   📤 Publishing test message..."
  puts "   📋 Batch ID: #{test_request[:batch_id]}"
  
  # Publish message
  pub_socket.send_string(message)
  
  # Receive and process message
  puts "   📥 Waiting for message on step subscriber..."
  
  message_received = false
  response_sent = false
  
  Timeout.timeout(3) do
    while !message_received
      # Check for step message
      step_message = String.new
      rc = step_sub_socket.recv_string(step_message, ZMQ::DONTWAIT)
      
      if ZMQ::Util.resultcode_ok?(rc)
        puts "   📨 Received step message: #{step_message[0..100]}#{'...' if step_message.length > 100}"
        message_received = true
        
        # Process message
        response = processor.process_step_message(step_message)
        
        if response
          result_message = "results #{response.to_json}"
          puts "   📤 Publishing result message..."
          
          result_pub_socket.send_string(result_message)
          response_sent = true
          
          puts "   ✅ Response sent successfully"
        else
          puts "   ❌ Failed to process message"
        end
      else
        sleep(0.01) # Brief pause before retrying
      end
    end
  end
  
  if message_received && response_sent
    puts "\n3. Checking for result on result subscriber..."
    
    # Wait for result
    result_received = false
    
    Timeout.timeout(3) do
      while !result_received
        result_message = String.new
        rc = sub_socket.recv_string(result_message, ZMQ::DONTWAIT)
        
        if ZMQ::Util.resultcode_ok?(rc)
          parts = result_message.split(' ', 2)
          if parts.length == 2 && parts[0] == 'results'
            response_data = JSON.parse(parts[1], symbolize_names: true)
            result_received = true
            
            puts "   📥 Received result message!"
            puts "   📋 Batch ID: #{response_data[:batch_id]}"
            puts "   📋 Results: #{response_data[:results]&.length || 0}"
            
            if response_data[:results]&.any?
              result = response_data[:results].first
              puts "   📋 First Result:"
              puts "       • Step ID: #{result[:step_id]}"
              puts "       • Status: #{result[:status]}"
              puts "       • Output: #{result[:output]}"
              puts "       • Execution Time: #{result[:metadata][:execution_time_ms]}ms"
            end
            
            puts "\n🎉 Simple ZeroMQ message flow working perfectly!"
          end
        else
          sleep(0.01) # Brief pause before retrying
        end
      end
    end
  end

rescue Timeout::Error
  puts "\n❌ Test timed out"
  if message_received
    puts "   ✅ Step message was received and processed"
    if response_sent
      puts "   ✅ Response was sent"
      puts "   ❌ But result was not received on subscriber"
    else
      puts "   ❌ But response was not sent"
    end
  else
    puts "   ❌ Step message was not received"
  end
  
rescue => e
  puts "\n❌ Test error: #{e.class}: #{e.message}"
  puts e.backtrace.join("\n")
  
ensure
  puts "\n4. Cleaning up..."
  
  if pub_socket || sub_socket || step_sub_socket || result_pub_socket || context
    pub_socket&.close
    sub_socket&.close
    step_sub_socket&.close
    result_pub_socket&.close
    context&.terminate
    puts "   ✅ ZeroMQ cleanup complete"
  end
end

puts "\nTest Summary:"
puts "- This test validates basic ZeroMQ pub-sub communication"
puts "- Uses simplified message processing without step handler infrastructure"
puts "- Tests bidirectional message flow (steps → results)"
puts "- Confirms message protocol compatibility"