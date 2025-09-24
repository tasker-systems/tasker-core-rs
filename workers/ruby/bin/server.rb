#!/usr/bin/env ruby
# frozen_string_literal: true

# =============================================================================
# Ruby Worker Server
# =============================================================================
# Production-ready server script that bootstraps Rust foundation via FFI
# and manages Ruby handler execution for workflow orchestration

require_relative '../lib/tasker_core'
require 'logger'

# Configure logger
logger = TaskerCore::Logger.instance
logger.logger.level = ENV['RUST_LOG'] == 'debug' ? Logger::DEBUG : Logger::INFO

# Startup banner
logger.info '=' * 60
logger.info 'Starting Ruby Worker Bootstrap System'
logger.info '=' * 60
logger.info "Environment: #{ENV['TASKER_ENV'] || 'development'}"
logger.info "Ruby Version: #{RUBY_VERSION}"
logger.info "Database URL: #{ENV['DATABASE_URL'] ? '[REDACTED]' : 'Not set'}"
logger.info "Template Path: #{ENV['TASK_TEMPLATE_PATH'] || 'Not set'}"

# Log configuration details
logger.info 'Configuration:'
logger.info '  Ruby will initialize Rust foundation via FFI'
logger.info '  Worker will process tasks by calling Ruby handlers'
if ENV['TASKER_ENV'] == 'production'
  logger.info '  Production optimizations: Enabled'
  logger.info "  GC Settings: HEAP_GROWTH_FACTOR=#{ENV.fetch('RUBY_GC_HEAP_GROWTH_FACTOR', nil)}"
end

# Initialize the Ruby worker system
begin
  logger.info 'Initializing Ruby Worker Bootstrap...'
  bootstrap = TaskerCore::Worker::Bootstrap.start!

  logger.info 'Ruby worker system started successfully'
  logger.info '  Rust foundation: Bootstrapped via FFI'
  logger.info '  Ruby handlers: Registered and ready'
  logger.info '  Worker status: Running and processing tasks'

  # Set up shutdown coordination (bootstrap.rb handles signal trapping)
  shutdown = false
  shutdown_mutex = Mutex.new
  shutdown_cv = ConditionVariable.new

  # Hook into bootstrap's shutdown process
  bootstrap.on_shutdown do
    logger.info 'Bootstrap initiated shutdown, signaling main loop...'
    shutdown_mutex.synchronize do
      shutdown = true
      shutdown_cv.signal
    end
  end

  # USR1 signal handler (Status report)
  Signal.trap('USR1') do
    logger.info 'Received USR1 signal, reporting worker status...'
    if bootstrap.respond_to?(:status)
      status = bootstrap.status
      logger.info "Worker Status: #{status.inspect}"
    else
      logger.info 'Worker Status: Running (detailed status not available)'
    end
  end

  # USR2 signal handler (Reload configuration - optional)
  Signal.trap('USR2') do
    logger.info 'Received USR2 signal, reloading configuration...'
    if bootstrap.respond_to?(:reload_config)
      begin
        bootstrap.reload_config
        logger.info 'Configuration reloaded successfully'
      rescue StandardError => e
        logger.error "Failed to reload configuration: #{e.message}"
      end
    else
      logger.info 'Configuration reload not supported in this version'
    end
  end

  logger.info 'Worker ready and processing tasks'
  logger.info '=' * 60

  # Main worker loop
  sleep_interval = ENV['TASKER_ENV'] == 'production' ? 5 : 1
  loop_count = 0

  shutdown_mutex.synchronize do
    until shutdown
      # Wait for shutdown signal with timeout for periodic health checks
      shutdown_cv.wait(shutdown_mutex, sleep_interval)

      # Periodic health check (every 60 iterations)
      loop_count += 1
      next unless (loop_count % 60).zero? && bootstrap.respond_to?(:health_check)

      begin
        health_status = bootstrap.health_check
        if health_status[:healthy]
          logger.debug "Health check ##{loop_count / 60}: OK" if logger.debug?
        else
          logger.warn "Health check ##{loop_count / 60}: UNHEALTHY - #{health_status[:error]}"
        end
      rescue TaskerCore::FFIError => e
        logger.error "FFI error during health check: #{e.message}"
      rescue StandardError => e
        logger.warn "Health check failed: #{e.message}"
      end
    end
  end

  # Shutdown sequence
  logger.info 'Starting shutdown sequence...'

  if bootstrap.respond_to?(:shutdown!)
    begin
      bootstrap.shutdown!
      logger.info 'Ruby worker shutdown completed successfully'
    rescue TaskerCore::FFIError => e
      logger.error "FFI error during shutdown: #{e.message}"
      logger.error e.backtrace.join("\n") if logger.debug?
      exit(3)
    rescue StandardError => e
      logger.error "Error during shutdown: #{e.message}"
      logger.error e.backtrace.join("\n") if logger.debug?
      exit(1)
    end
  else
    logger.info 'Worker shutdown method not available, exiting directly'
  end

  logger.info 'Ruby Worker Server terminated gracefully'
  exit(0)
rescue TaskerCore::ConfigurationError => e
  logger.fatal "Configuration error: #{e.message}"
  logger.fatal 'Please check your configuration and environment variables'
  exit(2)
rescue TaskerCore::FFIError => e
  logger.fatal "FFI initialization error: #{e.message}"
  logger.fatal 'Failed to bootstrap Rust foundation via FFI'
  exit(3)
rescue StandardError => e
  logger.fatal "Unexpected error during startup: #{e.class} - #{e.message}"
  logger.fatal e.backtrace.join("\n")
  exit(4)
rescue SignalException => e
  # Handle any uncaught signals
  logger.info "Received signal: #{e.signm}"
  exit(0)
end
