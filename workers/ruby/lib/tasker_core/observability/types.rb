# frozen_string_literal: true

require 'dry-struct'
require 'dry-types'

module TaskerCore
  module Observability
    # Type definitions for observability data structures
    #
    # These types use dry-struct for automatic validation and provide
    # structured access to health, metrics, and configuration data.
    module Types
      include Dry.Types()

      # ==========================================================================
      # Health Types
      # ==========================================================================

      # Basic health check response
      class BasicHealth < Dry::Struct
        attribute :status, Types::Strict::String
        attribute :worker_id, Types::Strict::String
        attribute :timestamp, Types::Strict::String
      end

      # Individual health check result
      class HealthCheck < Dry::Struct
        attribute :status, Types::Strict::String
        attribute :message, Types::Strict::String.optional
        attribute :duration_ms, Types::Strict::Integer
        attribute :last_checked, Types::Strict::String
      end

      # Worker system information
      class WorkerSystemInfo < Dry::Struct
        attribute :version, Types::Strict::String
        attribute :environment, Types::Strict::String
        attribute :uptime_seconds, Types::Strict::Integer
        attribute :worker_type, Types::Strict::String
        attribute :database_pool_size, Types::Strict::Integer
        attribute :command_processor_active, Types::Strict::Bool
        attribute :supported_namespaces, Types::Strict::Array.of(Types::Strict::String)
      end

      # Detailed health check response
      class DetailedHealth < Dry::Struct
        attribute :status, Types::Strict::String
        attribute :timestamp, Types::Strict::String
        attribute :worker_id, Types::Strict::String
        attribute :checks, Types::Strict::Hash.map(Types::Strict::String, HealthCheck)
        attribute :system_info, WorkerSystemInfo
      end

      # ==========================================================================
      # Metrics Types
      # ==========================================================================

      # Event router statistics
      class EventRouterStats < Dry::Struct
        attribute :total_routed, Types::Strict::Integer
        attribute :durable_routed, Types::Strict::Integer
        attribute :fast_routed, Types::Strict::Integer
        attribute :broadcast_routed, Types::Strict::Integer
        attribute :fast_delivery_errors, Types::Strict::Integer
        attribute :routing_errors, Types::Strict::Integer
      end

      # In-process event bus statistics
      class InProcessEventBusStats < Dry::Struct
        attribute :total_events_dispatched, Types::Strict::Integer
        attribute :rust_handler_dispatches, Types::Strict::Integer
        attribute :ffi_channel_dispatches, Types::Strict::Integer
        attribute :rust_handler_errors, Types::Strict::Integer
        attribute :ffi_channel_drops, Types::Strict::Integer
        attribute :rust_subscriber_patterns, Types::Strict::Integer
        attribute :rust_handler_count, Types::Strict::Integer
        attribute :ffi_subscriber_count, Types::Strict::Integer
      end

      # Domain event statistics
      class DomainEventStats < Dry::Struct
        attribute :router, EventRouterStats
        attribute :in_process_bus, InProcessEventBusStats
        attribute :captured_at, Types::Strict::String
        attribute :worker_id, Types::Strict::String
      end

      # ==========================================================================
      # Template Types
      # ==========================================================================

      # Template cache statistics
      class CacheStats < Dry::Struct
        attribute :total_entries, Types::Strict::Integer
        attribute :hits, Types::Strict::Integer
        attribute :misses, Types::Strict::Integer
        attribute :evictions, Types::Strict::Integer
        attribute :last_maintenance, Types::Strict::String.optional
      end

      # Cache operation result
      class CacheOperationResult < Dry::Struct
        attribute :success, Types::Strict::Bool
        attribute :message, Types::Strict::String
        attribute :timestamp, Types::Strict::String
      end

      # Template validation result
      class TemplateValidation < Dry::Struct
        attribute :valid, Types::Strict::Bool
        attribute :namespace, Types::Strict::String
        attribute :name, Types::Strict::String
        attribute :version, Types::Strict::String
        attribute :handler_count, Types::Strict::Integer
        attribute :issues, Types::Strict::Array.of(Types::Strict::String)
        attribute :handler_metadata, Types::Strict::Hash.optional
      end

      # ==========================================================================
      # Config Types
      # ==========================================================================

      # Configuration metadata
      class ConfigMetadata < Dry::Struct
        attribute :timestamp, Types::Strict::String
        attribute :source, Types::Strict::String
        attribute :redacted_fields, Types::Strict::Array.of(Types::Strict::String)
      end

      # Complete worker configuration response
      class RuntimeConfig < Dry::Struct
        attribute :environment, Types::Strict::String
        attribute :common, Types::Strict::Hash
        attribute :worker, Types::Strict::Hash
        attribute :metadata, ConfigMetadata
      end
    end
  end
end
