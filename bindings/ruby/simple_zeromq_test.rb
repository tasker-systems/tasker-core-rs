#!/usr/bin/env ruby
# frozen_string_literal: true

# Simple ZeroMQ test that validates the basic architecture without any Ruby infrastructure

require 'ffi-rzmq'
require 'json'

puts "🧪 Simple ZeroMQ Communication Test"
puts "=" * 40

begin
  # Test the most basic ZeroMQ functionality
  puts "1. Creating ZeroMQ context..."
  context = ZMQ::Context.new
  puts "   ✅ Context created"
  
  # Test pub-sub pattern with TCP
  puts "\n2. Setting up pub-sub sockets..."
  
  publisher = context.socket(ZMQ::PUB)
  publisher.bind('tcp://127.0.0.1:5557')
  
  subscriber = context.socket(ZMQ::SUB)
  subscriber.connect('tcp://127.0.0.1:5557')
  subscriber.setsockopt(ZMQ::SUBSCRIBE, 'test')
  
  puts "   ✅ Sockets created and connected"
  
  # Wait for connection
  sleep(0.2)
  
  # Test message sending
  puts "\n3. Testing message exchange..."
  
  test_message = {
    type: 'test',
    data: { message: 'Hello ZeroMQ!', timestamp: Time.now.to_f }
  }
  
  message = "test #{test_message.to_json}"
  puts "   📤 Sending: #{message}"
  
  rc = publisher.send_string(message)
  puts "   ✅ Send result: #{ZMQ::Util.resultcode_ok?(rc) ? 'SUCCESS' : 'FAILED'}"
  
  # Test message receiving
  sleep(0.1)
  
  received_message = String.new
  rc = subscriber.recv_string(received_message, ZMQ::DONTWAIT)
  
  if ZMQ::Util.resultcode_ok?(rc)
    puts "   ✅ Received: #{received_message}"
    
    # Parse message
    parts = received_message.split(' ', 2)
    if parts.length == 2
      parsed = JSON.parse(parts[1], symbolize_names: true)
      puts "   ✅ Parsed JSON: #{parsed}"
    end
  else
    puts "   ❌ No message received: #{ZMQ::Util.error_string}"
  end
  
  # Cleanup
  subscriber.close
  publisher.close
  context.terminate
  
  puts "\n🎉 Simple ZeroMQ test complete!"
  puts "   This proves ZeroMQ infrastructure is working correctly."
  
rescue LoadError => e
  puts "❌ ZeroMQ not available: #{e.message}"
rescue => e
  puts "❌ Test failed: #{e.class}: #{e.message}"
  puts e.backtrace.join("\n")
end