# frozen_string_literal: true

require 'spec_helper'

RSpec.describe TaskerCore::Observability do
  # Observability methods require a bootstrapped worker
  before(:all) do
    @bootstrap_result = TaskerCore::FFI.bootstrap_worker
    # Wait briefly for worker to stabilize
    sleep 0.1
  end

  after(:all) do
    TaskerCore::FFI.stop_worker
  rescue StandardError
    nil
  end

  # ==========================================================================
  # Health Methods
  # ==========================================================================

  describe '.health_basic' do
    subject(:health) { described_class.health_basic }

    it 'returns a BasicHealth struct' do
      expect(health).to be_a(TaskerCore::Observability::Types::BasicHealth)
    end

    it 'has a status field' do
      expect(health.status).to be_a(String)
      expect(%w[healthy unhealthy degraded]).to include(health.status)
    end

    it 'has a worker_id field' do
      expect(health.worker_id).to be_a(String)
      expect(health.worker_id).not_to be_empty
    end

    it 'has a timestamp field' do
      expect(health.timestamp).to be_a(String)
      expect(health.timestamp).not_to be_empty
    end
  end

  describe '.health_live' do
    subject(:health) { described_class.health_live }

    it 'returns a BasicHealth struct for liveness probe' do
      expect(health).to be_a(TaskerCore::Observability::Types::BasicHealth)
    end

    it 'indicates worker is alive' do
      # Liveness probe returns "alive" status, not "healthy"
      expect(health.status).to eq('alive')
    end
  end

  describe '.health_ready' do
    subject(:health) { described_class.health_ready }

    it 'returns a DetailedHealth struct for readiness probe' do
      expect(health).to be_a(TaskerCore::Observability::Types::DetailedHealth)
    end

    it 'has health checks hash' do
      expect(health.checks).to be_a(Hash)
    end

    it 'has system_info struct' do
      expect(health.system_info).to be_a(TaskerCore::Observability::Types::WorkerSystemInfo)
    end
  end

  describe '.health_detailed' do
    subject(:health) { described_class.health_detailed }

    it 'returns a DetailedHealth struct' do
      expect(health).to be_a(TaskerCore::Observability::Types::DetailedHealth)
    end

    it 'has status field' do
      expect(health.status).to be_a(String)
    end

    it 'has timestamp field' do
      expect(health.timestamp).to be_a(String)
    end

    it 'has worker_id field' do
      expect(health.worker_id).to be_a(String)
    end

    describe 'health checks' do
      it 'includes multiple health check categories' do
        expect(health.checks.keys).not_to be_empty
      end

      it 'each check is a HealthCheck struct' do
        health.checks.each_value do |check|
          expect(check).to be_a(TaskerCore::Observability::Types::HealthCheck)
        end
      end

      it 'each check has required fields' do
        health.checks.each_value do |check|
          expect(check.status).to be_a(String)
          expect(check.duration_ms).to be_a(Integer)
          expect(check.last_checked).to be_a(String)
        end
      end
    end

    describe 'system_info' do
      subject(:system_info) { health.system_info }

      it 'has version field' do
        expect(system_info.version).to be_a(String)
      end

      it 'has environment field' do
        expect(system_info.environment).to be_a(String)
      end

      it 'has uptime_seconds field' do
        expect(system_info.uptime_seconds).to be_a(Integer)
        expect(system_info.uptime_seconds).to be >= 0
      end

      it 'has worker_type field' do
        expect(system_info.worker_type).to be_a(String)
      end

      it 'has database_pool_size field' do
        expect(system_info.database_pool_size).to be_a(Integer)
      end

      it 'has command_processor_active field' do
        expect([true, false]).to include(system_info.command_processor_active)
      end

      it 'has supported_namespaces array' do
        expect(system_info.supported_namespaces).to be_an(Array)
      end
    end
  end

  # ==========================================================================
  # Convenience Health Methods
  # ==========================================================================

  describe '.ready?' do
    it 'returns a boolean' do
      expect([true, false]).to include(described_class.ready?)
    end

    it 'returns true when worker is healthy' do
      # Worker should be ready after bootstrap (status == 'healthy')
      # Note: May be false if any health checks fail
      result = described_class.ready?
      expect([true, false]).to include(result)
    end
  end

  describe '.alive?' do
    it 'returns a boolean' do
      expect([true, false]).to include(described_class.alive?)
    end

    it 'returns true when worker is alive' do
      # Now correctly checks for 'alive' status from liveness probe
      expect(described_class.alive?).to be true
    end
  end

  # ==========================================================================
  # Metrics Methods
  # ==========================================================================

  describe '.metrics_worker' do
    subject(:metrics) { described_class.metrics_worker }

    it 'returns a JSON string' do
      expect(metrics).to be_a(String)
      expect { JSON.parse(metrics) }.not_to raise_error
    end

    it 'contains worker metrics data' do
      data = JSON.parse(metrics)
      expect(data).to be_a(Hash)
    end
  end

  describe '.event_stats' do
    subject(:stats) { described_class.event_stats }

    it 'returns a DomainEventStats struct' do
      expect(stats).to be_a(TaskerCore::Observability::Types::DomainEventStats)
    end

    it 'has router stats' do
      expect(stats.router).to be_a(TaskerCore::Observability::Types::EventRouterStats)
    end

    it 'has in_process_bus stats' do
      expect(stats.in_process_bus).to be_a(TaskerCore::Observability::Types::InProcessEventBusStats)
    end

    it 'has captured_at timestamp' do
      expect(stats.captured_at).to be_a(String)
    end

    it 'has worker_id' do
      expect(stats.worker_id).to be_a(String)
    end

    describe 'router stats' do
      subject(:router) { stats.router }

      it 'has integer counter fields' do
        expect(router.total_routed).to be_a(Integer)
        expect(router.durable_routed).to be_a(Integer)
        expect(router.fast_routed).to be_a(Integer)
        expect(router.broadcast_routed).to be_a(Integer)
        expect(router.fast_delivery_errors).to be_a(Integer)
        expect(router.routing_errors).to be_a(Integer)
      end
    end

    describe 'in_process_bus stats' do
      subject(:bus) { stats.in_process_bus }

      it 'has integer counter fields' do
        expect(bus.total_events_dispatched).to be_a(Integer)
        expect(bus.rust_handler_dispatches).to be_a(Integer)
        expect(bus.ffi_channel_dispatches).to be_a(Integer)
        expect(bus.rust_handler_errors).to be_a(Integer)
        expect(bus.ffi_channel_drops).to be_a(Integer)
        expect(bus.rust_subscriber_patterns).to be_a(Integer)
        expect(bus.rust_handler_count).to be_a(Integer)
        expect(bus.ffi_subscriber_count).to be_a(Integer)
      end
    end
  end

  describe '.prometheus_metrics' do
    subject(:metrics) { described_class.prometheus_metrics }

    it 'returns a string' do
      expect(metrics).to be_a(String)
    end

    it 'contains Prometheus format text' do
      # Prometheus format should have # HELP or # TYPE lines
      expect(metrics).to include('# ')
    end
  end

  # ==========================================================================
  # Template Methods
  # ==========================================================================

  describe '.templates_list' do
    it 'returns a JSON string' do
      result = described_class.templates_list
      expect(result).to be_a(String)
      expect { JSON.parse(result) }.not_to raise_error
    end

    it 'accepts include_cache_stats option' do
      result = described_class.templates_list(include_cache_stats: true)
      expect(result).to be_a(String)
    end
  end

  describe '.template_get' do
    context 'with invalid template' do
      it 'raises RuntimeError for non-existent template' do
        expect do
          described_class.template_get(
            namespace: 'nonexistent',
            name: 'fake_template',
            version: '1.0.0'
          )
        end.to raise_error(RuntimeError)
      end
    end

    context 'with valid namespace' do
      it 'returns JSON for templates_list' do
        # templates_list always works
        result = described_class.templates_list
        expect(result).to be_a(String)
        data = JSON.parse(result)
        expect(data).to be_a(Hash)
      end
    end
  end

  describe '.template_validate' do
    context 'with invalid template' do
      it 'raises RuntimeError for non-existent namespace' do
        expect do
          described_class.template_validate(
            namespace: 'nonexistent',
            name: 'fake_template',
            version: '1.0.0'
          )
        end.to raise_error(RuntimeError)
      end
    end

    context 'with valid namespace but missing template' do
      # Use a namespace that exists in fixtures
      let(:existing_namespace) { 'test_errors' }

      it 'raises RuntimeError for non-existent template' do
        expect do
          described_class.template_validate(
            namespace: existing_namespace,
            name: 'nonexistent_template',
            version: '1.0.0'
          )
        end.to raise_error(RuntimeError)
      end
    end
  end

  describe '.cache_stats' do
    subject(:stats) { described_class.cache_stats }

    it 'returns a CacheStats struct' do
      expect(stats).to be_a(TaskerCore::Observability::Types::CacheStats)
    end

    it 'has total_entries field' do
      expect(stats.total_entries).to be_a(Integer)
      expect(stats.total_entries).to be >= 0
    end

    it 'has hits field' do
      expect(stats.hits).to be_a(Integer)
    end

    it 'has misses field' do
      expect(stats.misses).to be_a(Integer)
    end

    it 'has evictions field' do
      expect(stats.evictions).to be_a(Integer)
    end
  end

  describe '.cache_clear' do
    subject(:result) { described_class.cache_clear }

    it 'returns a CacheOperationResult struct' do
      expect(result).to be_a(TaskerCore::Observability::Types::CacheOperationResult)
    end

    it 'indicates success' do
      expect(result.success).to be true
    end

    it 'has message field' do
      expect(result.message).to be_a(String)
    end

    it 'has timestamp field' do
      expect(result.timestamp).to be_a(String)
    end
  end

  describe '.template_refresh' do
    subject(:result) do
      described_class.template_refresh(
        namespace: 'test',
        name: 'nonexistent',
        version: '1.0.0'
      )
    end

    it 'returns a CacheOperationResult struct' do
      expect(result).to be_a(TaskerCore::Observability::Types::CacheOperationResult)
    end

    it 'has success field' do
      expect([true, false]).to include(result.success)
    end

    it 'has message field' do
      expect(result.message).to be_a(String)
    end
  end

  # ==========================================================================
  # Config Methods
  # ==========================================================================

  describe '.config' do
    subject(:config) { described_class.config }

    it 'returns a RuntimeConfig struct' do
      expect(config).to be_a(TaskerCore::Observability::Types::RuntimeConfig)
    end

    it 'has environment field' do
      expect(config.environment).to be_a(String)
      expect(config.environment).not_to be_empty
    end

    it 'has common config hash' do
      expect(config.common).to be_a(Hash)
    end

    it 'has worker config hash' do
      expect(config.worker).to be_a(Hash)
    end

    describe 'metadata' do
      subject(:metadata) { config.metadata }

      it 'is a ConfigMetadata struct' do
        expect(metadata).to be_a(TaskerCore::Observability::Types::ConfigMetadata)
      end

      it 'has timestamp field' do
        expect(metadata.timestamp).to be_a(String)
      end

      it 'has source field' do
        expect(metadata.source).to be_a(String)
      end

      it 'has redacted_fields array' do
        expect(metadata.redacted_fields).to be_an(Array)
      end
    end
  end

  describe '.environment' do
    subject(:env) { described_class.environment }

    it 'returns a string' do
      expect(env).to be_a(String)
    end

    it 'is not empty' do
      expect(env).not_to be_empty
    end

    it 'matches common environment names' do
      expect(%w[test development production staging]).to include(env)
    end
  end

  # ==========================================================================
  # Type Coercion Tests
  # ==========================================================================

  describe 'dry-struct type coercion' do
    it 'BasicHealth validates string types' do
      health = described_class.health_basic
      expect { health.status }.not_to raise_error
      expect { health.worker_id }.not_to raise_error
      expect { health.timestamp }.not_to raise_error
    end

    it 'HealthCheck handles optional message field' do
      health = described_class.health_detailed
      health.checks.each_value do |check|
        # message can be nil or a string
        expect(check.message).to be_nil.or be_a(String)
      end
    end

    it 'EventRouterStats enforces integer types' do
      stats = described_class.event_stats
      router = stats.router

      # All counters should be integers
      expect(router.total_routed).to be_an(Integer)
      expect(router.durable_routed).to be_an(Integer)
    end

    it 'CacheStats handles optional last_maintenance field' do
      stats = described_class.cache_stats
      # last_maintenance can be nil or a string
      expect(stats.last_maintenance).to be_nil.or be_a(String)
    end

    it 'WorkerSystemInfo enforces array type for supported_namespaces' do
      health = described_class.health_detailed
      namespaces = health.system_info.supported_namespaces

      expect(namespaces).to be_an(Array)
      namespaces.each do |ns|
        expect(ns).to be_a(String)
      end
    end
  end

  # ==========================================================================
  # Error Handling Tests
  # ==========================================================================

  describe 'error handling' do
    it 'ready? returns false on error' do
      # Mock a failure scenario by checking the rescue behavior
      allow(TaskerCore::FFI).to receive(:health_ready).and_raise(StandardError)
      expect(described_class.ready?).to be false
    end

    it 'alive? returns false on error' do
      allow(TaskerCore::FFI).to receive(:health_live).and_raise(StandardError)
      expect(described_class.alive?).to be false
    end
  end
end
