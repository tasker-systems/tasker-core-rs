#!/usr/bin/env ruby
require 'socket'
require 'json'
require 'time'

# Test worker registration specifically
socket = TCPSocket.new('localhost', 8080)
puts "Connected to TCP executor"

# Create a worker registration command
command = {
  command_type: 'RegisterWorker',
  command_id: 'test_register_123',
  correlation_id: nil,
  metadata: {
    timestamp: Time.now.utc.iso8601,
    source: {
      type: 'RubyWorker',
      data: {
        id: 'test_worker'
      }
    },
    target: nil,
    timeout_ms: 5000,
    retry_policy: nil,
    namespace: nil,
    priority: nil
  },
  payload: {
    type: 'RegisterWorker',
    data: {
      worker_capabilities: {
        worker_id: 'test_worker',
        max_concurrent_steps: 5,
        supported_namespaces: ['test_namespace'],
        step_timeout_ms: 30000,
        supports_retries: true,
        language_runtime: 'ruby',
        version: RUBY_VERSION,
        custom_capabilities: {}
      }
    }
  }
}

json_cmd = JSON.generate(command)
puts "Sending worker registration command:"
puts json_cmd
puts ""

socket.puts(json_cmd)
socket.flush

puts "Waiting for response..."
if IO.select([socket], nil, nil, 5)
  response = socket.gets
  if response
    puts "Received response:"
    puts response
    puts ""
    parsed = JSON.parse(response, symbolize_names: true)
    puts "Parsed response structure:"
    puts JSON.pretty_generate(parsed)
  else
    puts "No response received"
  end
else
  puts "Timeout waiting for response"
end

socket.close
puts "Connection closed"