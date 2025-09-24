# frozen_string_literal: true

require 'singleton'

module TaskerCore
  module Registry
    # Simplified handler registry for pure business logic handlers
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

      # Get template discovery information for debugging
      def template_discovery_info
        template_path = TaskerCore::TemplateDiscovery::TemplatePath.find_template_config_directory

        {
          template_path: template_path,
          template_files: template_path ? TaskerCore::TemplateDiscovery::TemplatePath.discover_template_files(template_path) : [],
          discovered_handlers: template_path ? TaskerCore::TemplateDiscovery::HandlerDiscovery.discover_all_handlers(template_path) : [],
          handlers_by_namespace: template_path ? TaskerCore::TemplateDiscovery::HandlerDiscovery.discover_handlers_by_namespace(template_path) : {},
          environment: ENV['TASKER_ENV'] || ENV['RAILS_ENV'] || 'development',
          fallback_enabled: fallback_to_examples?
        }
      end

      private

      def bootstrap_handlers!
        logger.info('🔧 Bootstrapping Ruby handler registry with template-driven discovery')

        registered_count = 0

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

        # Fallback to example handlers in test/development environments if needed
        if registered_count.zero? && fallback_to_examples?
          logger.info('📚 No handlers found from templates, falling back to example handlers')
          load_example_handlers_fallback!
          registered_count = load_example_handlers_from_discovery
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
        template_path = get_template_path_override || TaskerCore::TemplateDiscovery::TemplatePath.find_template_config_directory

        # In test environments, also check for spec fixtures if no other path found
        if template_path.nil? && (ENV['TASKER_ENV'] == 'test' || ENV['RAILS_ENV'] == 'test')
          fixtures_path = File.expand_path('../../../spec/fixtures/templates', __dir__)
          template_path = fixtures_path if Dir.exist?(fixtures_path)
        end

        if template_path
          logger.debug("🔍 Discovering handlers from template path: #{template_path}")
          TaskerCore::TemplateDiscovery::HandlerDiscovery.discover_all_handlers(template_path)
        else
          logger.warn('⚠️ No template directory found, no handlers will be discovered')
          []
        end
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

      # Get template path override for testing
      def get_template_path_override
        # Support both test-specific override and general override
        ENV['TASKER_TEST_TEMPLATE_PATH'] || ENV.fetch('TASK_TEMPLATE_PATH_OVERRIDE', nil)
      end

      # Determine if we should fallback to example handlers
      def fallback_to_examples?
        # Always fallback if explicitly enabled for tests
        return true if ENV['TASKER_FORCE_EXAMPLE_HANDLERS'] == 'true'

        # Only fallback in test or development environments
        %w[test development].include?(ENV['TASKER_ENV'] || ENV['RAILS_ENV'] || 'development')
      end

      # Load example handlers as fallback (for tests/development)
      def load_example_handlers_fallback!
        load_example_handlers!
      end

      # Load example handlers using the old discovery method
      def load_example_handlers_from_discovery
        registered_count = 0
        discover_handler_classes_fallback.each do |handler_class_name, module_name|
          handler_class = Object.const_get("#{module_name}::StepHandlers::#{handler_class_name}")
          register_handler(handler_class_name, handler_class)
          registered_count += 1
        rescue NameError
          logger.debug("⚠️ Handler class not found: #{module_name}::StepHandlers::#{handler_class_name}")
        rescue StandardError => e
          logger.warn("❌ Failed to register handler #{handler_class_name}: #{e.message}")
        end
        registered_count
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

        # Add example handlers for fallback
        spec_dir = File.expand_path('../../../spec/handlers/examples', __dir__)
        Dir.glob("#{spec_dir}/**/").each { |dir| paths << dir } if Dir.exist?(spec_dir)

        paths
      end

      # Fallback discovery for example handlers (old hardcoded method)
      def discover_handler_classes_fallback
        [
          %w[LinearStep1Handler LinearWorkflow],
          %w[LinearStep2Handler LinearWorkflow],
          %w[LinearStep3Handler LinearWorkflow],
          %w[LinearStep4Handler LinearWorkflow],
          %w[DiamondStartHandler DiamondWorkflow],
          %w[DiamondBranchBHandler DiamondWorkflow],
          %w[DiamondBranchCHandler DiamondWorkflow],
          %w[DiamondEndHandler DiamondWorkflow],
          %w[ValidateOrderHandler OrderFulfillment],
          %w[ReserveInventoryHandler OrderFulfillment],
          %w[ProcessPaymentHandler OrderFulfillment],
          %w[ShipOrderHandler OrderFulfillment]
        ]
      end
    end
  end
end
