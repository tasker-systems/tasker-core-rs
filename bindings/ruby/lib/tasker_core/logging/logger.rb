# frozen_string_literal: true

require 'singleton'
require 'logger'
require 'json'

module TaskerCore
  module Logging
    class Logger
      include Singleton

      attr_reader :logger

      # Traditional string-based logging methods (backward compatibility)
      def info(message)
        @logger.info(message)
      end

      def warn(message)
        @logger.warn(message)
      end

      def error(message)
        @logger.error(message)
      end

      def fatal(message)
        @logger.fatal(message)
      end

      def debug(message)
        @logger.debug(message)
      end

      # Enhanced structured logging methods that match Rust patterns
      # These provide structured data while maintaining emoji + component format consistency

      def log_task(level, operation, **data)
        message = build_unified_message('📋 TASK_OPERATION', operation, **data)
        @logger.send(level, message)
      end

      def log_queue_worker(level, operation, namespace: nil, **data)
        message = if namespace
                    build_unified_message('🔄 QUEUE_WORKER', "#{operation} (namespace: #{namespace})", **data)
                  else
                    build_unified_message('🔄 QUEUE_WORKER', operation, **data)
                  end
        @logger.send(level, message)
      end

      def log_orchestrator(level, operation, **data)
        message = build_unified_message('🚀 ORCHESTRATOR', operation, **data)
        @logger.send(level, message)
      end

      def log_step(level, operation, **data)
        message = build_unified_message('🔧 STEP_OPERATION', operation, **data)
        @logger.send(level, message)
      end

      def log_database(level, operation, **data)
        message = build_unified_message('💾 DATABASE', operation, **data)
        @logger.send(level, message)
      end

      def log_ffi(level, operation, component: nil, **data)
        message = if component
                    build_unified_message('🌉 FFI', "#{operation} (#{component})", **data)
                  else
                    build_unified_message('🌉 FFI', operation, **data)
                  end
        @logger.send(level, message)
      end

      def log_config(level, operation, **data)
        message = build_unified_message('⚙️ CONFIG', operation, **data)
        @logger.send(level, message)
      end

      def log_registry(level, operation, namespace: nil, name: nil, **data)
        message = if namespace && name
                    build_unified_message('📚 REGISTRY', "#{operation} (#{namespace}/#{name})", **data)
                  else
                    build_unified_message('📚 REGISTRY', operation, **data)
                  end
        @logger.send(level, message)
      end

      def initialize
        @logger = if defined?(Rails)
                    Rails.logger
                  else
                    ::Logger.new($stdout).tap do |log|
                      log.level = ::Logger::INFO
                      log.formatter = method(:unified_formatter)
                    end
                  end
      end

      private

      # Build unified message format that matches Rust structured logging output
      def build_unified_message(component, operation, **data)
        base_message = "#{component}: #{operation}"
        
        # Add structured data if provided (for JSON logs or debugging)
        if data.any? && structured_logging_enabled?
          structured_data = data.merge(
            timestamp: Time.now.utc.iso8601,
            component: component.gsub(/[🎯📋🔄🚀🔧💾🌉⚙️📚❌⚠️✅]/, '').strip, # Remove emoji for structured field
            operation: operation
          )
          
          "#{base_message} | #{JSON.generate(structured_data)}"
        else
          base_message
        end
      end

      # Custom formatter that maintains readability while supporting structured data
      def unified_formatter(severity, datetime, progname, msg)
        # Check if message contains structured data (has JSON part)
        if msg.include?(' | {')
          base_msg, json_data = msg.split(' | ', 2)
          
          # In development, show readable format
          if development_environment?
            "[#{datetime}] #{severity} TaskerCore: #{base_msg}\n"
          else
            # In production, include structured data
            "[#{datetime}] #{severity} TaskerCore: #{base_msg} #{json_data}\n"
          end
        else
          # Traditional format for simple messages
          "[#{datetime}] #{severity} TaskerCore #{progname}: #{msg}\n"
        end
      end

      def structured_logging_enabled?
        # Enable structured logging in production or when explicitly requested
        ENV['STRUCTURED_LOGGING'] == 'true' || (!development_environment? && !test_environment?)
      end

      def development_environment?
        env = ENV['RAILS_ENV'] || ENV['TASKER_ENV'] || ENV['RACK_ENV'] || ENV['APP_ENV'] || 'development'
        env.downcase == 'development'
      end

      def test_environment?
        env = ENV['RAILS_ENV'] || ENV['TASKER_ENV'] || ENV['RACK_ENV'] || ENV['APP_ENV'] || 'development'
        env.downcase == 'test'
      end
    end
  end
end
