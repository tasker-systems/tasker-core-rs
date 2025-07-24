#!/usr/bin/env ruby
# frozen_string_literal: true

# Simple test to validate ZeroMQ communication pattern
# This validates the basic pub-sub pattern without the full handler infrastructure

require 'ffi-rzmq'
require 'json'

puts "🧪 Testing ZeroMQ Communication Pattern"
puts "=" * 50

begin
  # Create ZeroMQ context
  context = ZMQ::Context.new
  
  # Set up publisher (simulates Rust)
  publisher = context.socket(ZMQ::PUB)
  publisher.bind('inproc://test_steps')
  
  # Set up subscriber (simulates Ruby handler)  
  subscriber = context.socket(ZMQ::SUB)
  subscriber.connect('inproc://test_steps')
  subscriber.setsockopt(ZMQ::SUBSCRIBE, 'steps')
  
  puts "✅ ZeroMQ sockets created and connected"
  
  # Give sockets time to connect
  sleep(0.1)
  
  # Test message sending
  test_message = {
    batch_id: 'test-123',
    protocol_version: '1.0',
    steps: [
      {
        step_id: 456,
        step_name: 'test_step',
        task_id: 123
      }
    ]
  }
  
  message = "steps #{test_message.to_json}"
  puts "📤 Sending message: #{message[0..50]}..."
  
  rc = publisher.send_string(message)
  if ZMQ::Util.resultcode_ok?(rc)
    puts "✅ Message sent successfully"
  else
    puts "❌ Failed to send message: #{ZMQ::Util.error_string}"
  end
  
  # Brief delay for message delivery
  sleep(0.05)
  
  # Test message receiving
  puts "📥 Attempting to receive message..."
  
  received_message = String.new
  rc = subscriber.recv_string(received_message, ZMQ::DONTWAIT)
  
  if ZMQ::Util.resultcode_ok?(rc)
    puts "✅ Message received: #{received_message[0..50]}..."
    
    # Parse the message
    parts = received_message.split(' ', 2)
    if parts.length == 2 && parts[0] == 'steps'
      parsed = JSON.parse(parts[1], symbolize_names: true)
      puts "✅ Message parsed successfully"
      puts "   📊 Batch ID: #{parsed[:batch_id]}"
      puts "   📊 Steps count: #{parsed[:steps]&.length || 0}"
    else
      puts "❌ Message format invalid"
    end
  elsif ZMQ::Util.errno == ZMQ::EAGAIN
    puts "❌ No message available (EAGAIN)"
  else
    puts "❌ Receive error: #{ZMQ::Util.error_string}"
  end
  
  # Cleanup
  subscriber.close
  publisher.close
  context.terminate
  
  puts "\n🎉 ZeroMQ communication test complete"
  
rescue LoadError => e
  puts "❌ ZeroMQ not available: #{e.message}"
  exit 1
rescue => e
  puts "❌ Test failed: #{e.class}: #{e.message}"
  puts e.backtrace.join("\n")
  exit 1
end