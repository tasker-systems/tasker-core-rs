# frozen_string_literal: true

require_relative 'messaging/pgmq_client'
require_relative 'messaging/queue_worker'

module TaskerCore
  # Messaging module for pgmq-based workflow orchestration
  # 
  # This module provides the foundation for autonomous queue-based workers
  # that replace the complex TCP command architecture with simple, reliable
  # PostgreSQL message queues.
  #
  # Components:
  # - PgmqClient: Ruby interface to PostgreSQL message queues
  # - QueueWorker: Autonomous workers that poll queues and execute steps
  module Messaging
    # Create a new pgmq client
    # @param connection [PG::Connection] Optional database connection
    # @param logger [Logger] Optional logger
    # @return [PgmqClient] New pgmq client instance
    def self.create_pgmq_client(connection: nil, logger: nil)
      PgmqClient.new(connection: connection, logger: logger)
    end

    # Create a new queue worker for a namespace
    # @param namespace [String] Namespace to process (e.g., "fulfillment")
    # @param options [Hash] Worker configuration options
    # @return [QueueWorker] New queue worker instance
    def self.create_queue_worker(namespace, **options)
      QueueWorker.new(namespace, **options)
    end

    # Ensure all namespace queues exist
    # @param namespaces [Array<String>] List of namespaces
    # @param pgmq_client [PgmqClient] Optional client (creates new if nil)
    # @return [Boolean] true if all queues ensured
    def self.ensure_namespace_queues(namespaces, pgmq_client: nil)
      client = pgmq_client || create_pgmq_client
      client.ensure_namespace_queues(namespaces)
    ensure
      client&.close if client && !pgmq_client
    end

    # Get comprehensive queue metrics
    # @param pgmq_client [PgmqClient] Optional client (creates new if nil)
    # @return [Hash] Hash of queue names to metrics
    def self.all_queue_metrics(pgmq_client: nil)
      client = pgmq_client || create_pgmq_client
      client.all_queue_metrics
    ensure
      client&.close if client && !pgmq_client
    end
  end
end