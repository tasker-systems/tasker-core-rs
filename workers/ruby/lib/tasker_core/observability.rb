# frozen_string_literal: true

require 'json'
require_relative 'observability/types'

module TaskerCore
  # Observability - Access worker health, metrics, and configuration via FFI
  #
  # TAS-77: This module provides Ruby-friendly access to worker observability data
  # without requiring the HTTP web API. Data is fetched directly from Rust via FFI.
  #
  # @example Check worker health
  #   health = TaskerCore::Observability.health_basic
  #   puts health.status  # => "healthy"
  #   puts health.worker_id
  #
  # @example Get detailed health for K8s probes
  #   if TaskerCore::Observability.ready?
  #     puts "Worker ready to receive requests"
  #   end
  #
  # @example Get metrics
  #   events = TaskerCore::Observability.event_stats
  #   puts "Events routed: #{events.router.total_routed}"
  #
  # @example Query configuration
  #   config = TaskerCore::Observability.config
  #   puts "Environment: #{config.environment}"
  #
  # @example List templates
  #   templates_json = TaskerCore::Observability.templates_list
  #   templates = JSON.parse(templates_json)
  #
  module Observability
    class << self
      # ========================================================================
      # Health Methods
      # ========================================================================

      # Get basic health status
      #
      # @return [Types::BasicHealth] Basic health response
      def health_basic
        json = TaskerCore::FFI.health_basic
        data = JSON.parse(json)
        Types::BasicHealth.new(
          status: data['status'],
          worker_id: data['worker_id'],
          timestamp: data['timestamp']
        )
      end

      # Get liveness status (Kubernetes liveness probe)
      #
      # @return [Types::BasicHealth] Liveness response
      def health_live
        json = TaskerCore::FFI.health_live
        data = JSON.parse(json)
        Types::BasicHealth.new(
          status: data['status'],
          worker_id: data['worker_id'],
          timestamp: data['timestamp']
        )
      end

      # Get readiness status (Kubernetes readiness probe)
      #
      # @return [Types::DetailedHealth] Detailed health with checks
      def health_ready
        json = TaskerCore::FFI.health_ready
        parse_detailed_health(json)
      end

      # Get detailed health information
      #
      # @return [Types::DetailedHealth] Comprehensive health data
      def health_detailed
        json = TaskerCore::FFI.health_detailed
        parse_detailed_health(json)
      end

      # Check if worker is ready to receive requests
      #
      # @return [Boolean] true if worker is ready
      def ready?
        health_ready.status == 'healthy'
      rescue StandardError
        false
      end

      # Check if worker is alive
      #
      # @return [Boolean] true if worker is alive
      def alive?
        # Liveness probe returns 'alive' status, not 'healthy'
        health_live.status == 'alive'
      rescue StandardError
        false
      end

      # ========================================================================
      # Metrics Methods
      # ========================================================================

      # Get worker metrics as JSON string
      #
      # @return [String] JSON-formatted metrics
      def metrics_worker
        TaskerCore::FFI.metrics_worker
      end

      # Get domain event statistics
      #
      # @return [Types::DomainEventStats] Event routing statistics
      def event_stats
        json = TaskerCore::FFI.metrics_events
        data = JSON.parse(json)

        Types::DomainEventStats.new(
          router: Types::EventRouterStats.new(
            total_routed: data['router']['total_routed'],
            durable_routed: data['router']['durable_routed'],
            fast_routed: data['router']['fast_routed'],
            broadcast_routed: data['router']['broadcast_routed'],
            fast_delivery_errors: data['router']['fast_delivery_errors'],
            routing_errors: data['router']['routing_errors']
          ),
          in_process_bus: Types::InProcessEventBusStats.new(
            total_events_dispatched: data['in_process_bus']['total_events_dispatched'],
            rust_handler_dispatches: data['in_process_bus']['rust_handler_dispatches'],
            ffi_channel_dispatches: data['in_process_bus']['ffi_channel_dispatches'],
            rust_handler_errors: data['in_process_bus']['rust_handler_errors'],
            ffi_channel_drops: data['in_process_bus']['ffi_channel_drops'],
            rust_subscriber_patterns: data['in_process_bus']['rust_subscriber_patterns'],
            rust_handler_count: data['in_process_bus']['rust_handler_count'],
            ffi_subscriber_count: data['in_process_bus']['ffi_subscriber_count']
          ),
          captured_at: data['captured_at'],
          worker_id: data['worker_id']
        )
      end

      # Get Prometheus-formatted metrics
      #
      # @return [String] Prometheus text format metrics
      def prometheus_metrics
        TaskerCore::FFI.metrics_prometheus
      end

      # ========================================================================
      # Template Methods
      # ========================================================================

      # List all templates as JSON
      #
      # @param include_cache_stats [Boolean] Include cache statistics
      # @return [String] JSON-formatted template list
      def templates_list(include_cache_stats: false)
        TaskerCore::FFI.templates_list(include_cache_stats)
      end

      # Get a specific template as JSON
      #
      # @param namespace [String] Template namespace
      # @param name [String] Template name
      # @param version [String] Template version
      # @return [String] JSON-formatted template
      def template_get(namespace:, name:, version:)
        TaskerCore::FFI.template_get(namespace, name, version)
      end

      # Validate a template for worker execution
      #
      # @param namespace [String] Template namespace
      # @param name [String] Template name
      # @param version [String] Template version
      # @return [Types::TemplateValidation] Validation result
      def template_validate(namespace:, name:, version:)
        json = TaskerCore::FFI.template_validate(namespace, name, version)
        data = JSON.parse(json)

        Types::TemplateValidation.new(
          valid: data['valid'],
          namespace: data['namespace'],
          name: data['name'],
          version: data['version'],
          handler_count: data['handler_count'],
          issues: data['issues'] || [],
          handler_metadata: data['handler_metadata']
        )
      end

      # Get template cache statistics
      #
      # @return [Types::CacheStats] Cache statistics
      def cache_stats
        json = TaskerCore::FFI.templates_cache_stats
        data = JSON.parse(json)

        # Map Rust field names to Ruby struct names
        Types::CacheStats.new(
          total_entries: data['total_cached'] || 0,
          hits: data['cache_hits'] || 0,
          misses: data['cache_misses'] || 0,
          evictions: data['cache_evictions'] || 0,
          last_maintenance: nil # Not provided by Rust API
        )
      end

      # Clear the template cache
      #
      # @return [Types::CacheOperationResult] Operation result
      def cache_clear
        json = TaskerCore::FFI.templates_cache_clear
        data = JSON.parse(json)

        # Rust API returns { operation, success, cache_stats }
        Types::CacheOperationResult.new(
          success: data['success'] || false,
          message: data['operation'] || 'clear',
          timestamp: Time.now.utc.iso8601
        )
      end

      # Refresh a specific template in the cache
      #
      # @param namespace [String] Template namespace
      # @param name [String] Template name
      # @param version [String] Template version
      # @return [Types::CacheOperationResult] Operation result
      def template_refresh(namespace:, name:, version:)
        json = TaskerCore::FFI.template_refresh(namespace, name, version)
        data = JSON.parse(json)

        Types::CacheOperationResult.new(
          success: data['success'] || false,
          message: data['message'] || data['operation'] || 'refresh',
          timestamp: data['timestamp'] || Time.now.utc.iso8601
        )
      rescue RuntimeError => e
        # Return a structured error response instead of raising
        Types::CacheOperationResult.new(
          success: false,
          message: e.message,
          timestamp: Time.now.utc.iso8601
        )
      end

      # ========================================================================
      # Config Methods
      # ========================================================================

      # Get runtime configuration with secrets redacted
      #
      # @return [Types::RuntimeConfig] Configuration data
      def config
        json = TaskerCore::FFI.config_runtime
        data = JSON.parse(json)

        Types::RuntimeConfig.new(
          environment: data['environment'],
          common: data['common'],
          worker: data['worker'],
          metadata: Types::ConfigMetadata.new(
            timestamp: data['metadata']['timestamp'],
            source: data['metadata']['source'],
            redacted_fields: data['metadata']['redacted_fields']
          )
        )
      end

      # Get the current environment name
      #
      # @return [String] Environment name
      def environment
        TaskerCore::FFI.config_environment
      end

      private

      # Parse detailed health JSON into typed struct
      def parse_detailed_health(json)
        data = JSON.parse(json)

        checks = data['checks'].transform_values do |check|
          Types::HealthCheck.new(
            status: check['status'],
            message: check['message'],
            duration_ms: check['duration_ms'],
            last_checked: check['last_checked']
          )
        end

        system_info = Types::WorkerSystemInfo.new(
          version: data['system_info']['version'],
          environment: data['system_info']['environment'],
          uptime_seconds: data['system_info']['uptime_seconds'],
          worker_type: data['system_info']['worker_type'],
          database_pool_size: data['system_info']['database_pool_size'],
          command_processor_active: data['system_info']['command_processor_active'],
          supported_namespaces: data['system_info']['supported_namespaces']
        )

        Types::DetailedHealth.new(
          status: data['status'],
          timestamp: data['timestamp'],
          worker_id: data['worker_id'],
          checks: checks,
          system_info: system_info
        )
      end
    end
  end
end
