# frozen_string_literal: true

require 'singleton'

module TaskerCore
  module Registry
    # Handler registry with TAS-93 ResolverChain support
    #
    # Manages the discovery, registration, and instantiation of step handlers
    # throughout the worker lifecycle. Uses a ResolverChain for flexible
    # handler resolution with support for:
    # - Explicit registration (highest priority)
    # - Class constant resolution (inferential)
    # - Method dispatch (handler_method redirects .call())
    # - Resolver hints (bypass chain with specific resolver)
    #
    # == Resolution Priority
    #
    # 1. **Explicit Registration** (priority 10): Handlers registered via `register_handler`
    # 2. **Class Constant** (priority 100): Ruby class lookup via `Object.const_get`
    #
    # == Method Dispatch (TAS-93)
    #
    # When resolving with a HandlerDefinition that specifies `handler_method`,
    # the returned handler is wrapped to redirect `.call()` to the specified method:
    #
    #   # Template specifies: handler_method: "refund"
    #   handler = registry.resolve_handler(handler_definition)
    #   handler.call(context)  # Actually calls handler.refund(context)
    #
    # == Usage Examples
    #
    # @example Resolving by class name (string)
    #   handler = registry.resolve_handler("PaymentHandler")
    #
    # @example Resolving with HandlerDefinition (full TAS-93 support)
    #   definition = TaskerCore::Types::HandlerDefinition.new(
    #     callable: "PaymentHandler",
    #     handler_method: "refund"
    #   )
    #   handler = registry.resolve_handler(definition)
    #   handler.call(context)  # Calls PaymentHandler#refund
    #
    # @example Resolving from FFI HandlerWrapper
    #   handler = registry.resolve_handler(step_data.step_definition.handler)
    #
    # @see TaskerCore::Registry::ResolverChain For resolution chain details
    # @see TaskerCore::Registry::Resolvers::MethodDispatchWrapper For method dispatch
    class HandlerRegistry
      include Singleton

      attr_reader :logger, :handlers, :resolver_chain

      def initialize
        @logger = TaskerCore::Logger.instance
        @handlers = {} # Legacy compatibility - also populated for direct access
        @resolver_chain = ResolverChain.default
        bootstrap_handlers!
      end

      # Resolve handler by class name, HandlerDefinition, or HandlerWrapper
      #
      # TAS-93: Uses ResolverChain for flexible resolution with method dispatch support.
      #
      # @param handler_spec [String, Types::HandlerDefinition, Models::HandlerWrapper]
      #   Handler specification - can be:
      #   - String: Class name to resolve (e.g., "PaymentHandler")
      #   - HandlerDefinition: Full definition with method/resolver hints
      #   - HandlerWrapper: FFI wrapper from step_definition.handler
      # @return [Object, nil] Handler instance ready for .call(), or nil if not found
      def resolve_handler(handler_spec)
        # Convert to HandlerDefinition for unified resolution
        definition = normalize_to_definition(handler_spec)

        # Use resolver chain for resolution
        handler = @resolver_chain.resolve(definition)
        return handler if handler

        # Fallback: try legacy @handlers hash for backward compatibility
        handler_class = @handlers[definition.callable]
        return nil unless handler_class

        instantiate_handler(handler_class, definition)
      rescue StandardError => e
        logger.error("💥 Failed to resolve handler '#{extract_callable(handler_spec)}': #{e.message}")
        nil
      end

      # Register handler class
      #
      # @param class_name [String] Handler identifier (typically class name)
      # @param handler_class [Class] Handler class to register
      def register_handler(class_name, handler_class)
        # Register in both resolver chain and legacy hash
        @resolver_chain.register(class_name, handler_class)
        @handlers[class_name] = handler_class
        logger.debug("✅ Registered handler: #{class_name}")
      end

      # Check if handler is available
      #
      # @param class_name [String] Handler class name
      # @return [Boolean] true if handler can be resolved
      def handler_available?(class_name)
        definition = TaskerCore::Types::HandlerDefinition.new(callable: class_name)
        @resolver_chain.can_resolve?(definition) || @handlers.key?(class_name)
      end

      # Get all registered handler names
      #
      # @return [Array<String>] Sorted list of registered handler names
      def registered_handlers
        (@resolver_chain.registered_callables + @handlers.keys).uniq.sort
      end

      # ========================================================================
      # CROSS-LANGUAGE STANDARD ALIASES (TAS-96)
      # ========================================================================

      # Cross-language standard: register(name, handler_class)
      # @see #register_handler
      alias register register_handler

      # Cross-language standard: is_registered(name)
      # @see #handler_available?
      alias is_registered handler_available?

      # Cross-language standard: list_handlers
      # @see #registered_handlers
      alias list_handlers registered_handlers

      # Cross-language standard: resolve(name)
      # @see #resolve_handler
      alias resolve resolve_handler

      # Get template discovery information for debugging
      def template_discovery_info
        template_path = TaskerCore::TemplateDiscovery::TemplatePath.find_template_config_directory

        {
          template_path: template_path,
          template_files: template_path ? TaskerCore::TemplateDiscovery::TemplatePath.discover_template_files(template_path) : [],
          discovered_handlers: template_path ? TaskerCore::TemplateDiscovery::HandlerDiscovery.discover_all_handlers(template_path) : [],
          handlers_by_namespace: template_path ? TaskerCore::TemplateDiscovery::HandlerDiscovery.discover_handlers_by_namespace(template_path) : {},
          environment: ENV['TASKER_ENV'] || ENV['RAILS_ENV'] || 'development'
        }
      end

      # TAS-93: Add custom resolver to the chain
      #
      # @param resolver [Resolvers::BaseResolver] Resolver to add
      def add_resolver(resolver)
        @resolver_chain.add_resolver(resolver)
        logger.info("✅ Added resolver '#{resolver.name}' (priority #{resolver.priority})")
      end

      private

      # TAS-93: Normalize handler spec to HandlerDefinition for unified resolution
      #
      # @param handler_spec [String, Types::HandlerDefinition, Models::HandlerWrapper]
      # @return [Types::HandlerDefinition]
      def normalize_to_definition(handler_spec)
        case handler_spec
        when String
          # Simple string callable - create minimal definition
          TaskerCore::Types::HandlerDefinition.new(callable: handler_spec)
        when TaskerCore::Types::HandlerDefinition
          # Already a HandlerDefinition - use as-is
          handler_spec
        when TaskerCore::Models::HandlerWrapper
          # FFI HandlerWrapper - convert to HandlerDefinition
          # TAS-93: HandlerWrapper now includes handler_method and resolver from Rust FFI
          TaskerCore::Types::HandlerDefinition.new(
            callable: handler_spec.callable,
            initialization: handler_spec.initialization.to_h,
            handler_method: handler_spec.handler_method,
            resolver: handler_spec.resolver
          )
        else
          # Try to extract callable from object (duck typing)
          unless handler_spec.respond_to?(:callable)
            raise ArgumentError, "Cannot normalize #{handler_spec.class} to HandlerDefinition"
          end

          attrs = { callable: handler_spec.callable.to_s }
          attrs[:initialization] = handler_spec.initialization.to_h if handler_spec.respond_to?(:initialization)
          attrs[:handler_method] = handler_spec.handler_method if handler_spec.respond_to?(:handler_method)
          attrs[:resolver] = handler_spec.resolver if handler_spec.respond_to?(:resolver)
          TaskerCore::Types::HandlerDefinition.new(**attrs)

        end
      end

      # Extract callable string from handler spec for error messages
      #
      # @param handler_spec [Object] Handler specification
      # @return [String] Callable name
      def extract_callable(handler_spec)
        case handler_spec
        when String then handler_spec
        when TaskerCore::Types::HandlerDefinition then handler_spec.callable
        else
          handler_spec.respond_to?(:callable) ? handler_spec.callable.to_s : handler_spec.to_s
        end
      end

      # Instantiate handler with method dispatch support
      #
      # @param handler_class [Class] Handler class
      # @param definition [Types::HandlerDefinition] Handler definition
      # @return [Object] Handler instance (possibly wrapped for method dispatch)
      def instantiate_handler(handler_class, definition)
        # Check constructor arity for config support
        arity = handler_class.instance_method(:initialize).arity
        handler = if arity.positive? || (arity.negative? && accepts_config_kwarg?(handler_class))
                    handler_class.new(config: definition.initialization || {})
                  else
                    handler_class.new
                  end

        # Wrap for method dispatch if needed
        @resolver_chain.wrap_for_method_dispatch(handler, definition)
      end

      # Check if class accepts config: keyword argument
      #
      # @param klass [Class] Class to check
      # @return [Boolean]
      def accepts_config_kwarg?(klass)
        params = klass.instance_method(:initialize).parameters
        params.any? { |type, name| %i[key keyreq].include?(type) && name == :config }
      end

      def bootstrap_handlers!
        logger.info('🔧 Bootstrapping Ruby handler registry with template-driven discovery')

        registered_count = 0

        # Check if test environment has already loaded handlers
        if test_environment_active? && test_handlers_preloaded?
          logger.info('🧪 Test environment detected with preloaded handlers')
          registered_count = register_preloaded_handlers
        end

        # If no handlers registered yet, discover from templates
        if registered_count.zero?
          # Discover handlers from YAML templates
          discovered_handlers = discover_handlers_from_templates

          # Load and register discovered handlers
          discovered_handlers.each do |handler_class_name|
            # Try to load and register the handler
            handler_class = find_and_load_handler_class(handler_class_name)
            if handler_class
              register_handler(handler_class_name, handler_class)
              registered_count += 1
            else
              logger.debug("⚠️ Handler class not found for: #{handler_class_name}")
            end
          rescue StandardError => e
            logger.warn("❌ Failed to register handler #{handler_class_name}: #{e.message}")
          end
        end

        logger.info("✅ Handler registry bootstrapped with #{registered_count} handlers")
      end

      def load_example_handlers!
        # Load all example handler files from spec/handlers/examples/
        spec_dir = File.expand_path('../../../spec/handlers/examples', __dir__)
        return unless Dir.exist?(spec_dir)

        Dir.glob("#{spec_dir}/**/*_handler.rb").each do |handler_file|
          require handler_file
          logger.debug("✅ Loaded handler file: #{handler_file}")
        rescue StandardError => e
          logger.warn("❌ Failed to load handler file #{handler_file}: #{e.message}")
        end
      end

      # Discover handlers from YAML templates using the template discovery system
      def discover_handlers_from_templates
        template_path = determine_template_path

        if template_path
          logger.debug("🔍 Discovering handlers from template path: #{template_path}")
          TaskerCore::TemplateDiscovery::HandlerDiscovery.discover_all_handlers(template_path)
        else
          logger.warn('⚠️ No template directory found, no handlers will be discovered')
          []
        end
      end

      # Determine the template path based on environment and configuration
      def determine_template_path
        # 1. Explicit override takes highest priority
        return ENV['TASKER_TEMPLATE_PATH'] if ENV['TASKER_TEMPLATE_PATH']

        # 2. Test environment: default to spec/fixtures/templates
        if test_environment?
          fixtures_path = File.expand_path('../../../spec/fixtures/templates', __dir__)
          return fixtures_path if Dir.exist?(fixtures_path)
        end

        # 3. Production/development: use standard discovery
        TaskerCore::TemplateDiscovery::TemplatePath.find_template_config_directory
      end

      # Check if we're in a test environment
      def test_environment?
        ENV['TASKER_ENV'] == 'test' || ENV['RAILS_ENV'] == 'test'
      end

      # Find and dynamically load handler class by name
      def find_and_load_handler_class(handler_class_name)
        # First try to find if it's already loaded
        existing_class = find_loaded_handler_class(handler_class_name)
        return existing_class if existing_class

        # Try to load from common handler file patterns
        load_handler_file(handler_class_name)

        # Try to find it again after loading
        find_loaded_handler_class(handler_class_name)
      end

      # Find a handler class that's already loaded in memory
      def find_loaded_handler_class(handler_class_name)
        # Search through ObjectSpace for classes that match the handler name
        ObjectSpace.each_object(Class).find do |klass|
          klass.name&.end_with?(handler_class_name) &&
            klass < TaskerCore::StepHandler::Base
        end
      end

      # Attempt to load handler file using common naming conventions
      def load_handler_file(handler_class_name)
        # Convert CamelCase to snake_case for file names
        file_name = "#{handler_class_name.gsub(/([A-Z])/, '_\1').downcase.sub(/^_/, '')}.rb"

        # Try various common locations
        search_paths = handler_search_paths

        search_paths.each do |search_path|
          potential_file = File.join(search_path, file_name)
          next unless File.exist?(potential_file)

          begin
            require potential_file
            logger.debug("✅ Loaded handler file: #{potential_file}")
            return true
          rescue StandardError => e
            logger.debug("❌ Failed to load #{potential_file}: #{e.message}")
          end
        end

        false
      end

      # Get search paths for handler files
      def handler_search_paths
        paths = []

        # Add current working directory patterns
        %w[
          app/handlers
          lib/handlers
          handlers
          app/tasker/handlers
          lib/tasker/handlers
        ].each do |relative_path|
          full_path = File.expand_path(relative_path)
          paths << full_path if Dir.exist?(full_path)
        end

        # In test environment, add spec/handlers/examples for test fixtures
        if test_environment?
          spec_dir = File.expand_path('../../../spec/handlers/examples', __dir__)
          Dir.glob("#{spec_dir}/**/").each { |dir| paths << dir } if Dir.exist?(spec_dir)
        end

        paths
      end

      # Check if test environment is active
      def test_environment_active?
        return false unless defined?(TaskerCore::TestEnvironment)

        TaskerCore::TestEnvironment.loaded?
      end

      # Check if test environment has preloaded handler classes
      def test_handlers_preloaded?
        return false unless test_environment_active?

        # Check if we have handler classes loaded in ObjectSpace
        handler_count = 0
        ObjectSpace.each_object(Class) do |klass|
          if klass.name&.end_with?('Handler') &&
             klass.ancestors.any? { |ancestor| ancestor.name&.include?('StepHandler') }
            handler_count += 1
            break if handler_count.positive? # We just need to know if any exist
          end
        rescue StandardError
          next # Skip classes that can't be introspected
        end

        handler_count.positive?
      end

      # Register preloaded handlers from test environment
      def register_preloaded_handlers
        return 0 unless test_environment_active?

        registered_count = 0
        handler_names = TaskerCore::TestEnvironment.handler_names

        handler_names.each do |handler_class_name|
          # Find the loaded class - use the full class name from ObjectSpace
          handler_class = find_loaded_handler_class_by_full_name(handler_class_name)
          if handler_class
            # Register with FULL class name so templates can reference it properly
            register_handler(handler_class_name, handler_class)
            registered_count += 1
            logger.debug("✅ Registered preloaded test handler: #{handler_class_name}")
          else
            logger.debug("⚠️ Preloaded handler class not found: #{handler_class_name}")
          end
        rescue StandardError => e
          logger.warn("❌ Failed to register preloaded handler #{handler_class_name}: #{e.message}")
        end

        logger.info("📚 Registered #{registered_count} preloaded test handlers")
        registered_count
      end

      # Find a handler class by its full class name (e.g., "LinearWorkflow::StepHandlers::LinearStep1Handler")
      def find_loaded_handler_class_by_full_name(full_class_name)
        # Try to constantize the full class name
        full_class_name.split('::').reduce(Object) do |mod, const_name|
          mod.const_get(const_name)
        end
      rescue NameError
        # If not found by constantize, search ObjectSpace for a match
        ObjectSpace.each_object(Class).find do |klass|
          klass.name == full_class_name &&
            klass < TaskerCore::StepHandler::Base
        end
      end
    end
  end
end
