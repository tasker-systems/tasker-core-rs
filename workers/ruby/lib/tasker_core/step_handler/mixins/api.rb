# frozen_string_literal: true

require 'faraday'

module TaskerCore
  module StepHandler
    module Mixins
      # API mixin that provides HTTP functionality for step handlers.
      # Use this mixin with Base to add HTTP capabilities to any handler.
      #
      # ## TAS-112: Composition Pattern
      #
      # This module follows the composition-over-inheritance pattern. Instead of
      # inheriting from a specialized API handler class, include this mixin in
      # your Base handler.
      #
      # ## Usage
      #
      # ```ruby
      # class MyApiHandler < TaskerCore::StepHandler::Base
      #   include TaskerCore::StepHandler::Mixins::API
      #
      #   def call(context)
      #     response = get('/users', params: { limit: 10 })
      #     success(result: response.body)
      #   end
      # end
      # ```
      #
      # ## Key Features
      #
      # - Faraday HTTP client with full configuration support
      # - Automatic error classification (RetryableError vs PermanentError)
      # - Retry-After header support for server-requested backoff
      # - SSL configuration, headers, query parameters
      module API
        # Hook called when module is included
        def self.included(base)
          base.extend(ClassMethods)
        end

        # Class methods added to including class
        module ClassMethods
          # No class methods needed for now
        end

        # Override capabilities to include API-specific features
        def capabilities
          super + %w[http_client error_classification retry_headers faraday_connection]
        end

        # Enhanced configuration schema for API handlers
        def config_schema
          super.merge({ properties: get_merged_api_config(super[:properties]) })
        end

        # ========================================================================
        # HTTP CONNECTION
        # ========================================================================

        # Access to the configured Faraday connection
        # @return [Faraday::Connection] Configured HTTP connection
        def connection
          @connection ||= build_faraday_connection
        end

        # Allow connection customization with block
        # @yield [connection] Faraday connection for customization
        def configure_connection(&)
          @connection = build_faraday_connection(&)
        end

        # ========================================================================
        # CONVENIENCE HTTP METHODS
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
        # @param pagination_key [String] Key for pagination parameter
        # @param limit_key [String] Key for limit parameter
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

            params = { limit_key => @config.dig(:pagination, :page_size) || 100 }
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
        # RESULT HELPERS
        # ========================================================================

        # Return a successful API response result
        #
        # @param data [Object] Response data
        # @param status [Integer] HTTP status code
        # @param headers [Hash] Response headers
        # @param execution_time_ms [Integer] Execution time in milliseconds
        # @param metadata [Hash] Additional metadata
        # @return [StepHandlerCallResult] Success result
        def api_success(data:, status: 200, headers: {}, execution_time_ms: nil, metadata: {})
          result = {
            'data' => data,
            'status' => status,
            'headers' => headers
          }
          result['execution_time_ms'] = execution_time_ms if execution_time_ms

          success(result: result, metadata: metadata.merge(api_call: true))
        end

        # Return a failed API response result
        #
        # @param message [String] Error message
        # @param status [Integer] HTTP status code
        # @param error_type [String] Error type classification
        # @param headers [Hash] Response headers
        # @param execution_time_ms [Integer] Execution time in milliseconds
        # @param metadata [Hash] Additional metadata
        # @return [StepHandlerCallResult] Error result
        def api_failure(message:, status: nil, error_type: nil, headers: {}, execution_time_ms: nil, metadata: {})
          # Classify error type based on status code if not provided
          classified_error_type = error_type || classify_error_type(status)
          retryable = retryable_status?(status)

          failure(
            message: message,
            error_type: classified_error_type,
            retryable: retryable,
            metadata: metadata.merge(
              api_call: true,
              status: status,
              headers: headers,
              execution_time_ms: execution_time_ms
            ).compact
          )
        end

        # ========================================================================
        # RESPONSE PROCESSING
        # ========================================================================

        # Process HTTP response and classify errors
        # @param response [Faraday::Response] HTTP response to process
        def process_response(response)
          return response if response.success?

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

        private

        # Classify error type based on HTTP status code
        def classify_error_type(status)
          case status
          when 400..499 then 'PermanentError'
          when 500..599 then 'RetryableError'
          else 'UnexpectedError'
          end
        end

        # Determine if status code indicates retryable error
        def retryable_status?(status)
          return false if status.nil?

          status >= 500 || status == 429
        end

        def get_merged_api_config(config)
          ::TaskerCore::ConfigSchemas::API_CONFIG_SCHEMA.dup.merge(config)
        end

        # Build Faraday connection with configuration
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

        # Apply authentication to connection
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

        # Extract retry-after header value
        def extract_retry_after_header(headers)
          TaskerCore::ErrorClassification.extract_retry_after(headers)
        end

        # Extract pagination cursor from response
        def extract_pagination_cursor(response, pagination_key)
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
              return extract_param_from_link_header(link_header, pagination_key)
            end
          end

          nil
        end

        # Simple link header parsing for pagination
        def extract_param_from_link_header(link_header, param_key)
          match = link_header.match(/[?&]#{param_key}=([^&>]+)/)
          match ? match[1] : nil
        end

        # Classify client error category by status code
        def classify_client_error(status_code)
          case status_code
          when 400, 422 then 'validation'
          when 401, 403 then 'authorization'
          when 404 then 'not_found'
          else 'client_error'
          end
        end
      end
    end
  end
end
