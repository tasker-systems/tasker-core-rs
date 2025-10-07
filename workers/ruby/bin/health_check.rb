#!/usr/bin/env ruby
# frozen_string_literal: true

# =============================================================================
# Ruby Worker Health Check
# =============================================================================
# Simple health check script for Docker HEALTHCHECK or Kubernetes probes

require 'net/http'
require 'json'
require 'timeout'

# Configuration
health_endpoint = ENV['HEALTH_CHECK_ENDPOINT'] || 'http://localhost:8081/health'
timeout_seconds = (ENV['HEALTH_CHECK_TIMEOUT'] || '5').to_i

begin
  uri = URI(health_endpoint)

  Timeout.timeout(timeout_seconds) do
    response = Net::HTTP.get_response(uri)

    if response.code == '200'
      body = begin
        JSON.parse(response.body)
      rescue StandardError
        {}
      end
      status = body['status'] || body['health'] || 'unknown'

      if %w[healthy ok ready].include?(status.to_s.downcase)
        puts "Health check passed: #{status}"
        exit 0
      else
        puts "Health check failed: status=#{status}"
        exit 1
      end
    else
      puts "Health check failed: HTTP #{response.code}"
      exit 1
    end
  end
rescue Timeout::Error
  puts "Health check timeout after #{timeout_seconds} seconds"
  exit 1
rescue StandardError => e
  puts "Health check error: #{e.message}"
  exit 1
end
