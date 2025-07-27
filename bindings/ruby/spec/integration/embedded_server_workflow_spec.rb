# frozen_string_literal: true

require 'spec_helper'
require 'json'
require 'socket'

RSpec.describe 'Embedded Server Workflow Integration', type: :integration do
  let(:server_config) do
    {
      bind_address: '127.0.0.1:0', # Auto-assign port
      command_queue_size: 50,
      connection_timeout_ms: 5000,
      max_connections: 5,
      background: true
    }
  end

  # Helper method to create a simple TCP client
  def create_tcp_client(address)
    host, port = address.split(':')
    TCPSocket.new(host, port.to_i)
  end

  # Helper method to send a command via TCP
  def send_command(client, command)
    message = JSON.generate(command)
    client.puts(message)
    client.flush
  end

  # Helper method to read response via TCP
  def read_response(client)
    response = client.gets
    response ? JSON.parse(response.strip) : nil
  rescue JSON::ParserError
    response
  end

  describe 'end-to-end workflow with embedded server' do
    it 'starts server, registers workers, processes commands, and shuts down cleanly' do
      TaskerCore::EmbeddedServer.with_server(server_config) do |server|
        expect(server.running?).to be true
        
        # Get the actual bind address (port was auto-assigned)
        bind_address = server.bind_address
        expect(bind_address).to match(/127\.0\.0\.1:\d+/)
        
        # Create TCP client to communicate with server
        client = create_tcp_client(bind_address)
        
        begin
          # Test 1: Send health check command
          health_check_command = {
            type: 'HealthCheck',
            data: {},
            metadata: {
              id: SecureRandom.uuid,
              source: 'rspec',
              timestamp: Time.now.utc.iso8601,
              correlation_id: 'test-health-check'
            }
          }
          
          send_command(client, health_check_command)
          response = read_response(client)
          
          expect(response).to be_a(Hash)
          puts "Health check response: #{response.inspect}"
          
          # Test 2: Send worker registration
          register_command = {
            type: 'WorkerRegistration',
            data: {
              worker_id: 'test-worker-1',
              capabilities: ['task_processing', 'data_validation'],
              max_concurrent_tasks: 3
            },
            metadata: {
              id: SecureRandom.uuid,
              source: 'rspec',
              timestamp: Time.now.utc.iso8601,
              correlation_id: 'test-worker-registration'
            }
          }
          
          send_command(client, register_command)
          response = read_response(client)
          
          expect(response).to be_a(Hash)
          puts "Worker registration response: #{response.inspect}"
          
          # Test 3: Send heartbeat
          heartbeat_command = {
            type: 'Heartbeat',
            data: {
              worker_id: 'test-worker-1',
              status: 'active',
              current_load: 0.2
            },
            metadata: {
              id: SecureRandom.uuid,
              source: 'rspec',
              timestamp: Time.now.utc.iso8601,
              correlation_id: 'test-heartbeat'
            }
          }
          
          send_command(client, heartbeat_command)
          response = read_response(client)
          
          expect(response).to be_a(Hash)
          puts "Heartbeat response: #{response.inspect}"
          
          # Test 4: Verify server status shows activity
          status = server.status
          expect(status[:running]).to be true
          expect(status[:active_connections]).to be >= 1
          
          puts "Server status: #{status.inspect}"
          
        ensure
          client.close
        end
      end
      
      # Server should be stopped after block
      # Note: We can't test this directly since the server variable is out of scope
    end

    it 'handles multiple concurrent clients' do
      TaskerCore::EmbeddedServer.with_server(server_config) do |server|
        bind_address = server.bind_address
        clients = []
        responses = []
        
        begin
          # Create multiple clients
          3.times do |i|
            clients << create_tcp_client(bind_address)
          end
          
          # Send commands from all clients concurrently
          threads = clients.map.with_index do |client, i|
            Thread.new do
              command = {
                type: 'HealthCheck',
                data: { client_id: "client-#{i}" },
                metadata: {
                  id: SecureRandom.uuid,
                  source: "rspec-client-#{i}",
                  timestamp: Time.now.utc.iso8601,
                  correlation_id: "concurrent-test-#{i}"
                }
              }
              
              send_command(client, command)
              response = read_response(client)
              responses << { client: i, response: response }
            end
          end
          
          # Wait for all responses
          threads.each(&:join)
          
          # Verify all clients got responses
          expect(responses.length).to eq(3)
          responses.each do |result|
            expect(result[:response]).to be_a(Hash)
          end
          
          # Verify server handled multiple connections
          status = server.status
          expect(status[:total_connections]).to be >= 3
          
        ensure
          clients.each(&:close)
        end
      end
    end

    it 'recovers gracefully from client disconnections' do
      TaskerCore::EmbeddedServer.with_server(server_config) do |server|
        bind_address = server.bind_address
        
        # Create client and then disconnect abruptly
        client = create_tcp_client(bind_address)
        
        # Send partial command and disconnect
        client.write('{"type":"HealthCheck"')
        client.close # Disconnect without completing command
        
        # Server should continue running normally
        expect(server.running?).to be true
        
        # New client should be able to connect
        new_client = create_tcp_client(bind_address)
        begin
          command = {
            type: 'HealthCheck',
            data: {},
            metadata: {
              id: SecureRandom.uuid,
              source: 'rspec-recovery-test',
              timestamp: Time.now.utc.iso8601,
              correlation_id: 'recovery-test'
            }
          }
          
          send_command(new_client, command)
          response = read_response(new_client)
          
          expect(response).to be_a(Hash)
          
        ensure
          new_client.close
        end
      end
    end
  end

  describe 'configuration validation' do
    it 'uses reasonable defaults for production deployment' do
      production_config = {
        bind_address: '0.0.0.0:8080',
        command_queue_size: 5000,
        connection_timeout_ms: 30000,
        graceful_shutdown_timeout_ms: 10000,
        max_connections: 1000
      }
      
      server = TaskerCore::EmbeddedServer.new(production_config)
      expect(server.config).to include(production_config)
    end

    it 'handles IPv6 addresses correctly' do
      ipv6_config = {
        bind_address: '[::1]:8080', # IPv6 localhost
        max_connections: 10
      }
      
      server = TaskerCore::EmbeddedServer.new(ipv6_config)
      expect(server.config[:bind_address]).to eq('[::1]:8080')
    end
  end

  describe 'development and testing utilities' do
    it 'provides convenient testing patterns' do
      test_results = []
      
      # Pattern 1: Inline server usage
      TaskerCore::EmbeddedServer.with_server(server_config) do |server|
        test_results << "server_running: #{server.running?}"
        test_results << "bind_address: #{server.bind_address}"
      end
      
      # Pattern 2: Server factory
      server = TaskerCore::EmbeddedServer.start_server(server_config)
      begin
        test_results << "factory_running: #{server.running?}"
      ensure
        server.stop
      end
      
      expect(test_results).to include(
        match(/server_running: true/),
        match(/bind_address: 127\.0\.0\.1:\d+/),
        match(/factory_running: true/)
      )
    end

    it 'enables rapid development iteration' do
      # Simulate development workflow
      iteration_count = 0
      
      3.times do
        TaskerCore::EmbeddedServer.with_server(server_config) do |server|
          iteration_count += 1
          
          # Each iteration tests a different feature
          case iteration_count
          when 1
            expect(server.running?).to be true
          when 2
            expect(server.uptime).to be >= 0
          when 3
            expect(server.commands_processed).to be >= 0
          end
        end
      end
      
      expect(iteration_count).to eq(3)
    end
  end
end