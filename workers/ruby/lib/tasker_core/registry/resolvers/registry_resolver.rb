# frozen_string_literal: true

require_relative 'base_resolver'

module TaskerCore
  module Registry
    module Resolvers
      # TAS-93: Developer-Friendly Base Class for Custom Resolvers
      #
      # RegistryResolver provides a DSL for creating custom handler resolvers
      # with pattern matching, prefix matching, and explicit handler registration.
      #
      # == Usage Examples
      #
      # === Pattern-Based Resolution
      #
      #   class PaymentResolver < RegistryResolver
      #     # Match handlers by pattern
      #     handles_pattern(/^Payments::.*Handler$/)
      #
      #     # Custom resolution logic
      #     def resolve_handler(definition, config)
      #       klass = definition.callable.constantize
      #       klass.new(config: definition.initialization)
      #     end
      #   end
      #
      # === Prefix-Based Resolution
      #
      #   class LegacyResolver < RegistryResolver
      #     handles_prefix 'Legacy::'
      #     priority_value 50
      #
      #     def resolve_handler(definition, config)
      #       # Custom instantiation for legacy handlers
      #       LegacyAdapter.wrap(definition.callable.constantize)
      #     end
      #   end
      #
      # === Explicit Handler Registration
      #
      #   class ExplicitResolver < RegistryResolver
      #     register_handler 'payment.process', PaymentHandler
      #     register_handler 'payment.refund', RefundHandler
      #   end
      #
      # == DSL Methods
      #
      # - `handles_pattern(regex)` - Match callable names by regex
      # - `handles_prefix(prefix)` - Match callable names by prefix
      # - `register_handler(key, handler)` - Explicit key → handler mapping
      # - `priority_value(n)` - Set resolver priority (default: 50)
      # - `resolver_name(name)` - Set human-readable name
      #
      class RegistryResolver < BaseResolver
        class << self
          # DSL: Set pattern to match callable names
          def handles_pattern(pattern)
            @pattern = pattern
          end

          # DSL: Set prefix to match callable names
          def handles_prefix(prefix)
            @prefix = prefix
          end

          # DSL: Register explicit handler mapping
          def register_handler(key, handler_or_class)
            @handlers ||= {}
            @handlers[key.to_s] = handler_or_class
          end

          # DSL: Set resolver priority
          def priority_value(value)
            @priority = value
          end

          # DSL: Set resolver name
          def resolver_name(value)
            @resolver_name = value
          end

          # Accessors for DSL values
          attr_reader :pattern, :prefix, :handlers

          def configured_priority
            @priority || 50 # Default: between explicit (10) and inferential (100)
          end

          def configured_name
            @resolver_name || name.demodulize.underscore
          end

          # Inherit DSL configuration in subclasses
          def inherited(subclass)
            super
            subclass.instance_variable_set(:@handlers, @handlers&.dup || {})
          end
        end

        def initialize
          super
          @handlers = self.class.handlers&.dup || {}
          @logger = TaskerCore::Logger.instance
        end

        # @return [String] Human-readable resolver name
        def name
          self.class.configured_name
        end

        # @return [Integer] Resolution priority
        def priority
          self.class.configured_priority
        end

        # Check if this resolver can handle the definition
        #
        # Resolution order:
        # 1. Check explicit handler registration
        # 2. Check pattern match
        # 3. Check prefix match
        #
        # @param definition [TaskerCore::Types::HandlerDefinition] Handler configuration
        # @param config [Hash] Additional context
        # @return [Boolean]
        def can_resolve?(definition, _config)
          callable = definition.callable.to_s

          # Check explicit registration
          return true if @handlers.key?(callable)

          # Check pattern
          return true if self.class.pattern && callable.match?(self.class.pattern)

          # Check prefix
          return true if self.class.prefix && callable.start_with?(self.class.prefix)

          false
        end

        # Resolve the handler from the definition
        #
        # @param definition [TaskerCore::Types::HandlerDefinition] Handler configuration
        # @param config [Hash] Additional context
        # @return [Object, nil] Handler instance or nil
        def resolve(definition, config)
          callable = definition.callable.to_s

          # Try explicit registration first
          if @handlers.key?(callable)
            handler = @handlers[callable]
            return instantiate_handler(handler, definition.initialization)
          end

          # Otherwise, delegate to subclass implementation
          resolve_handler(definition, config)
        rescue StandardError => e
          @logger.debug("RegistryResolver #{name}: Failed to resolve '#{callable}': #{e.message}")
          nil
        end

        # @return [Array<String>] List of explicitly registered callables
        def registered_callables
          @handlers.keys
        end

        # Register a handler at runtime
        #
        # @param key [String] Handler key/callable name
        # @param handler [Object] Handler class or instance
        def register(key, handler)
          @handlers[key.to_s] = handler
        end

        # Unregister a handler at runtime
        #
        # @param key [String] Handler key to remove
        # @return [Object, nil] Removed handler or nil
        def unregister(key)
          @handlers.delete(key.to_s)
        end

        protected

        # Override in subclass to provide custom resolution logic
        #
        # This is called when no explicit handler is registered.
        #
        # @param definition [TaskerCore::Types::HandlerDefinition] Handler configuration
        # @param config [Hash] Additional context
        # @return [Object, nil] Handler instance or nil
        def resolve_handler(_definition, _config)
          nil # Default: no custom resolution
        end

        # Instantiate a handler with configuration
        #
        # Handles:
        # - Classes with (config:) keyword arg
        # - Classes with no-arg constructor
        # - Already-instantiated objects
        #
        # @param handler [Class, Object] Handler class or instance
        # @param initialization [Hash] Configuration to pass
        # @return [Object] Handler instance
        def instantiate_handler(handler, initialization)
          return handler unless handler.is_a?(Class)

          arity = handler.instance_method(:initialize).arity

          if arity.positive? || (arity.negative? && accepts_config_kwarg?(handler))
            handler.new(config: initialization || {})
          else
            handler.new
          end
        end

        private

        # Check if the class accepts config: keyword argument
        def accepts_config_kwarg?(klass)
          params = klass.instance_method(:initialize).parameters
          params.any? { |type, name| %i[key keyreq].include?(type) && name == :config }
        end
      end
    end
  end
end
