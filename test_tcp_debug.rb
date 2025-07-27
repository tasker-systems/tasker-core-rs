#!/usr/bin/env ruby
require 'socket'
require 'json'
require 'time'

# Test direct TCP connection with debug output
socket = TCPSocket.new('localhost', 8080)
puts "Connected to TCP executor"

# Create a health check command matching Rust structure
command = {
  command_type: 'HealthCheck',
  command_id: 'debug_test_123',
  correlation_id: nil,
  metadata: {
    timestamp: Time.now.utc.iso8601,
    source: {
      type: 'RubyWorker',
      data: {
        id: 'debug_client'
      }
    },
    target: nil,
    timeout_ms: 5000,
    retry_policy: nil,
    namespace: nil,
    priority: nil
  },
  payload: {
    type: 'HealthCheck',
    data: {
      diagnostic_level: 'Basic'
    }
  }
}

json_cmd = JSON.generate(command)
puts "Sending command: #{json_cmd}"
socket.puts(json_cmd)
socket.flush

puts "Waiting for response..."
if IO.select([socket], nil, nil, 5)
  response = socket.gets
  if response
    puts "Received response: #{response}"
    parsed = JSON.parse(response, symbolize_names: true)
    puts "Parsed response: #{parsed.inspect}"
  else
    puts "No response received (gets returned nil)"
  end
else
  puts "Timeout waiting for response"
end

socket.close
puts "Connection closed"