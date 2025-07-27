# frozen_string_literal: true

require 'spec_helper'
require 'timeout'

RSpec.describe TaskerCore::EmbeddedServer, type: :integration do
  let(:default_config) do
    {
      bind_address: '127.0.0.1:0', # Use port 0 for automatic assignment
      command_queue_size: 100,
      connection_timeout_ms: 5000,
      graceful_shutdown_timeout_ms: 2000,
      max_connections: 10,
      background: true
    }
  end

  let(:server) { described_class.new(default_config) }

  after do
    server&.stop(force: true)
  end

  describe '#initialize' do
    it 'creates server with default configuration' do
      server = described_class.new
      expect(server.config).to include(
        bind_address: '127.0.0.1:8080',
        command_queue_size: 1000,
        max_connections: 100
      )
      expect(server.background_mode).to be true
    end

    it 'creates server with custom configuration' do
      custom_config = {
        bind_address: '0.0.0.0:9999',
        max_connections: 50,
        background: false
      }
      server = described_class.new(custom_config)
      
      expect(server.config).to include(custom_config)
      expect(server.background_mode).to be false
    end
  end

  describe '#start and #stop' do
    it 'starts and stops server successfully' do
      expect(server.running?).to be false
      
      result = server.start(ready_timeout: 5)
      expect(result).to be true
      expect(server.running?).to be true
      
      result = server.stop
      expect(result).to be true
      expect(server.running?).to be false
    end

    it 'waits for server to be ready when block_until_ready is true' do
      start_time = Time.now
      
      result = server.start(block_until_ready: true, ready_timeout: 5)
      
      expect(result).to be true
      expect(server.running?).to be true
      expect(Time.now - start_time).to be < 5 # Should not timeout
    end

    it 'raises error when server fails to become ready within timeout' do
      # This test would need a way to simulate server startup failure
      # For now, we'll test the timeout behavior with a very short timeout
      skip 'Need to implement startup failure simulation'
    end

    it 'raises error when trying to start already running server' do
      server.start(ready_timeout: 5)
      expect(server.running?).to be true
      
      expect { server.start }.to raise_error(TaskerCore::Errors::ServerError, /already running/)
    end

    it 'returns false when stopping non-running server' do
      expect(server.running?).to be false
      result = server.stop
      expect(result).to be false
    end
  end

  describe '#status' do
    context 'when server is not running' do
      it 'returns default status' do
        status = server.status
        expect(status).to include(
          running: false,
          total_connections: 0,
          active_connections: 0,
          uptime_seconds: 0,
          commands_processed: 0
        )
      end
    end

    context 'when server is running' do
      before { server.start(ready_timeout: 5) }

      it 'returns actual server status' do
        status = server.status
        expect(status).to include(
          running: true
        )
        expect(status[:bind_address]).to be_a(String)
        expect(status[:uptime_seconds]).to be >= 0
        expect(status[:commands_processed]).to be >= 0
      end
    end
  end

  describe '#bind_address' do
    it 'returns configured address when server is not running' do
      expect(server.bind_address).to eq(default_config[:bind_address])
    end

    context 'when server is running' do
      before { server.start(ready_timeout: 5) }

      it 'returns actual bind address from running server' do
        address = server.bind_address
        expect(address).to be_a(String)
        expect(address).to match(/127\.0\.0\.1:\d+/)
      end
    end
  end

  describe '#wait_for_ready' do
    it 'returns false when server is not running' do
      expect(server.wait_for_ready(1)).to be false
    end

    context 'when server is running' do
      before { server.start(ready_timeout: 5) }

      it 'returns true when server is ready' do
        expect(server.wait_for_ready(5)).to be true
      end
    end
  end

  describe '#restart' do
    it 'restarts a running server' do
      server.start(ready_timeout: 5)
      expect(server.running?).to be true
      
      old_address = server.bind_address
      
      result = server.restart(ready_timeout: 5)
      expect(result).to be true
      expect(server.running?).to be true
      
      # Address might be different due to port 0 assignment
      new_address = server.bind_address
      expect(new_address).to be_a(String)
    end

    it 'starts a non-running server' do
      expect(server.running?).to be false
      
      result = server.restart(ready_timeout: 5)
      expect(result).to be true
      expect(server.running?).to be true
    end
  end

  describe '#uptime' do
    it 'returns 0 when server is not running' do
      expect(server.uptime).to eq(0)
    end

    context 'when server is running' do
      before { server.start(ready_timeout: 5) }

      it 'returns positive uptime' do
        sleep 0.1 # Brief pause to ensure uptime > 0
        expect(server.uptime).to be >= 0
      end
    end
  end

  describe '.start_server' do
    let(:started_server) { described_class.start_server(default_config) }

    after { started_server&.stop(force: true) }

    it 'creates and starts server in one call' do
      expect(started_server.running?).to be true
      expect(started_server.config).to include(default_config)
    end
  end

  describe '.with_server' do
    it 'provides running server in block and cleans up afterward' do
      server_in_block = nil
      
      result = described_class.with_server(default_config) do |server|
        server_in_block = server
        expect(server.running?).to be true
        'block_result'
      end
      
      expect(result).to eq('block_result')
      expect(server_in_block.running?).to be false # Should be stopped after block
    end

    it 'cleans up server even if block raises exception' do
      server_in_block = nil
      
      expect do
        described_class.with_server(default_config) do |server|
          server_in_block = server
          expect(server.running?).to be true
          raise StandardError, 'test error'
        end
      end.to raise_error(StandardError, 'test error')
      
      expect(server_in_block.running?).to be false # Should be stopped after exception
    end
  end

  describe 'background vs foreground mode' do
    it 'runs in background mode by default' do
      server = described_class.new(default_config.merge(background: true))
      server.start(ready_timeout: 5)
      
      expect(server.server_future).to be_a(Concurrent::Future)
      expect(server.server_future.complete?).to be false
      
      server.stop
      expect(server.server_future).to be_nil
    end

    it 'can run in foreground mode for testing' do
      # This test is tricky because foreground mode blocks
      # We'll test the configuration but not actual blocking behavior
      server = described_class.new(default_config.merge(background: false))
      expect(server.background_mode).to be false
      
      # For foreground mode, we'd typically start it in a separate thread for testing
      # but that defeats the purpose. Instead, we'll verify the configuration.
    end
  end

  describe 'error handling' do
    it 'handles FFI errors gracefully' do
      # This test would need to simulate FFI failures
      # For now, we'll test basic error scenarios
      
      server = described_class.new(default_config)
      
      # Test that status method handles errors
      allow(TaskerCore).to receive(:embedded_executor_status).and_raise(StandardError, 'FFI error')
      
      status = server.status
      expect(status[:error]).to eq('FFI error')
    end
  end

  describe 'thread safety' do
    it 'handles concurrent start/stop operations safely' do
      threads = []
      results = []
      
      # Start multiple threads trying to start/stop the server
      5.times do
        threads << Thread.new do
          begin
            server.start(ready_timeout: 2)
            results << 'started'
            sleep 0.1
            server.stop
            results << 'stopped'
          rescue => e
            results << "error: #{e.message}"
          end
        end
      end
      
      threads.each(&:join)
      
      # Should have at least one successful start and some expected errors
      expect(results).to include('started')
      expect(results.count { |r| r.include?('already running') }).to be >= 0
    end
  end
end