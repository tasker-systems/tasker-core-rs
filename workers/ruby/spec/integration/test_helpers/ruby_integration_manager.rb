# frozen_string_literal: true

require 'faraday'
require 'timeout'
require 'json'
require 'active_support/core_ext/hash/keys'

# Ruby Integration Test Manager for Docker-based service communication
# Mirrors the Rust IntegrationTestManager pattern but focused on Ruby worker testing
class RubyWorkerIntegrationManager
  class ServiceNotReadyError < StandardError; end
  class ServiceHealthCheckError < StandardError; end

  attr_reader :orchestration_url, :worker_url, :orchestration_client, :worker_client

  def initialize(orchestration_url:, worker_url:)
    @orchestration_url = orchestration_url
    @worker_url = worker_url
    @orchestration_client = OrchestrationClient.new(orchestration_url)
    @worker_client = WorkerClient.new(worker_url)
  end

  # Primary setup method - assumes Docker services are already running
  def self.setup
    orchestration_url = ENV['TASKER_TEST_ORCHESTRATION_URL'] || 'http://localhost:8080'
    worker_url = ENV['TASKER_TEST_RUBY_WORKER_URL'] || 'http://localhost:8082'

    manager = new(
      orchestration_url: orchestration_url,
      worker_url: worker_url
    )

    # Health check services unless explicitly skipped
    manager.verify_services_ready! unless ENV['TASKER_TEST_SKIP_HEALTH_CHECK'] == 'true'

    manager
  end

  # Setup for orchestration-only testing (no worker health checks)
  def self.setup_orchestration_only
    orchestration_url = ENV['TASKER_TEST_ORCHESTRATION_URL'] || 'http://localhost:8080'
    worker_url = ENV['TASKER_TEST_RUBY_WORKER_URL'] || 'http://localhost:8082'

    manager = new(
      orchestration_url: orchestration_url,
      worker_url: worker_url
    )

    # Only check orchestration service
    manager.verify_orchestration_ready! unless ENV['TASKER_TEST_SKIP_HEALTH_CHECK'] == 'true'

    manager
  end

  # Verify all required services are running and healthy
  def verify_services_ready!
    timeout = ENV['TASKER_TEST_HEALTH_TIMEOUT']&.to_i || 60 # Longer timeout for Ruby bootstrap
    retry_interval = ENV['TASKER_TEST_HEALTH_RETRY_INTERVAL']&.to_f || 3.0

    puts 'Verifying Docker services are ready...'
    puts "   Orchestration: #{@orchestration_url}"
    puts "   Ruby Worker: #{@worker_url} (Ruby bootstrap + Rust FFI)"

    Timeout.timeout(timeout) do
      loop do
        # Check orchestration service
        orchestration_health = @orchestration_client.health_check
        unless orchestration_health[:healthy]
          raise ServiceHealthCheckError,
                "Orchestration unhealthy: #{orchestration_health}"
        end

        # Check Ruby-enabled worker service (may take longer to bootstrap)
        worker_health = @worker_client.health_check
        raise ServiceHealthCheckError, "Worker unhealthy: #{worker_health}" unless worker_health[:healthy]

        puts 'All services ready!'
        break
      rescue Errno::ECONNREFUSED, Net::OpenTimeout, Faraday::Error => e
        puts "Services not ready yet: #{e.class} - #{e.message}"
        puts '   (Ruby worker may be bootstrapping Rust foundation via FFI...)'
        sleep retry_interval
      end
    end
  rescue Timeout::Error
    raise ServiceNotReadyError, "Services failed to become ready within #{timeout} seconds"
  end

  # Verify only orchestration service (for tests that don't need worker)
  def verify_orchestration_ready!
    timeout = ENV['TASKER_TEST_HEALTH_TIMEOUT']&.to_i || 30
    retry_interval = ENV['TASKER_TEST_HEALTH_RETRY_INTERVAL']&.to_f || 2.0

    puts 'Verifying orchestration service is ready...'
    puts "   Orchestration: #{@orchestration_url}"

    Timeout.timeout(timeout) do
      loop do
        # Check orchestration service only
        orchestration_health = @orchestration_client.health_check
        unless orchestration_health[:healthy]
          raise ServiceHealthCheckError,
                "Orchestration unhealthy: #{orchestration_health}"
        end

        puts 'Orchestration service ready!'
        break
      rescue Errno::ECONNREFUSED, Net::OpenTimeout, Faraday::Error => e
        puts "Orchestration not ready yet: #{e.class} - #{e.message}"
        sleep retry_interval
      end
    end
  rescue Timeout::Error
    raise ServiceNotReadyError, "Orchestration service failed to become ready within #{timeout} seconds"
  end

  # HTTP Client for Orchestration API
  class OrchestrationClient
    def initialize(base_url)
      @connection = Faraday.new(base_url) do |conn|
        conn.request :json
        conn.response :json
        conn.adapter Faraday.default_adapter
        conn.options.timeout = 30
        conn.headers['Content-Type'] = 'application/json'
      end
    end

    def health_check
      response = @connection.get('/health')
      parse_response(response, expect_healthy: true)
    end

    def create_task(task_request)
      response = @connection.post('/v1/tasks', task_request)
      parse_response(response)
    end

    # Get full task details including status and steps
    # Returns: { task_uuid, name, namespace, version, status, execution_status, steps, ... }
    def get_task(task_uuid)
      response = @connection.get("/v1/tasks/#{task_uuid}")
      parse_response(response)
    end

    # Alias for compatibility - get_task returns status in the response
    def get_task_status(task_uuid)
      task = get_task(task_uuid)
      {
        task_uuid: task[:task_uuid],
        status: task[:status],
        execution_status: task[:execution_status],
        total_steps: task[:total_steps],
        completed_steps: task[:completed_steps],
        failed_steps: task[:failed_steps],
        pending_steps: task[:pending_steps],
        in_progress_steps: task[:in_progress_steps],
        completion_percentage: task[:completion_percentage]
      }
    end

    # Alias for compatibility - get_task returns steps in the response
    def list_task_steps(task_uuid)
      task = get_task(task_uuid)
      task[:steps] || []
    end

    private

    def parse_response(response, expect_healthy: false)
      case response.status
      when 200, 201
        # Parse response and ensure symbolized keys
        data = if response.body.is_a?(Hash)
                 response.body.deep_symbolize_keys
               elsif response.body.is_a?(String)
                 JSON.parse(response.body, symbolize_names: true)
               else
                 response.body
               end

        if expect_healthy
          { healthy: %w[healthy ok].include?(data[:status].to_s), data: data }
        else
          data
        end
      when 404
        raise "Resource not found: #{response.body}"
      when 500
        raise "Server error: #{response.body}"
      else
        raise "HTTP error #{response.status}: #{response.body}"
      end
    rescue JSON::ParserError => e
      raise "Invalid JSON response: #{e.message}"
    end
  end

  # HTTP Client for Ruby Worker API
  class WorkerClient
    def initialize(base_url)
      @connection = Faraday.new(base_url) do |conn|
        conn.request :json
        conn.response :json
        conn.adapter Faraday.default_adapter
        conn.options.timeout = 30
        conn.headers['Content-Type'] = 'application/json'
      end
    end

    def health_check
      response = @connection.get('/health')
      parse_response(response, expect_healthy: true)
    end

    def worker_status
      response = @connection.get('/status')
      parse_response(response)
    end

    def worker_info
      # Use detailed status for comprehensive worker information
      response = @connection.get('/status/detailed')
      parse_response(response)
    end

    def list_handlers
      response = @connection.get('/handlers')
      parse_response(response)
    end

    def supported_namespaces
      response = @connection.get('/status/namespaces')
      parse_response(response)
    end

    private

    def parse_response(response, expect_healthy: false)
      case response.status
      when 200, 201
        # Parse response and ensure symbolized keys
        data = if response.body.is_a?(Hash)
                 response.body.deep_symbolize_keys
               elsif response.body.is_a?(String)
                 JSON.parse(response.body, symbolize_names: true)
               else
                 response.body
               end

        if expect_healthy
          { healthy: %w[healthy ok].include?(data[:status].to_s), data: data }
        else
          data
        end
      when 404
        raise "Resource not found: #{response.body}"
      when 500
        raise "Server error: #{response.body}"
      else
        raise "HTTP error #{response.status}: #{response.body}"
      end
    rescue JSON::ParserError => e
      raise "Invalid JSON response: #{e.message}"
    end
  end
end

# Helper methods for integration tests
module RubyIntegrationTestHelpers
  def wait_for_task_completion(manager, task_uuid, timeout_seconds = 5)
    start_time = Time.now

    loop do
      task_status = manager.orchestration_client.get_task_status(task_uuid)

      # Check for completion
      if task_status[:execution_status] == 'complete' || task_status[:status] == 'complete'
        return manager.orchestration_client.get_task(task_uuid)
      end

      # Check for error states
      if task_status[:execution_status] == 'error' || task_status[:status] == 'error'
        raise "Task #{task_uuid} failed with status: #{task_status}"
      end

      # Check timeout
      if Time.now - start_time > timeout_seconds
        raise "Task #{task_uuid} did not complete within #{timeout_seconds} seconds. Status: #{task_status}"
      end

      sleep 1
    end
  end

  def wait_for_task_failure(manager, task_uuid, timeout_seconds = 5)
    start_time = Time.now

    loop do
      task_status = manager.orchestration_client.get_task_status(task_uuid)

      # Check for failure states
      if task_status[:execution_status] == 'error' || task_status[:status] == 'error'
        return manager.orchestration_client.get_task(task_uuid)
      end

      # Check for unexpected completion (should fail, not complete)
      if task_status[:execution_status] == 'complete' || task_status[:status] == 'complete'
        raise "Task #{task_uuid} completed successfully but was expected to fail. Status: #{task_status}"
      end

      # Check timeout
      if Time.now - start_time > timeout_seconds
        raise "Task #{task_uuid} did not fail within #{timeout_seconds} seconds. Status: #{task_status}"
      end

      sleep 1
    end
  end

  def create_task_request(namespace, name, context = {})
    {
      namespace: namespace,
      name: name,
      version: '1.0.0',
      context: context.merge(random_uuid: SecureRandom.uuid),
      status: 'PENDING',
      initiator: 'ruby_integration_test',
      source_system: 'rspec_docker_integration',
      reason: "Test #{namespace}/#{name} workflow with Ruby handlers",
      complete: false,
      tags: [],
      bypass_steps: [],
      # NaiveDateTime expects ISO 8601 format: "2025-10-02T12:34:56" (T separator, no timezone)
      requested_at: Time.now.utc.strftime('%Y-%m-%dT%H:%M:%S'),
      options: nil,
      priority: 5
    }
  end
end
