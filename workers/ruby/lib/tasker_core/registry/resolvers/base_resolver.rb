# frozen_string_literal: true

module TaskerCore
  module Registry
    module Resolvers
      # TAS-93: Abstract base class for step handler resolvers
      #
      # This class defines the contract that all resolvers must implement.
      # Resolvers are tried in priority order by the ResolverChain until
      # one successfully resolves the handler.
      #
      # == Resolution Contract
      #
      # 1. `name` - Human-readable identifier for logging/debugging
      # 2. `priority` - Lower numbers = tried first (10 = explicit, 100 = inferential)
      # 3. `can_resolve?` - Quick check if this resolver might handle the definition
      # 4. `resolve` - Actually resolve the handler callable
      #
      # == Implementation Example
      #
      #   class MyResolver < BaseResolver
      #     def name
      #       'my_custom_resolver'
      #     end
      #
      #     def priority
      #       50  # Between explicit (10) and class constant (100)
      #     end
      #
      #     def can_resolve?(definition, config)
      #       # Quick eligibility check
      #       definition.callable.start_with?('MyApp::')
      #     end
      #
      #     def resolve(definition, config)
      #       # Return handler or nil
      #       # handler must respond to effective_method
      #     end
      #   end
      #
      class BaseResolver
        # Human-readable name for this resolver (for logging/debugging)
        #
        # @return [String] The resolver name
        def name
          raise NotImplementedError, "#{self.class}#name must be implemented"
        end

        # Resolution priority (lower = tried first)
        #
        # Standard priorities:
        # - 10: Explicit mapping (registered handlers)
        # - 100: Class constant (inferential)
        #
        # @return [Integer] The priority value
        def priority
          raise NotImplementedError, "#{self.class}#priority must be implemented"
        end

        # Quick eligibility check (called before resolve)
        #
        # This should be a fast check to determine if this resolver
        # is even worth trying for the given definition.
        #
        # @param definition [TaskerCore::Types::HandlerDefinition] Handler configuration
        # @param config [Hash] Additional configuration context
        # @return [Boolean] true if this resolver might be able to resolve the handler
        def can_resolve?(definition, config)
          raise NotImplementedError, "#{self.class}#can_resolve? must be implemented"
        end

        # Resolve the handler callable from the definition
        #
        # @param definition [TaskerCore::Types::HandlerDefinition] Handler configuration
        # @param config [Hash] Additional configuration context
        # @return [Object, nil] Handler object or nil if cannot resolve
        def resolve(definition, config)
          raise NotImplementedError, "#{self.class}#resolve must be implemented"
        end

        # Return all callable keys this resolver knows about (for debugging/introspection)
        #
        # @return [Array<String>] List of registered callable identifiers
        def registered_callables
          []
        end
      end
    end
  end
end
