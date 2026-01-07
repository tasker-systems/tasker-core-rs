# frozen_string_literal: true

module TaskerCore
  module Registry
    module Resolvers
      # TAS-93: Method Dispatch Wrapper
      #
      # Wraps a handler to redirect the standard `.call()` interface to
      # a specified method name. This enables handlers with multiple
      # entry points (e.g., process, refund, validate) to be invoked
      # through the unified `.call()` interface.
      #
      # == Background
      #
      # The worker execution system always invokes `handler.call(context)`.
      # When a HandlerDefinition specifies `method: refund`, we need to
      # redirect that call to `handler.refund(context)`.
      #
      # == Usage
      #
      #   # Original handler
      #   class PaymentHandler
      #     def process(context); end
      #     def refund(context); end
      #   end
      #
      #   # Wrap for refund dispatch
      #   handler = PaymentHandler.new
      #   wrapped = MethodDispatchWrapper.new(handler, :refund)
      #
      #   # Now call() invokes refund()
      #   wrapped.call(context) # => handler.refund(context)
      #
      # == When Wrapping Occurs
      #
      # The ResolverChain automatically wraps handlers when:
      # - HandlerDefinition has `method` set to something other than 'call'
      # - The resolved handler doesn't already respond to `.call()`
      #
      class MethodDispatchWrapper
        attr_reader :handler, :target_method

        # Create a method dispatch wrapper
        #
        # @param handler [Object] The underlying handler instance
        # @param target_method [Symbol, String] Method to invoke on call()
        def initialize(handler, target_method)
          @handler = handler
          @target_method = target_method.to_sym
          validate!
        end

        # Invoke the target method on the wrapped handler
        #
        # @param args [Array] Arguments to pass through
        # @param kwargs [Hash] Keyword arguments to pass through
        # @param block [Proc] Block to pass through
        # @return [Object] Result from the target method
        def call(*, **kwargs, &)
          if kwargs.empty?
            @handler.public_send(@target_method, *, &)
          else
            @handler.public_send(@target_method, *, **kwargs, &)
          end
        end

        # Delegate respond_to? to the underlying handler
        #
        # @param method_name [Symbol] Method to check
        # @param include_private [Boolean] Include private methods
        # @return [Boolean]
        # rubocop:disable Style/OptionalBooleanParameter -- Must match Ruby's respond_to? signature
        def respond_to?(method_name, include_private = false)
          method_name.to_sym == :call || @handler.respond_to?(method_name, include_private)
        end
        # rubocop:enable Style/OptionalBooleanParameter

        # Delegate respond_to_missing? to the underlying handler
        def respond_to_missing?(method_name, include_private = false)
          @handler.respond_to_missing?(method_name, include_private)
        end

        # Forward method_missing to underlying handler for delegation
        #
        # @param method_name [Symbol] Method name
        # @param args [Array] Arguments
        # @param block [Proc] Block
        def method_missing(method_name, *, &)
          if @handler.respond_to?(method_name)
            @handler.public_send(method_name, *, &)
          else
            super
          end
        end

        # String representation for debugging
        #
        # @return [String]
        def to_s
          "#<MethodDispatchWrapper handler=#{@handler.class} method=#{@target_method}>"
        end

        # Inspect for debugging
        #
        # @return [String]
        def inspect
          to_s
        end

        # Access the underlying handler class
        #
        # @return [Class]
        def handler_class
          @handler.class
        end

        # Check if this is wrapping a particular class
        #
        # @param klass [Class] Class to check
        # @return [Boolean]
        def wraps?(klass)
          @handler.is_a?(klass)
        end

        # Unwrap to get the original handler
        #
        # @return [Object] The underlying handler
        def unwrap
          @handler
        end

        private

        # Validate the wrapper can dispatch to target method
        def validate!
          return if @handler.respond_to?(@target_method)

          raise ArgumentError,
                "Handler #{@handler.class} does not respond to ##{@target_method}"
        end
      end
    end
  end
end
