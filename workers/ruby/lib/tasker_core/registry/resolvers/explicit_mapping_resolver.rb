# frozen_string_literal: true

require_relative 'base_resolver'

module TaskerCore
  module Registry
    module Resolvers
      # TAS-93: Explicit Key-to-Handler Mapping Resolver
      #
      # This resolver handles explicitly registered handlers with direct
      # key → handler mappings. It has the highest priority (10) because
      # explicit registrations should always take precedence.
      #
      # == Usage
      #
      #   resolver = ExplicitMappingResolver.new
      #   resolver.register('payment.process', PaymentHandler)
      #   resolver.register('payment.refund', RefundHandler.new)
      #
      #   # Later, during resolution
      #   handler = resolver.resolve(definition, config)
      #
      # == When to Use
      #
      # - Handlers registered at application boot
      # - Proc/Lambda handlers that can't be resolved by class path
      # - Overriding inferential resolution with specific handlers
      # - Testing with mock handlers
      #
      class ExplicitMappingResolver < BaseResolver
        PRIORITY = 10 # Highest priority - explicit mappings always win

        def initialize(resolver_name = 'explicit_mapping')
          super()
          @resolver_name = resolver_name
          @handlers = Concurrent::Hash.new
          @logger = TaskerCore::Logger.instance
        end

        # @return [String] Human-readable resolver name
        def name
          @resolver_name
        end

        # @return [Integer] Resolution priority (10 = highest)
        def priority
          PRIORITY
        end

        # Check if we have an explicit mapping for this callable
        #
        # @param definition [TaskerCore::Types::HandlerDefinition] Handler configuration
        # @param config [Hash] Additional context
        # @return [Boolean] true if callable is registered
        def can_resolve?(definition, _config)
          @handlers.key?(definition.callable.to_s)
        end

        # Resolve handler from explicit registration
        #
        # @param definition [TaskerCore::Types::HandlerDefinition] Handler configuration
        # @param config [Hash] Additional context
        # @return [Object, nil] Handler instance or nil
        def resolve(definition, _config)
          callable = definition.callable.to_s
          handler = @handlers[callable]

          return nil unless handler

          instantiate_handler(handler, definition.initialization)
        rescue StandardError => e
          @logger.warn("ExplicitMappingResolver: Failed to instantiate '#{callable}': #{e.message}")
          nil
        end

        # @return [Array<String>] List of registered callable keys
        def registered_callables
          @handlers.keys
        end

        # Register a handler with an explicit key
        #
        # @param key [String] Handler key (usually the callable string)
        # @param handler [Class, Object, Proc] Handler class, instance, or callable
        # @return [void]
        def register(key, handler)
          @handlers[key.to_s] = handler
          @logger.debug("ExplicitMappingResolver: Registered '#{key}'")
        end

        # Unregister a handler
        #
        # @param key [String] Handler key to remove
        # @return [Object, nil] Removed handler or nil
        def unregister(key)
          @handlers.delete(key.to_s)
        end

        # Check if a handler is registered
        #
        # @param key [String] Handler key
        # @return [Boolean] true if registered
        def registered?(key)
          @handlers.key?(key.to_s)
        end

        # Clear all registered handlers
        def clear!
          @handlers.clear
        end

        # @return [Integer] Number of registered handlers
        def size
          @handlers.size
        end

        private

        # Instantiate a handler with configuration
        #
        # @param handler [Class, Object, Proc] Handler class, instance, or callable
        # @param initialization [Hash] Configuration to pass
        # @return [Object] Handler instance or callable
        def instantiate_handler(handler, initialization)
          # Already an instance or Proc - return as-is
          return handler if handler.is_a?(Proc) || !handler.is_a?(Class)

          # Class - instantiate with config if supported
          arity = handler.instance_method(:initialize).arity

          if arity.positive? || (arity.negative? && accepts_config_kwarg?(handler))
            handler.new(config: initialization || {})
          else
            handler.new
          end
        end

        # Check if the class accepts config: keyword argument
        def accepts_config_kwarg?(klass)
          params = klass.instance_method(:initialize).parameters
          params.any? { |type, name| %i[key keyreq].include?(type) && name == :config }
        end
      end
    end
  end
end
