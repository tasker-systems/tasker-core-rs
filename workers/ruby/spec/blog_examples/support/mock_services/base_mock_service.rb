# frozen_string_literal: true

require 'securerandom'

# Base Mock Service
# Provides a foundation for creating mock external services for blog example testing
class BaseMockService
  # Class-level state management
  class << self
    # Reset all mock state
    def reset!
      @call_log = []
      @responses = {}
      @failures = {}
      @failure_counts = {}
      @delays = {}
    end

    # Configure a mock response for a method
    # @param method [Symbol] The method name to mock
    # @param response [Hash] The response to return
    def stub_response(method, response)
      @responses ||= {}
      @responses[method] = response
    end

    # Configure a failure for a method
    # @param method [Symbol] The method name to fail
    # @param error_class [Class] The exception class to raise (default: StandardError)
    # @param message [String] Optional error message
    # @param fail_count [Integer] Number of times to fail before succeeding (nil = always fail)
    def stub_failure(method, error_class = StandardError, message = nil, fail_count: nil)
      @failures ||= {}
      @failure_counts ||= {}
      @failures[method] = { class: error_class, message: message }
      @failure_counts[method] = { remaining: fail_count, original: fail_count } if fail_count
    end

    # Configure a delay for a method (simulates slow network calls)
    # @param method [Symbol] The method name to delay
    # @param delay [Float] Delay in seconds
    def stub_delay(method, delay)
      @delays ||= {}
      @delays[method] = delay
    end

    # Get the call log for inspection
    # @return [Array<Hash>] List of method calls with timestamps and arguments
    def call_log
      @call_log ||= []
    end

    # Get calls for a specific method
    # @param method [Symbol] The method name to filter by
    # @return [Array<Hash>] Filtered call log
    def calls_for(method)
      call_log.select { |call| call[:method] == method }
    end

    # Check if a method was called
    # @param method [Symbol] The method name to check
    # @return [Boolean] True if method was called
    def called?(method)
      calls_for(method).any?
    end

    # Get the number of times a method was called
    # @param method [Symbol] The method name to count
    # @return [Integer] Number of calls
    def call_count(method)
      calls_for(method).count
    end

    # Get the most recent call for a method
    # @param method [Symbol] The method name
    # @return [Hash, nil] Most recent call or nil if never called
    def last_call(method)
      calls_for(method).last
    end

    # Clear the call log
    def clear_log!
      @call_log = []
    end

    # Reset all mock services that inherit from BaseMockService
    def reset_all_mocks!
      # Find all classes that inherit from BaseMockService
      ObjectSpace.each_object(Class).select { |klass| klass < BaseMockService }.each(&:reset!)
    end

    # Get call log from all mock services
    def get_call_log
      all_calls = []
      ObjectSpace.each_object(Class).select { |klass| klass < BaseMockService }.each do |service_class|
        all_calls.concat(service_class.call_log)
      end
      all_calls.sort_by { |call| call[:timestamp] }
    end
  end

  private

  # Log a method call
  # @param method [Symbol] The method name
  # @param args [Hash] The method arguments
  def log_call(method, args = {})
    self.class.call_log << {
      method: method,
      args: args,
      timestamp: Time.now,
      service: self.class.name
    }
  end

  # Handle the response for a method, including failures and delays
  # @param method [Symbol] The method name
  # @param default_response [Hash] Default response if no stub configured
  # @return [Hash] The response
  def handle_response(method, default_response = {})
    # Apply delay if configured
    if self.class.instance_variable_get(:@delays)&.key?(method)
      delay = self.class.instance_variable_get(:@delays)[method]
      sleep(delay)
    end

    # Check for configured failure
    failures = self.class.instance_variable_get(:@failures)
    failure_counts = self.class.instance_variable_get(:@failure_counts)

    if failures&.key?(method)
      # Check if we should fail based on failure count
      if failure_counts&.key?(method)
        count_info = failure_counts[method]
        if count_info[:remaining] && count_info[:remaining] > 0
          # Decrement the failure count and fail
          count_info[:remaining] -= 1
          failure_config = failures[method]
          error_message = failure_config[:message] || "Mock failure for #{method}"
          raise failure_config[:class], error_message
        end
        # If remaining count is 0 or nil, don't fail (success)
      else
        # No count limit, always fail
        failure_config = failures[method]
        error_message = failure_config[:message] || "Mock failure for #{method}"
        raise failure_config[:class], error_message
      end
    end

    # Return configured response or default
    responses = self.class.instance_variable_get(:@responses)
    responses&.fetch(method, default_response) || default_response
  end

  # Generate a unique ID for mock responses
  # @param prefix [String] Prefix for the ID
  # @return [String] Unique ID
  def generate_id(prefix = 'mock')
    "#{prefix}_#{SecureRandom.hex(8)}"
  end

  # Generate a timestamp for mock responses
  # @return [String] ISO8601 timestamp
  def generate_timestamp
    Time.now.utc.iso8601
  end
end
