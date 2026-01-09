# frozen_string_literal: true

require_relative 'resolvers/base_resolver'
require_relative 'resolvers/method_dispatch_wrapper'

module TaskerCore
  module Registry
    # TAS-93: Resolver Chain - Priority-Ordered Handler Resolution
    #
    # The ResolverChain orchestrates handler resolution by trying resolvers
    # in priority order until one successfully resolves the handler.
    #
    # == Resolution Contract
    #
    # 1. If HandlerDefinition has `resolver:` hint, use ONLY that resolver
    # 2. Otherwise, try resolvers in priority order (lower = first)
    # 3. Wrap resolved handler with MethodDispatchWrapper if needed
    # 4. Return nil if no resolver can handle the definition
    #
    # == Default Chain (when using .default)
    #
    # Priority 10:  ExplicitMappingResolver - registered handlers
    # Priority 100: ClassConstantResolver   - class path inference
    #
    # == Usage
    #
    #   # Create default chain
    #   chain = ResolverChain.default
    #
    #   # Or build custom chain
    #   chain = ResolverChain.new
    #     .with_resolver(MyResolver.new)
    #     .with_resolver(AnotherResolver.new)
    #
    #   # Resolve a handler
    #   handler = chain.resolve(definition, config)
    #
    # == Adding Custom Resolvers
    #
    #   chain.add_resolver(CustomResolver.new)
    #
    # == Named Resolver Access
    #
    #   # Add with name for resolver hint support
    #   chain.add_resolver(PaymentResolver.new)
    #
    #   # Definition with resolver hint
    #   definition.resolver = 'payment_resolver'
    #   # Chain will ONLY use PaymentResolver
    #
    class ResolverChain
      # Error raised when resolver hint points to unknown resolver
      class ResolverNotFoundError < StandardError; end

      # Error raised when resolution fails completely
      class ResolutionError < StandardError; end

      def initialize
        @resolvers = []
        @resolvers_by_name = {}
        @logger = TaskerCore::Logger.instance
      end

      # Create a chain with default resolvers
      #
      # @return [ResolverChain] Chain with Explicit + ClassConstant resolvers
      def self.default
        require_relative 'resolvers/explicit_mapping_resolver'
        require_relative 'resolvers/class_constant_resolver'

        new.tap do |chain|
          chain.add_resolver(Resolvers::ExplicitMappingResolver.new)
          chain.add_resolver(Resolvers::ClassConstantResolver.new)
        end
      end

      # Add a resolver to the chain
      #
      # @param resolver [Resolvers::BaseResolver] Resolver to add
      # @return [self] For method chaining
      def add_resolver(resolver)
        @resolvers << resolver
        @resolvers.sort_by!(&:priority)
        @resolvers_by_name[resolver.name] = resolver
        self
      end

      # Fluent API for adding resolvers
      alias with_resolver add_resolver

      # Remove a resolver by name
      #
      # @param name [String] Resolver name to remove
      # @return [Resolvers::BaseResolver, nil] Removed resolver
      def remove_resolver(name)
        resolver = @resolvers_by_name.delete(name)
        @resolvers.delete(resolver) if resolver
        resolver
      end

      # Get a resolver by name
      #
      # @param name [String] Resolver name
      # @return [Resolvers::BaseResolver, nil]
      def get_resolver(name)
        @resolvers_by_name[name]
      end

      # Get the explicit mapping resolver (convenience method)
      #
      # @return [Resolvers::ExplicitMappingResolver, nil]
      def explicit_resolver
        @resolvers_by_name['explicit_mapping']
      end

      # Resolve a handler from the definition
      #
      # @param definition [TaskerCore::Types::HandlerDefinition] Handler configuration
      # @param config [Hash] Additional context
      # @return [Object, nil] Resolved handler or nil
      def resolve(definition, config = {})
        # Contract: If resolver hint is present, use ONLY that resolver
        return resolve_with_hint(definition, config) if definition.has_resolver_hint?

        # Otherwise: Try inferential chain resolution
        resolve_with_chain(definition, config)
      end

      # Check if any resolver can handle this definition
      #
      # @param definition [TaskerCore::Types::HandlerDefinition] Handler configuration
      # @param config [Hash] Additional context
      # @return [Boolean]
      def can_resolve?(definition, config = {})
        if definition.has_resolver_hint?
          resolver = @resolvers_by_name[definition.resolver]
          return resolver&.can_resolve?(definition, config) || false
        end

        @resolvers.any? { |r| r.can_resolve?(definition, config) }
      end

      # Get all registered callables across all resolvers
      #
      # @return [Array<String>] List of all registered callable identifiers
      def registered_callables
        @resolvers.flat_map(&:registered_callables).uniq
      end

      # Register a handler directly (convenience method for ExplicitMappingResolver)
      #
      # @param key [String] Handler key
      # @param handler [Object] Handler class or instance
      # @return [self]
      def register(key, handler)
        explicit = explicit_resolver
        raise 'No ExplicitMappingResolver in chain' unless explicit

        explicit.register(key, handler)
        self
      end

      # Get chain info for debugging
      #
      # @return [Array<Hash>] Resolver info
      def chain_info
        @resolvers.map do |resolver|
          {
            name: resolver.name,
            priority: resolver.priority,
            callables: resolver.registered_callables.size
          }
        end
      end

      # @return [Integer] Number of resolvers in chain
      def size
        @resolvers.size
      end

      # @return [Array<String>] Names of resolvers in priority order
      def resolver_names
        @resolvers.map(&:name)
      end

      # Wrap handler for method dispatch if needed.
      #
      # This method is public because HandlerRegistry needs to wrap handlers
      # that it instantiates directly (outside the resolver chain flow).
      #
      # @param handler [Object] Resolved handler instance
      # @param definition [TaskerCore::Types::HandlerDefinition] Handler configuration
      # @return [Object, nil] Handler (possibly wrapped), or nil if method not found
      def wrap_for_method_dispatch(handler, definition)
        # No wrapping needed if using default .call method
        return handler unless definition.uses_method_dispatch?

        effective_method = definition.effective_method.to_sym

        # Verify handler responds to the target method
        unless handler.respond_to?(effective_method)
          @logger.warn("ResolverChain: Handler #{handler.class} doesn't respond to ##{effective_method}")
          return nil
        end

        @logger.debug("ResolverChain: Wrapping #{handler.class} for method dispatch → ##{effective_method}")
        Resolvers::MethodDispatchWrapper.new(handler, effective_method)
      end

      private

      # Resolve using explicit resolver hint (bypasses chain)
      #
      # @param definition [TaskerCore::Types::HandlerDefinition] Handler configuration
      # @param config [Hash] Additional context
      # @return [Object, nil] Resolved handler
      def resolve_with_hint(definition, config)
        resolver_name = definition.resolver
        resolver = @resolvers_by_name[resolver_name]

        unless resolver
          @logger.warn("ResolverChain: Unknown resolver hint '#{resolver_name}'")
          raise ResolverNotFoundError, "Unknown resolver: '#{resolver_name}'"
        end

        handler = resolver.resolve(definition, config)

        unless handler
          @logger.warn("ResolverChain: Resolver '#{resolver_name}' failed to resolve '#{definition.callable}'")
          return nil
        end

        @logger.debug("ResolverChain: Resolved '#{definition.callable}' via hint '#{resolver_name}'")
        wrap_for_method_dispatch(handler, definition)
      end

      # Resolve using chain (tries resolvers in priority order)
      #
      # @param definition [TaskerCore::Types::HandlerDefinition] Handler configuration
      # @param config [Hash] Additional context
      # @return [Object, nil] Resolved handler
      def resolve_with_chain(definition, config)
        @resolvers.each do |resolver|
          next unless resolver.can_resolve?(definition, config)

          handler = resolver.resolve(definition, config)
          next unless handler

          @logger.debug("ResolverChain: Resolved '#{definition.callable}' via '#{resolver.name}'")
          return wrap_for_method_dispatch(handler, definition)
        end

        @logger.debug("ResolverChain: No resolver could handle '#{definition.callable}'")
        nil
      end
    end
  end
end
