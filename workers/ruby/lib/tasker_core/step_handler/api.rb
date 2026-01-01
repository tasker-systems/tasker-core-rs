# frozen_string_literal: true

require_relative 'base'
require_relative 'mixins'

module TaskerCore
  module StepHandler
    # API step handler that provides HTTP functionality.
    #
    # ## TAS-112: Composition Pattern (DEPRECATED CLASS)
    #
    # This class is provided for backward compatibility. For new code, use the mixin pattern:
    #
    # ```ruby
    # class MyApiHandler < TaskerCore::StepHandler::Base
    #   include TaskerCore::StepHandler::Mixins::API
    #
    #   def call(context)
    #     response = get('/users')
    #     success(result: response.body)
    #   end
    # end
    # ```
    #
    # ## Key Features
    # - Faraday HTTP client with full configuration support
    # - Automatic error classification (RetryableError vs PermanentError)
    # - Retry-After header support for server-requested backoff
    # - SSL configuration, headers, query parameters
    class Api < Base
      include Mixins::API

      # Legacy process method for Rails engine compatibility
      # New handlers should implement call(context) instead
      #
      # @param _task [Object] Task (unused, for Rails compatibility)
      # @param _sequence [Object] Sequence (unused, for Rails compatibility)
      # @param _step [Object] Step (unused, for Rails compatibility)
      # @return [Faraday::Response] HTTP response object
      def process(_task, _sequence, _step)
        url = config[:url] || config['url']
        response = connection.get(url)
        process_response(response)
        response
      end
    end
  end
end
