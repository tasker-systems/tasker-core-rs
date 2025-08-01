# frozen_string_literal: true

require 'pg'
require 'json'

module TaskerCore
  module Messaging
    # Ruby pgmq client using pg gem for PostgreSQL message queue operations
    # 
    # This provides a Ruby interface to pgmq (PostgreSQL message queue) that mirrors
    # the SQS-like API for workflow orchestration without FFI coupling.
    #
    # Examples:
    #   client = PgmqClient.new
    #   msg_id = client.send_message("fulfillment_queue", {"step_id" => 123, "data" => {...}})
    #   messages = client.read_messages("fulfillment_queue", visibility_timeout: 30, qty: 5)
    #   client.delete_message("fulfillment_queue", msg_id)
    class PgmqClient
      # Default configuration
      DEFAULT_VISIBILITY_TIMEOUT = 30
      DEFAULT_MESSAGE_COUNT = 1
      MAX_MESSAGE_COUNT = 100

      attr_reader :connection, :logger

      def initialize(connection: nil, logger: nil)
        @connection = connection || create_connection
        @logger = logger || TaskerCore::Logging::Logger.instance
      end

      # Create a new queue
      #
      # @param queue_name [String] Name of the queue to create
      # @return [Boolean] true if queue created successfully or already exists
      # @raise [TaskerCore::Error] if creation fails
      def create_queue(queue_name)
        logger.debug("📦 PGMQ: Creating queue: #{queue_name}")
        
        connection.exec("SELECT pgmq.create($1)", [queue_name])
        
        logger.info("✅ PGMQ: Queue created successfully: #{queue_name}")
        true
      rescue PG::Error => e
        logger.error("❌ PGMQ: Failed to create queue #{queue_name}: #{e.message}")
        raise TaskerCore::Error, "Failed to create queue #{queue_name}: #{e.message}"
      end

      # Drop/delete a queue and all its messages
      #
      # @param queue_name [String] Name of the queue to drop
      # @return [Boolean] true if queue dropped successfully
      def drop_queue(queue_name)
        logger.debug("🗑️ PGMQ: Dropping queue: #{queue_name}")
        
        connection.exec("SELECT pgmq.drop_queue($1)", [queue_name])
        
        logger.info("✅ PGMQ: Queue dropped successfully: #{queue_name}")
        true
      rescue PG::Error => e
        logger.error("❌ PGMQ: Failed to drop queue #{queue_name}: #{e.message}")
        raise TaskerCore::Error, "Failed to drop queue #{queue_name}: #{e.message}"
      end

      # Send a message to a queue
      #
      # @param queue_name [String] Name of the queue
      # @param message [Hash] Message content (will be converted to JSON)
      # @param delay_seconds [Integer] Optional delay before message becomes visible (default: 0)
      # @return [Integer] Message ID assigned by pgmq
      # @raise [TaskerCore::Error] if send fails
      def send_message(queue_name, message, delay_seconds: 0)
        logger.debug("📤 PGMQ: Sending message to queue: #{queue_name} (delay: #{delay_seconds}s)")
        
        message_json = message.is_a?(String) ? message : JSON.generate(message)
        
        result = connection.exec("SELECT pgmq.send($1, $2, $3) as msg_id", 
                                [queue_name, message_json, delay_seconds])
        
        msg_id = result[0]['msg_id'].to_i
        logger.debug("✅ PGMQ: Message sent successfully: #{queue_name} -> msg_id: #{msg_id}")
        
        msg_id
      rescue PG::Error => e
        logger.error("❌ PGMQ: Failed to send message to #{queue_name}: #{e.message}")
        raise TaskerCore::Error, "Failed to send message to #{queue_name}: #{e.message}"
      rescue JSON::GeneratorError => e
        logger.error("❌ PGMQ: Failed to serialize message for #{queue_name}: #{e.message}")
        raise TaskerCore::Error, "Failed to serialize message for #{queue_name}: #{e.message}"
      end

      # Send a step message to the appropriate namespace queue
      #
      # @param step_message [TaskerCore::Types::StepMessage] Step message to send
      # @param delay_seconds [Integer] Optional delay before message becomes visible
      # @return [Integer] Message ID assigned by pgmq
      def send_step_message(step_message, delay_seconds: 0)
        queue_name = step_message.queue_name
        message_hash = step_message.to_h
        
        logger.debug("📤 PGMQ: Sending step message - step_id: #{step_message.step_id}, task_id: #{step_message.task_id}, queue: #{queue_name}")
        
        send_message(queue_name, message_hash, delay_seconds: delay_seconds)
      end

      # Read messages from a queue
      #
      # @param queue_name [String] Name of the queue
      # @param visibility_timeout [Integer] Visibility timeout in seconds (default: 30)
      # @param qty [Integer] Number of messages to read (default: 1, max: 100)
      # @return [Array<Hash>] List of messages with metadata
      def read_messages(queue_name, visibility_timeout: DEFAULT_VISIBILITY_TIMEOUT, qty: DEFAULT_MESSAGE_COUNT)
        quantity = [qty, MAX_MESSAGE_COUNT].min
        
        logger.debug("📥 PGMQ: Reading #{quantity} messages from queue: #{queue_name} (vt: #{visibility_timeout}s)")
        
        result = connection.exec(
          "SELECT msg_id, read_ct, enqueued_at, vt, message FROM pgmq.read($1, $2, $3)",
          [queue_name, visibility_timeout, quantity]
        )
        
        messages = result.map do |row|
          {
            msg_id: row['msg_id'].to_i,
            queue_name: queue_name,
            message: JSON.parse(row['message']),
            vt: Time.parse(row['vt']),
            enqueued_at: Time.parse(row['enqueued_at']),
            read_ct: row['read_ct'].to_i
          }
        end
        
        logger.debug("✅ PGMQ: Read #{messages.length} messages from queue: #{queue_name}")
        messages
      rescue PG::Error => e
        logger.error("❌ PGMQ: Failed to read from queue #{queue_name}: #{e.message}")
        raise TaskerCore::Error, "Failed to read from queue #{queue_name}: #{e.message}"
      rescue JSON::ParserError => e
        logger.error("❌ PGMQ: Failed to parse message from queue #{queue_name}: #{e.message}")
        raise TaskerCore::Error, "Failed to parse message from queue #{queue_name}: #{e.message}"
      end

      # Read step messages from a namespace queue
      #
      # @param namespace [String] Namespace to read from (e.g., "fulfillment")
      # @param visibility_timeout [Integer] Visibility timeout in seconds
      # @param qty [Integer] Number of messages to read
      # @return [Array<Hash>] Messages with parsed step message data
      def read_step_messages(namespace, visibility_timeout: DEFAULT_VISIBILITY_TIMEOUT, qty: DEFAULT_MESSAGE_COUNT)
        queue_name = "#{namespace}_queue"
        messages = read_messages(queue_name, visibility_timeout: visibility_timeout, qty: qty)
        
        messages.map do |msg|
          begin
            step_message = TaskerCore::Types::StepMessage.new(msg[:message])
            {
              queue_message: msg,
              step_message: step_message
            }
          rescue => e
            logger.warn("⚠️ PGMQ: Failed to parse step message from queue #{queue_name}: #{e.message} - message: #{msg[:message]}")
            nil
          end
        end.compact
      end

      # Delete a message from the queue (acknowledge processing completion)
      #
      # @param queue_name [String] Name of the queue
      # @param msg_id [Integer] Message ID to delete
      # @return [Boolean] true if message was deleted, false if not found
      def delete_message(queue_name, msg_id)
        logger.debug("🗑️ PGMQ: Deleting message: #{msg_id} from queue: #{queue_name}")
        
        result = connection.exec("SELECT pgmq.delete($1, $2) as deleted", [queue_name, msg_id])
        deleted = result[0]['deleted'] == 't'
        
        if deleted
          logger.debug("✅ PGMQ: Message deleted successfully: #{msg_id} from #{queue_name}")
        else
          logger.warn("⚠️ PGMQ: Message not found for deletion: #{msg_id} from #{queue_name}")
        end
        
        deleted
      rescue PG::Error => e
        logger.error("❌ PGMQ: Failed to delete message #{msg_id} from #{queue_name}: #{e.message}")
        raise TaskerCore::Error, "Failed to delete message #{msg_id} from #{queue_name}: #{e.message}"
      end

      # Archive a message (move to archive table for retention)
      #
      # @param queue_name [String] Name of the queue
      # @param msg_id [Integer] Message ID to archive
      # @return [Boolean] true if message was archived, false if not found
      def archive_message(queue_name, msg_id)
        logger.debug("📦 PGMQ: Archiving message: #{msg_id} from queue: #{queue_name}")
        
        result = connection.exec("SELECT pgmq.archive($1, $2) as archived", [queue_name, msg_id])
        archived = result[0]['archived'] == 't'
        
        if archived
          logger.debug("✅ PGMQ: Message archived successfully: #{msg_id} from #{queue_name}")
        else
          logger.warn("⚠️ PGMQ: Message not found for archiving: #{msg_id} from #{queue_name}")
        end
        
        archived
      rescue PG::Error => e
        logger.error("❌ PGMQ: Failed to archive message #{msg_id} from #{queue_name}: #{e.message}")
        raise TaskerCore::Error, "Failed to archive message #{msg_id} from #{queue_name}: #{e.message}"
      end

      # Get queue statistics
      #
      # @param queue_name [String] Name of the queue
      # @return [Hash] Queue statistics
      def queue_stats(queue_name)
        logger.debug("📊 PGMQ: Getting stats for queue: #{queue_name}")
        
        result = connection.exec(
          "SELECT queue_length, newest_msg_age_sec, oldest_msg_age_sec, total_messages FROM pgmq.metrics($1)",
          [queue_name]
        )
        
        if result.ntuples > 0
          row = result[0]
          stats = {
            queue_name: queue_name,
            queue_length: row['queue_length'].to_i,
            oldest_msg_age_seconds: row['oldest_msg_age_sec']&.to_i,
            total_messages: row['total_messages'].to_i,
            collected_at: Time.now
          }
          
          logger.debug("✅ PGMQ: Queue stats - #{queue_name}: #{stats[:queue_length]} total")
          stats
        else
          logger.warn("⚠️ PGMQ: No stats found for queue: #{queue_name}")
          nil
        end
      rescue PG::Error => e
        logger.error("❌ PGMQ: Failed to get stats for queue #{queue_name}: #{e.message}")
        raise TaskerCore::Error, "Failed to get stats for queue #{queue_name}: #{e.message}"
      end

      # List all available queues
      #
      # @return [Array<String>] List of queue names
      def list_queues
        logger.debug("📋 PGMQ: Listing all queues")
        
        result = connection.exec("SELECT queue_name FROM pgmq.list_queues()")
        queue_names = result.map { |row| row['queue_name'] }
        
        logger.debug("✅ PGMQ: Found #{queue_names.length} queues: #{queue_names}")
        queue_names
      rescue PG::Error => e
        logger.error("❌ PGMQ: Failed to list queues: #{e.message}")
        raise TaskerCore::Error, "Failed to list queues: #{e.message}"
      end

      # Purge all messages from a queue
      #
      # @param queue_name [String] Name of the queue to purge
      # @return [Integer] Number of messages purged
      def purge_queue(queue_name)
        logger.debug("🧹 PGMQ: Purging all messages from queue: #{queue_name}")
        
        result = connection.exec("SELECT pgmq.purge_queue($1) as purged_count", [queue_name])
        purged_count = result[0]['purged_count'].to_i
        
        logger.info("✅ PGMQ: Purged #{purged_count} messages from queue: #{queue_name}")
        purged_count
      rescue PG::Error => e
        logger.error("❌ PGMQ: Failed to purge queue #{queue_name}: #{e.message}")
        raise TaskerCore::Error, "Failed to purge queue #{queue_name}: #{e.message}"
      end

      # Ensure namespace queues exist for workflow processing
      #
      # @param namespaces [Array<String>] List of namespaces to ensure queues for
      # @return [Boolean] true if all queues ensured successfully
      def ensure_namespace_queues(namespaces)
        logger.info("🚀 PGMQ: Ensuring namespace queues exist: #{namespaces}")
        
        namespaces.each do |namespace|
          queue_name = "#{namespace}_queue"
          create_queue(queue_name)
        end
        
        logger.info("✅ PGMQ: All namespace queues ensured")
        true
      end

      # Get comprehensive metrics for all queues
      #
      # @return [Hash] Hash of queue names to stats
      def all_queue_metrics
        queue_names = list_queues
        metrics = {}
        
        queue_names.each do |queue_name|
          begin
            stats = queue_stats(queue_name)
            metrics[queue_name] = stats if stats
          rescue TaskerCore::Error => e
            logger.warn("⚠️ PGMQ: Failed to get stats for queue #{queue_name}: #{e.message}")
          end
        end
        
        metrics
      end

      # Close the database connection
      def close
        connection&.close
      end

      private

      # Create database connection
      # Uses TaskerCore configuration or DATABASE_URL environment variable
      def create_connection
        # Try to use TaskerCore configuration if available
        if defined?(TaskerCore::Config)
          config = TaskerCore::Config.instance
          if config.respond_to?(:database_url) && config.database_url
            return PG.connect(config.database_url)
          end
        end
        
        # Fall back to DATABASE_URL environment variable
        database_url = ENV['DATABASE_URL']
        if database_url
          PG.connect(database_url)
        else
          raise TaskerCore::Error, "No database connection available. Set DATABASE_URL or configure TaskerCore."
        end
      rescue PG::Error => e
        raise TaskerCore::Error, "Failed to connect to database: #{e.message}"
      end
    end
  end
end