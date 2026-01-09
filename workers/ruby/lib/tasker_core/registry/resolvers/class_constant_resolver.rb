# frozen_string_literal: true

require_relative 'base_resolver'

module TaskerCore
  module Registry
    module Resolvers
      # TAS-93: Class Constant Resolver (Inferential)
      #
      # This resolver uses Ruby's `const_get` to resolve class paths
      # to actual handler classes. It's the fallback resolver with
      # lowest priority (100), tried after explicit mappings.
      #
      # == Resolution Behavior
      #
      # Given callable: "MyApp::Handlers::PaymentHandler"
      # 1. Attempts `Object.const_get('MyApp::Handlers::PaymentHandler')`
      # 2. Verifies the result is a Class
      # 3. Verifies the class responds to effective_method (from HandlerDefinition)
      # 4. Instantiates with config if supported
      #
      # == Usage
      #
      #   resolver = ClassConstantResolver.new
      #
      #   # This will find any loaded Ruby class
      #   handler = resolver.resolve(definition, config)
      #
      # == When This Resolver is Used
      #
      # - Standard handler class paths (e.g., "MyApp::PaymentHandler")
      # - Handler classes already loaded in memory
      # - Conventional Ruby handler patterns
      #
      class ClassConstantResolver < BaseResolver
        PRIORITY = 100 # Lowest priority - tried last

        def initialize
          super
          @logger = TaskerCore::Logger.instance
        end

        # @return [String] Human-readable resolver name
        def name
          'class_constant'
        end

        # @return [Integer] Resolution priority (100 = lowest)
        def priority
          PRIORITY
        end

        # Check if callable looks like a Ruby class constant path.
        #
        # This method checks the STRING FORMAT, not whether the class exists.
        # Actual class existence is verified in resolve() - this allows the
        # resolver chain to skip this resolver for callables that are clearly
        # not class paths (e.g., "payment_handler", "./path/to/handler").
        #
        # @param definition [TaskerCore::Types::HandlerDefinition] Handler configuration
        # @param config [Hash] Additional context
        # @return [Boolean] true if callable looks like a class constant
        def can_resolve?(definition, _config)
          callable = definition.callable.to_s

          # Basic validation: should look like a class constant
          # (starts with uppercase, may contain ::)
          callable.match?(/\A[A-Z]/)
        end

        # Resolve handler class from constant path
        #
        # @param definition [TaskerCore::Types::HandlerDefinition] Handler configuration
        # @param config [Hash] Additional context
        # @return [Object, nil] Handler instance or nil
        def resolve(definition, _config)
          callable = definition.callable.to_s
          effective_method = definition.effective_method.to_sym

          # Resolve the class constant
          klass = resolve_constant(callable)
          return nil unless klass

          # Verify it's a class
          unless klass.is_a?(Class)
            @logger.debug("ClassConstantResolver: '#{callable}' is not a Class (#{klass.class})")
            return nil
          end

          # Verify it responds to the effective method
          unless responds_to_method?(klass, effective_method)
            @logger.debug("ClassConstantResolver: '#{callable}' does not respond to ##{effective_method}")
            return nil
          end

          # Instantiate with configuration
          instantiate_handler(klass, definition.initialization)
        rescue StandardError => e
          @logger.debug("ClassConstantResolver: Failed to resolve '#{callable}': #{e.message}")
          nil
        end

        private

        # Resolve constant path to class
        #
        # @param callable [String] Class constant path
        # @return [Class, nil] Resolved class or nil
        def resolve_constant(callable)
          # Use constantize from ActiveSupport if available
          if callable.respond_to?(:constantize)
            callable.constantize
          else
            Object.const_get(callable)
          end
        rescue NameError => e
          @logger.debug("ClassConstantResolver: Constant '#{callable}' not found: #{e.message}")
          nil
        end

        # Check if class or its instances respond to the method
        #
        # @param klass [Class] Handler class
        # @param method_name [Symbol] Method to check
        # @return [Boolean] true if responds to method
        def responds_to_method?(klass, method_name)
          # Check instance methods (most common case)
          klass.instance_methods.include?(method_name) ||
            # Check if the class itself responds (for class methods)
            klass.respond_to?(method_name)
        end

        # Instantiate a handler with configuration
        #
        # @param klass [Class] Handler class
        # @param initialization [Hash] Configuration to pass
        # @return [Object] Handler instance
        def instantiate_handler(klass, initialization)
          arity = klass.instance_method(:initialize).arity

          if arity.positive? || (arity.negative? && accepts_config_kwarg?(klass))
            klass.new(config: initialization || {})
          else
            klass.new
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
