# frozen_string_literal: true

# Removed circular dependency on logger

module TaskerCore
  module Logging
    # CorrelationIdGenerator creates unique correlation IDs for distributed tracing
    #
    # Generates correlation IDs that are:
    # - Unique across distributed systems
    # - URL-safe and log-friendly
    # - Sortable by generation time
    # - Human-readable length
    #
    # Format: tsk_[timestamp_base36][random_suffix]
    # Example: tsk_l7k9m2p4_a8x3f9
    class CorrelationIdGenerator
      # Prefix for all Tasker correlation IDs
      PREFIX = 'tsk'

      # Length of random suffix
      RANDOM_SUFFIX_LENGTH = 6

      class << self
        # Generate a new correlation ID
        #
        # @return [String] A unique correlation ID
        #
        # @example
        #   id = Tasker::Logging::CorrelationIdGenerator.generate
        #   # => "tsk_l7k9m2p4_a8x3f9"
        def generate
          timestamp_part = generate_timestamp_component
          random_part = generate_random_component

          "#{PREFIX}_#{timestamp_part}_#{random_part}"
        end

        # Generate correlation ID from an existing ID (for HTTP propagation)
        #
        # @param existing_id [String, nil] Existing correlation ID from headers
        # @return [String] Valid correlation ID (existing or newly generated)
        #
        # @example
        #   # With existing valid ID
        #   id = from_existing("req_abc123")
        #   # => "req_abc123"
        #
        #   # With invalid/missing ID
        #   id = from_existing(nil)
        #   # => "tsk_l7k9m2p4_a8x3f9"
        def from_existing(existing_id)
          return generate if existing_id.nil? || existing_id.strip.empty?
          return existing_id if valid_correlation_id?(existing_id)

          # If existing ID is invalid, create a new one but log the attempt
          # Note: Removed logger dependency to avoid circular dependency
          generate
        end

        # Extract correlation ID from HTTP headers
        #
        # @param headers [Hash] HTTP headers hash
        # @param header_name [String] Header name to check (defaults to config)
        # @return [String, nil] Correlation ID if found and valid
        #
        # @example
        #   id = from_headers(request.headers)
        #   id = from_headers(env, 'X-Request-ID')
        def from_headers(headers, header_name = nil)
          header_name ||= Tasker::Configuration.configuration.telemetry.correlation_id_header

          # Handle different header formats (rack vs rails)
          header_value = headers[header_name] ||
                         headers["HTTP_#{header_name.upcase.tr('-', '_')}"] ||
                         headers[header_name.downcase]

          from_existing(header_value)
        end

        # Validate if a string is a valid correlation ID format
        #
        # @param id [String] The ID to validate
        # @return [Boolean] Whether the ID is valid
        #
        # @example
        #   valid_correlation_id?("tsk_l7k9m2p4_a8x3f9")  # => true
        #   valid_correlation_id?("invalid")              # => false
        #   valid_correlation_id?("")                     # => false
        def valid_correlation_id?(id)
          return false if id.nil? || id.strip.empty?
          return false if id.length > 100 # Reasonable max length

          # Allow various prefixes for external system compatibility
          # But ensure it's alphanumeric with underscores/hyphens only
          id.match?(/\A[a-zA-Z0-9][a-zA-Z0-9_\-]*\z/)
        end

        private

        # Generate timestamp component in base36 for compactness
        #
        # @return [String] Timestamp component
        def generate_timestamp_component
          # Use millisecond precision timestamp
          timestamp_ms = (Time.now.to_f * 1000).to_i
          timestamp_ms.to_s(36)
        end

        # Generate random component using secure random
        #
        # @return [String] Random component
        def generate_random_component
          # Use URL-safe base64 character set: A-Z, a-z, 0-9
          chars = ('A'..'Z').to_a + ('a'..'z').to_a + ('0'..'9').to_a

          Array.new(RANDOM_SUFFIX_LENGTH) { chars.sample }.join
        end
      end
    end
  end
end
