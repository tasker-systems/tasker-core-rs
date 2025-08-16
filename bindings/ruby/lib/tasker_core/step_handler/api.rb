# frozen_string_literal: true

require_relative 'base'
require 'faraday'

module TaskerCore
  module StepHandler
    # API step handler that provides HTTP functionality mirroring Rails engine's StepHandler::Api.
    # Integrates with Rust orchestration layer while preserving Rails patterns developers expect.
    #
    # Key Features (preserved from Rails engine):
    # - Faraday HTTP client with full configuration support
    # - Automatic error classification (RetryableError vs PermanentError)
    # - Retry-After header support for server-requested backoff
    # - SSL configuration, headers, query parameters
    # - Event publishing for monitoring and debugging
    # - process() and process_results() hooks for customization
    #
    # Enhanced Features:
    # - Type safety through dry-types integration
    # - Rust performance for orchestration decisions
    # - Enhanced error context for debugging
    class Api < Base
      # Override capabilities to include API-specific features (mirrors Rails engine)
      def capabilities
        super + %w[http_client error_classification retry_headers faraday_connection]
      end

      # Enhanced configuration schema for API handlers (mirrors Rails StepHandler::Api::Config)
      def config_schema
        super.merge({ properties: get_merged_api_config(super[:properties]) })
      end

      # ========================================================================
      # HTTP CONNECTION (mirrors Rails engine pattern)
      # ========================================================================

      # Access to the configured Faraday connection (mirrors Rails StepHandler::Api)
      # This is the primary interface developers use for HTTP operations
      # @return [Faraday::Connection] Configured HTTP connection
      def connection
        @connection ||= build_faraday_connection
      end

      # Allow connection customization with block (mirrors Rails pattern)
      # @yield [connection] Faraday connection for customization
      def configure_connection(&)
        @connection = build_faraday_connection(&)
      end

      # ========================================================================
      # STEP HANDLER INTERFACE (Rails engine compatible)
      # ========================================================================

      # Override process method to add API-specific error handling
      # Rails engine signature: process(task, sequence, step)
      # @param task [Tasker::Task] Task model instance with context data
      # @param sequence [Tasker::Types::StepSequence] Step sequence for navigation
      # @param step [Tasker::WorkflowStep] Current step being processed
      # @return [Faraday::Response] HTTP response object
      def process(_task, _sequence, _step)
        # Subclasses should override this method to make their specific API calls
        # This base implementation shows the pattern but needs to be customized

        url = config[:url] || config['url']

        # Make HTTP request using the configured connection
        response = connection.get(url)

        # Process response for error classification (automatic retry/permanent error handling)
        process_response(response)

        # Return response - Rails framework will store this in step.results
        response
      end

      # ========================================================================
      # CONVENIENCE HTTP METHODS (additional helpers)
      # ========================================================================

      # Perform HTTP GET request with automatic error classification
      # @param path [String] API endpoint path
      # @param params [Hash] Query parameters
      # @param headers [Hash] Additional headers
      # @return [Faraday::Response] Raw response object
      def get(path, params: {}, headers: {})
        response = connection.get(path, params, headers)
        process_response(response)
        response
      end

      # Perform HTTP POST request with automatic error classification
      # @param path [String] API endpoint path
      # @param data [Hash] Request body data
      # @param headers [Hash] Additional headers
      # @return [Faraday::Response] Raw response object
      def post(path, data: {}, headers: {})
        response = connection.post(path, data, headers)
        process_response(response)
        response
      end

      # Perform HTTP PUT request with automatic error classification
      # @param path [String] API endpoint path
      # @param data [Hash] Request body data
      # @param headers [Hash] Additional headers
      # @return [Faraday::Response] Raw response object
      def put(path, data: {}, headers: {})
        response = connection.put(path, data, headers)
        process_response(response)
        response
      end

      # Perform HTTP DELETE request with automatic error classification
      # @param path [String] API endpoint path
      # @param params [Hash] Query parameters
      # @param headers [Hash] Additional headers
      # @return [Faraday::Response] Raw response object
      def delete(path, params: {}, headers: {})
        response = connection.delete(path, params, headers)
        process_response(response)
        response
      end

      # ========================================================================
      # SPECIALIZED HTTP METHODS
      # ========================================================================

      # Upload file via multipart form data
      # @param path [String] Upload endpoint path
      # @param file_path [String] Path to file to upload
      # @param field_name [String] Form field name for file
      # @param additional_fields [Hash] Additional form fields
      # @return [Hash] Response data
      def api_upload_file(path, file_path, field_name: 'file', additional_fields: {})
        unless File.exist?(file_path)
          raise TaskerCore::PermanentError.new(
            "File not found: #{file_path}",
            error_code: 'FILE_NOT_FOUND',
            error_category: 'validation'
          )
        end

        payload = additional_fields.merge({
                                            field_name => Faraday::Multipart::FilePart.new(file_path,
                                                                                           File.open(file_path).content_type || 'application/octet-stream')
                                          })

        http_request(:post, path, multipart: payload)
      end

      # Perform paginated request that handles cursor/offset pagination
      # @param path [String] API endpoint path
      # @param method [Symbol] HTTP method (:get, :post)
      # @param pagination_key [String] Key for pagination parameter ('cursor', 'offset', 'page')
      # @param limit_key [String] Key for limit parameter ('limit', 'per_page', 'page_size')
      # @param max_pages [Integer] Maximum pages to fetch (safety limit)
      # @yield [page_data] Block to process each page of data
      # @return [Array] All collected results
      def api_paginated_request(path, method: :get, pagination_key: 'cursor',
                                limit_key: 'limit', max_pages: 100)
        results = []
        pagination_value = nil
        page_count = 0

        loop do
          page_count += 1
          if page_count > max_pages
            logger.warn("Pagination limit reached (#{max_pages} pages) for #{path}")
            break
          end

          params = { limit_key => (@config.dig(:pagination, :page_size) || 100) }
          params[pagination_key] = pagination_value if pagination_value

          response = http_request(method, path, params: params)
          page_data = response[:data] || response['data'] || []

          # Process page with block if provided
          if block_given?
            yield(page_data)
          else
            results.concat(Array(page_data))
          end

          # Check for next page
          pagination_value = extract_pagination_cursor(response, pagination_key)
          break unless pagination_value && page_data.any?
        end

        results
      end

      # ========================================================================
      # RESPONSE PROCESSING (mirrors Rails engine ResponseProcessor)
      # ========================================================================

      # Process HTTP response and classify errors (mirrors Rails engine logic)
      # This implements the same error classification as Rails StepHandler::Api
      # @param response [Faraday::Response] HTTP response to process
      def process_response(response)
        return response if response.success?

        # Mirror Rails engine error classification logic
        case response.status
        when 400, 401, 403, 404, 422
          # Client errors - permanent failures (don't retry)
          raise TaskerCore::PermanentError.new(
            "HTTP #{response.status}: #{response.reason_phrase}",
            error_code: "HTTP_#{response.status}",
            error_category: classify_client_error(response.status),
            context: {
              status: response.status,
              body: response.body,
              headers: response.headers.to_h
            }
          )
        when 429
          # Rate limiting - retryable with server-suggested backoff
          retry_after = extract_retry_after_header(response.headers)
          raise TaskerCore::RetryableError.new(
            "Rate limited: #{response.reason_phrase}",
            retry_after: retry_after,
            error_category: 'rate_limit',
            context: {
              status: response.status,
              retry_after: retry_after,
              body: response.body,
              headers: response.headers.to_h
            }
          )
        when 503
          # Service unavailable - retryable with server-suggested backoff
          retry_after = extract_retry_after_header(response.headers)
          raise TaskerCore::RetryableError.new(
            "Service unavailable: #{response.reason_phrase}",
            retry_after: retry_after,
            error_category: 'service_unavailable',
            context: {
              status: response.status,
              retry_after: retry_after,
              body: response.body,
              headers: response.headers.to_h
            }
          )
        when 500..599
          # Other server errors - retryable without forced backoff
          raise TaskerCore::RetryableError.new(
            "Server error: HTTP #{response.status} #{response.reason_phrase}",
            error_category: 'server_error',
            context: {
              status: response.status,
              body: response.body,
              headers: response.headers.to_h
            }
          )
        else
          # Unknown status codes - treat as retryable for safety
          raise TaskerCore::RetryableError.new(
            "Unknown HTTP status: #{response.status} #{response.reason_phrase}",
            error_category: 'unknown',
            context: {
              status: response.status,
              body: response.body,
              headers: response.headers.to_h
            }
          )
        end
      end

      # ========================================================================
      # PRIVATE IMPLEMENTATION (mirrors Rails engine patterns)
      # ========================================================================

      private

      def get_merged_api_config(config)
        ::TaskerCore::ConfigSchemas::API_CONFIG_SCHEMA.dup.merge(config)
      end

      # Build Faraday connection with configuration (mirrors Rails engine ConnectionBuilder)
      def build_faraday_connection
        base_url = config[:url] || config['url']

        Faraday.new(base_url) do |conn|
          # Apply configuration
          apply_connection_config(conn)

          # Apply custom configuration block if provided
          yield(conn) if block_given?

          # Default adapter (must be last)
          conn.adapter Faraday.default_adapter unless conn.builder.handlers.any? { |h| h.klass < Faraday::Adapter }
        end
      end

      # Apply configuration to Faraday connection
      def apply_connection_config(conn)
        # Get API timeouts from configuration
        api_timeouts = TaskerCore::Config.instance.api_timeouts

        # Timeouts - use config values or TaskerCore configuration defaults
        conn.options.timeout = config[:timeout] || config['timeout'] || api_timeouts[:timeout]
        conn.options.open_timeout = config[:open_timeout] || config['open_timeout'] || api_timeouts[:open_timeout]

        # SSL configuration
        if (ssl_config = config[:ssl] || config['ssl'])
          conn.ssl.merge!(ssl_config.transform_keys(&:to_sym))
        end

        # Headers
        if (headers = config[:headers] || config['headers'])
          headers.each { |key, value| conn.headers[key] = value }
        end

        # Query parameters
        if (params = config[:params] || config['params'])
          conn.params.merge!(params)
        end

        # Authentication
        apply_authentication(conn)

        # Request/response middleware
        conn.request :json
        conn.response :json

        # Logging (only in debug mode)
        return unless logger.level <= Logger::DEBUG

        conn.response :logger, logger, { headers: false, bodies: false }
      end

      # Apply authentication to connection (mirrors Rails engine patterns)
      def apply_authentication(conn)
        auth_config = config[:auth] || config['auth'] || {}

        case auth_config[:type] || auth_config['type']
        when 'bearer'
          token = auth_config[:token] || auth_config['token']
          conn.request :authorization, 'Bearer', token if token
        when 'basic'
          username = auth_config[:username] || auth_config['username']
          password = auth_config[:password] || auth_config['password']
          conn.request :authorization, :basic, username, password if username && password
        when 'api_key'
          token = auth_config[:token] || auth_config['token']
          header = auth_config[:api_key_header] || auth_config['api_key_header'] || 'X-API-Key'
          conn.headers[header] = token if token
        end
      end

      # Build Faraday HTTP client with configuration
      def build_http_client
        Faraday.new do |config|
          config.adapter Faraday.default_adapter
          config.request :json
          config.response :json

          # Get API timeouts from configuration
          api_timeouts = TaskerCore::Config.instance.api_timeouts

          # Timeouts - use config values or TaskerCore configuration defaults
          config.options.timeout = self.config[:timeout] || api_timeouts[:timeout]
          config.options.open_timeout = self.config[:open_timeout] || api_timeouts[:open_timeout]

          # Authentication middleware
          setup_authentication(config)

          # Logging middleware (only in development)
          config.response :logger, logger, { headers: false, bodies: false } if logger.level <= Logger::DEBUG
        end
      end

      # Setup authentication based on configuration
      def setup_authentication(config)
        auth_config = self.config[:auth] || {}

        case auth_config[:type]
        when 'bearer'
          config.request :authorization, 'Bearer', auth_config[:token]
        when 'basic'
          config.request :authorization, :basic, auth_config[:username], auth_config[:password]
        when 'api_key'
          # API key will be added in request headers
        when 'oauth2'
          # OAuth2 token will be added in request headers
        end
      end

      # Build request headers
      def build_request_headers(additional_headers)
        headers = (config[:headers] || {}).dup
        headers.merge!(additional_headers)

        # Add authentication headers
        auth_config = config[:auth] || {}
        case auth_config[:type]
        when 'api_key'
          key_header = auth_config[:api_key_header] || 'X-API-Key'
          headers[key_header] = auth_config[:token]
        when 'oauth2'
          headers['Authorization'] = "Bearer #{auth_config[:token]}"
        end

        headers
      end

      # Build full URL from base URL and path
      def build_full_url(path)
        base_url = config[:base_url]
        return path unless base_url

        File.join(base_url, path)
      end

      # Handle HTTP response with error classification
      def handle_response(response)
        if response.status >= 400
          # Classify error based on status code
          error = TaskerCore::ErrorClassification.from_http_response(
            response.status,
            "HTTP #{response.status}: #{response.reason_phrase}",
            response.body.to_json,
            response.headers
          )

          raise error
        end

        # Return successful response data
        {
          status: response.status,
          data: response.body,
          headers: response.headers,
          success: true
        }
      end

      # Extract retry-after header value
      def extract_retry_after_header(headers)
        TaskerCore::ErrorClassification.extract_retry_after(headers)
      end

      # Extract pagination cursor from response
      def extract_pagination_cursor(response, pagination_key)
        # Try different common pagination patterns
        data = response[:data] || response['data']

        # Check for cursor in response metadata
        meta = response[:meta] || response['meta'] || response[:pagination] || response['pagination']
        return meta[pagination_key] || meta[pagination_key.to_s] if meta

        # Check for cursor in response body
        return data[pagination_key] || data[pagination_key.to_s] if data.is_a?(Hash)

        # Check for next URL in headers (Link header)
        if (headers = response[:headers] || response['headers'])
          link_header = headers['Link'] || headers['link']
          if link_header&.include?('rel="next"')
            # Extract URL and parse pagination parameter
            # This is a simplified implementation
            return extract_param_from_link_header(link_header, pagination_key)
          end
        end

        nil
      end

      # Simple link header parsing for pagination
      def extract_param_from_link_header(link_header, param_key)
        # Very basic implementation - would need more robust parsing for production
        match = link_header.match(/[?&]#{param_key}=([^&>]+)/)
        match ? match[1] : nil
      end

      # Classify client error category by status code (mirrors Rails engine logic)
      def classify_client_error(status_code)
        case status_code
        when 400 then 'validation'
        when 401, 403 then 'authorization'
        when 404 then 'not_found'
        when 422 then 'validation'
        else 'client_error'
        end
      end

      # Request/response logging
      def log_request(method, url, params, data, headers)
        return unless logger.level <= Logger::DEBUG

        logger.debug("HTTP #{method.upcase} #{url}")
        logger.debug("Params: #{params}") if params.any?
        logger.debug("Data: #{data}") if data.any?
        logger.debug("Headers: #{headers.reject { |k, _v| k.downcase.include?('auth') }}")
      end

      def log_response(response)
        return unless logger.level <= Logger::DEBUG

        logger.debug("HTTP Response: #{response.status} #{response.reason_phrase}")
        logger.debug("Response Headers: #{response.headers}")
        logger.debug("Response Body: #{response.body}") if response.body
      end
    end
  end
end
