# frozen_string_literal: true

require 'singleton'

module TaskerCore
  module Registry
    # Simplified handler registry for pure business logic handlers
    #
    # Manages the discovery, registration, and instantiation of step handlers
    # throughout the worker lifecycle. The registry supports multiple discovery
    # modes to handle different deployment scenarios and environments.
    #
    # Discovery Modes (in priority order):
    # 1. **Preloaded Handlers**: Test environment handlers loaded at startup
    # 2. **Template-Driven Discovery**: YAML templates defining workflow handlers
    #
    # The registry automatically bootstraps on initialization, discovering and
    # registering all available handlers based on the current environment and
    # configuration.
    #
    # @example Resolving a handler by class name
    #   registry = TaskerCore::Registry::HandlerRegistry.instance
    #   handler = registry.resolve_handler("ValidateOrderHandler")
    #   # => Instance of ValidateOrderHandler or nil if not found
    #
    #   if handler
    #     result = handler.call(task, sequence, step)
    #   else
    #     raise "Handler not found: ValidateOrderHandler"
    #   end
    #
    # @example Checking handler availability
    #   registry.handler_available?("ProcessPaymentHandler")
    #   # => true/false
    #
    #   if registry.handler_available?("ProcessPaymentHandler")
    #     puts "Payment processing is available"
    #   end
    #
    # @example Getting list of registered handlers
    #   handlers = registry.registered_handlers
    #   # => ["LinearStep1Handler", "LinearStep2Handler", "ValidateOrderHandler", ...]
    #
    #   puts "Available handlers:"
    #   handlers.each { |name| puts "  - #{name}" }
    #
    # @example Debugging template discovery
    #   info = registry.template_discovery_info
    #   # => {
    #   #   template_path: "/app/config/tasker/tasks",
    #   #   template_files: ["linear_workflow.yml", "order_fulfillment.yml"],
    #   #   discovered_handlers: ["ValidateOrderHandler", "ProcessPaymentHandler", ...],
    #   #   handlers_by_namespace: {
    #   #     "payments" => ["ProcessPaymentHandler", "RefundHandler"],
    #   #     "fulfillment" => ["ValidateOrderHandler", "ShipOrderHandler"]
    #   #   },
    #   #   environment: "production",
    #   #   fallback_enabled: false
    #   # }
    #
    #   puts "Template path: #{info[:template_path]}"
    #   puts "Discovered #{info[:discovered_handlers].size} handlers"
    #
    # @example Manual handler registration
    #   # Register a custom handler at runtime
    #   registry.register_handler("CustomHandler", CustomHandlerClass)
    #   # => Logs: "✅ Registered handler: CustomHandler"
    #
    # Discovery Priority Explained:
    #
    # 1. **Test Environment Preloaded Handlers** (TASKER_ENV=test):
    #    - Checks if TaskerCore::TestEnvironment has loaded handlers
    #    - Uses ObjectSpace to find loaded handler classes
    #    - Fastest discovery, no file I/O needed
    #
    # 2. **YAML Template-Driven Discovery**:
    #    - Scans template directory for YAML files
    #    - Extracts handler_class from step definitions
    #    - Loads handler files and registers classes
    #
    # Template Path Resolution (in priority order):
    # 1. **TASKER_TEMPLATE_PATH**: Explicit override (highest priority)
    # 2. **TASKER_ENV=test**: Defaults to spec/fixtures/templates
    # 3. **Production/Development**: Uses standard config/tasker/tasks discovery
    #
    # Environment Variables:
    # - **TASKER_ENV**: Current environment (test/development/production)
    # - **RAILS_ENV**: Rails environment (fallback for TASKER_ENV)
    # - **TASKER_TEMPLATE_PATH**: Explicit template directory override
    #
    # Handler Search Paths:
    # - app/handlers/
    # - lib/handlers/
    # - handlers/
    # - app/tasker/handlers/
    # - lib/tasker/handlers/
    # - spec/handlers/examples/ (test environment only)
    #
    # @see TaskerCore::TemplateDiscovery For template discovery implementation
    # @see TaskerCore::TestEnvironment For test environment integration
    # @see TaskerCore::StepHandler::Base For handler base class
    class HandlerRegistry
      include Singleton

      attr_reader :logger, :handlers

      def initialize
        @logger = TaskerCore::Logger.instance
        @handlers = {}
        bootstrap_handlers!
      end

      # Resolve handler by class name
      def resolve_handler(handler_class_name)
        handler_class = @handlers[handler_class_name]
        return nil unless handler_class

        # Simple instantiation - no infrastructure setup needed
        handler_class.new
      rescue StandardError => e
        logger.error("💥 Failed to instantiate handler #{handler_class_name}: #{e.message}")
        nil
      end

      # Register handler class
      def register_handler(class_name, handler_class)
        @handlers[class_name] = handler_class
        logger.debug("✅ Registered handler: #{class_name}")
      end

      # Check if handler is available
      def handler_available?(class_name)
        @handlers.key?(class_name)
      end

      # Get all registered handler names
      def registered_handlers
        @handlers.keys.sort
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

      private

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
