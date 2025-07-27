# frozen_string_literal: true

require 'spec_helper'
require 'tasker_core/execution'

RSpec.describe TaskerCore::Execution::CommandClient do
  let(:host) { 'localhost' }
  let(:port) { 8080 }
  let(:logger) { instance_double(Logger) }
  let(:client) { described_class.new(host: host, port: port, logger: logger) }

  before do
    # Stub logger calls
    allow(logger).to receive(:info)
    allow(logger).to receive(:debug)
    allow(logger).to receive(:warn)
    allow(logger).to receive(:error)
  end

  describe '#initialize' do
    it 'sets default configuration' do
      default_client = described_class.new

      expect(default_client.host).to eq('localhost')
      expect(default_client.port).to eq(8080)
      expect(default_client.timeout).to eq(30)
      expect(default_client.connected).to be false
    end

    it 'accepts custom configuration' do
      custom_client = described_class.new(
        host: 'custom-host',
        port: 9090,
        timeout: 60,
        logger: logger
      )

      expect(custom_client.host).to eq('custom-host')
      expect(custom_client.port).to eq(9090)
      expect(custom_client.timeout).to eq(60)
      expect(custom_client.logger).to eq(logger)
    end
  end

  describe '#connect' do
    let(:mock_socket) { instance_double(TCPSocket) }

    context 'when connection succeeds' do
      before do
        allow(TCPSocket).to receive(:new).with(host, port).and_return(mock_socket)
        allow(mock_socket).to receive(:setsockopt)
      end

      it 'establishes connection and marks as connected' do
        result = client.connect

        expect(result).to be true
        expect(client.connected?).to be true
        expect(client.socket).to eq(mock_socket)
      end

      it 'logs successful connection' do
        expect(logger).to receive(:info).with("Connected to Rust TCP executor at #{host}:#{port}")
        
        client.connect
      end

      it 'returns true if already connected' do
        client.connect
        
        expect(client.connect).to be true
        expect(TCPSocket).to have_received(:new).once
      end
    end

    context 'when connection fails' do
      before do
        allow(TCPSocket).to receive(:new).and_raise(StandardError.new('Connection refused'))
      end

      it 'raises ConnectionError' do
        expect { client.connect }.to raise_error(
          TaskerCore::Execution::ConnectionError,
          'Failed to connect: Connection refused'
        )
      end

      it 'logs connection failure' do
        expect(logger).to receive(:error).with(/Failed to connect/)
        
        expect { client.connect }.to raise_error(TaskerCore::Execution::ConnectionError)
      end

      it 'marks as not connected' do
        expect { client.connect }.to raise_error(TaskerCore::Execution::ConnectionError)
        expect(client.connected?).to be false
      end
    end
  end

  describe '#disconnect' do
    let(:mock_socket) { instance_double(TCPSocket) }

    before do
      allow(TCPSocket).to receive(:new).and_return(mock_socket)
      allow(mock_socket).to receive(:setsockopt)
      allow(mock_socket).to receive(:close)
      client.connect
    end

    it 'closes socket and marks as disconnected' do
      client.disconnect

      expect(mock_socket).to have_received(:close)
      expect(client.connected?).to be false
      expect(client.socket).to be_nil
    end

    it 'logs disconnection' do
      expect(logger).to receive(:info).with('Disconnected from Rust TCP executor')
      
      client.disconnect
    end

    it 'handles socket close errors gracefully' do
      allow(mock_socket).to receive(:close).and_raise(StandardError.new('Close error'))
      expect(logger).to receive(:warn).with(/Error closing socket/)
      
      client.disconnect
      expect(client.connected?).to be false
    end

    it 'handles no socket gracefully' do
      client.instance_variable_set(:@socket, nil)
      
      expect { client.disconnect }.not_to raise_error
    end
  end

  describe '#register_worker' do
    let(:mock_socket) { instance_double(TCPSocket) }
    let(:worker_capabilities) do
      {
        worker_id: 'test_worker',
        max_concurrent_steps: 10,
        supported_namespaces: ['orders', 'payments'],
        step_timeout_ms: 30000,
        supports_retries: true,
        language_runtime: 'ruby',
        version: '3.1.0'
      }
    end

    let(:mock_response) do
      {
        command_type: 'WorkerRegistered',
        command_id: 'response_123',
        correlation_id: 'ruby_cmd_123',
        payload: {
          WorkerRegistered: {
            worker_id: 'test_worker',
            assigned_pool: 'default',
            queue_position: 1
          }
        }
      }
    end

    before do
      allow(TCPSocket).to receive(:new).and_return(mock_socket)
      allow(mock_socket).to receive(:setsockopt)
      allow(mock_socket).to receive(:puts)
      allow(mock_socket).to receive(:gets).and_return(JSON.generate(mock_response))
      client.connect
    end

    it 'sends registration command and returns response' do
      response = client.register_worker(**worker_capabilities)

      expect(mock_socket).to have_received(:puts) do |json_command|
        command = JSON.parse(json_command, symbolize_names: true)
        expect(command[:command_type]).to eq('RegisterWorker')
        expect(command[:payload][:RegisterWorker][:worker_capabilities][:worker_id]).to eq('test_worker')
        expect(command[:payload][:RegisterWorker][:worker_capabilities][:max_concurrent_steps]).to eq(10)
      end

      expect(response[:command_type]).to eq('WorkerRegistered')
      expect(response[:payload][:WorkerRegistered][:worker_id]).to eq('test_worker')
    end

    it 'includes all worker capabilities in command' do
      client.register_worker(**worker_capabilities)

      expect(mock_socket).to have_received(:puts) do |json_command|
        command = JSON.parse(json_command, symbolize_names: true)
        capabilities = command[:payload][:RegisterWorker][:worker_capabilities]
        
        expect(capabilities[:supported_namespaces]).to eq(['orders', 'payments'])
        expect(capabilities[:step_timeout_ms]).to eq(30000)
        expect(capabilities[:supports_retries]).to be true
        expect(capabilities[:language_runtime]).to eq('ruby')
        expect(capabilities[:version]).to eq('3.1.0')
      end
    end

    it 'includes custom capabilities when provided' do
      custom_capabilities = { 'feature_flags' => ['flag1', 'flag2'] }
      
      client.register_worker(**worker_capabilities, custom_capabilities: custom_capabilities)

      expect(mock_socket).to have_received(:puts) do |json_command|
        command = JSON.parse(json_command, symbolize_names: true)
        capabilities = command[:payload][:RegisterWorker][:worker_capabilities]
        
        expect(capabilities[:custom_capabilities]).to eq(custom_capabilities)
      end
    end

    it 'raises NotConnectedError when not connected' do
      client.disconnect

      expect { client.register_worker(**worker_capabilities) }.to raise_error(
        TaskerCore::Execution::NotConnectedError,
        'Client not connected'
      )
    end
  end

  describe '#send_heartbeat' do
    let(:mock_socket) { instance_double(TCPSocket) }
    let(:heartbeat_params) do
      {
        worker_id: 'test_worker',
        current_load: 5,
        system_stats: {
          cpu_usage_percent: 45.0,
          memory_usage_mb: 1024,
          active_connections: 3,
          uptime_seconds: 3600
        }
      }
    end

    let(:mock_response) do
      {
        command_type: 'HeartbeatAcknowledged',
        command_id: 'response_456',
        payload: {
          HeartbeatAcknowledged: {
            worker_id: 'test_worker',
            status: 'healthy'
          }
        }
      }
    end

    before do
      allow(TCPSocket).to receive(:new).and_return(mock_socket)
      allow(mock_socket).to receive(:setsockopt)
      allow(mock_socket).to receive(:puts)
      allow(mock_socket).to receive(:gets).and_return(JSON.generate(mock_response))
      client.connect
    end

    it 'sends heartbeat command with load and stats' do
      response = client.send_heartbeat(**heartbeat_params)

      expect(mock_socket).to have_received(:puts) do |json_command|
        command = JSON.parse(json_command, symbolize_names: true)
        expect(command[:command_type]).to eq('WorkerHeartbeat')
        expect(command[:payload][:WorkerHeartbeat][:worker_id]).to eq('test_worker')
        expect(command[:payload][:WorkerHeartbeat][:current_load]).to eq(5)
        expect(command[:payload][:WorkerHeartbeat][:system_stats][:cpu_usage_percent]).to eq(45.0)
      end

      expect(response[:command_type]).to eq('HeartbeatAcknowledged')
    end

    it 'works without system stats' do
      client.send_heartbeat(worker_id: 'test_worker', current_load: 3)

      expect(mock_socket).to have_received(:puts) do |json_command|
        command = JSON.parse(json_command, symbolize_names: true)
        expect(command[:payload][:WorkerHeartbeat][:system_stats]).to be_nil
      end
    end
  end

  describe '#health_check' do
    let(:mock_socket) { instance_double(TCPSocket) }
    let(:mock_response) do
      {
        command_type: 'HealthCheckResult',
        payload: {
          HealthCheckResult: {
            status: 'healthy',
            uptime_seconds: 3600,
            total_workers: 5,
            active_commands: 10
          }
        }
      }
    end

    before do
      allow(TCPSocket).to receive(:new).and_return(mock_socket)
      allow(mock_socket).to receive(:setsockopt)
      allow(mock_socket).to receive(:puts)
      allow(mock_socket).to receive(:gets).and_return(JSON.generate(mock_response))
      client.connect
    end

    it 'sends health check command' do
      response = client.health_check

      expect(mock_socket).to have_received(:puts) do |json_command|
        command = JSON.parse(json_command, symbolize_names: true)
        expect(command[:command_type]).to eq('HealthCheck')
        expect(command[:payload][:HealthCheck][:diagnostic_level]).to eq('Basic')
      end

      expect(response[:command_type]).to eq('HealthCheckResult')
    end

    it 'accepts custom diagnostic level' do
      client.health_check(diagnostic_level: 'Detailed')

      expect(mock_socket).to have_received(:puts) do |json_command|
        command = JSON.parse(json_command, symbolize_names: true)
        expect(command[:payload][:HealthCheck][:diagnostic_level]).to eq('Detailed')
      end
    end
  end

  describe 'error handling' do
    let(:mock_socket) { instance_double(TCPSocket) }

    before do
      allow(TCPSocket).to receive(:new).and_return(mock_socket)
      allow(mock_socket).to receive(:setsockopt)
      allow(mock_socket).to receive(:puts)
      client.connect
    end

    it 'handles timeout errors' do
      allow(mock_socket).to receive(:gets).and_return(nil)
      allow(Timeout).to receive(:timeout).and_raise(Timeout::Error)

      expect { client.health_check }.to raise_error(
        TaskerCore::Execution::CommandError,
        'Command timed out'
      )
    end

    it 'handles invalid JSON responses' do
      allow(mock_socket).to receive(:gets).and_return('invalid json')

      expect { client.health_check }.to raise_error(
        TaskerCore::Execution::CommandError,
        /Invalid response format/
      )
    end

    it 'handles no response' do
      allow(mock_socket).to receive(:gets).and_return(nil)

      expect { client.health_check }.to raise_error(
        TaskerCore::Execution::CommandError,
        /No response received/
      )
    end
  end
end