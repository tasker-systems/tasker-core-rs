# frozen_string_literal: true

module TaskerCore
  module TestEnvironment
    # Test environment conditional loading that only activates when TASKER_ENV=test
    # Loads example handlers and configures test-specific paths for template discovery
    class ConditionalLoader
      include Singleton

      attr_reader :logger, :loaded, :example_handlers_path, :template_fixtures_path

      def initialize
        @logger = nil # Will be set when logger is available
        @loaded = false
        @example_handlers_path = File.expand_path('../../spec/handlers/examples', __dir__)
        @template_fixtures_path = File.expand_path('../../spec/fixtures/templates', __dir__)
      end

      # Check if we should load test environment components
      def should_load_test_environment?
        test_env = ENV['TASKER_ENV']&.downcase == 'test'
        rails_test_env = ENV['RAILS_ENV']&.downcase == 'test'
        force_examples = ENV['TASKER_FORCE_EXAMPLE_HANDLERS'] == 'true'

        test_env || rails_test_env || force_examples
      end

      # Load test environment components if appropriate
      def load_if_test_environment!
        return false unless should_load_test_environment?
        return true if @loaded # Already loaded

        # Defer logger initialization until after TaskerCore loads
        @logger = TaskerCore::Logger.instance

        log_info('🧪 Test environment detected, loading example handlers and templates')

        # Set template path override for test fixtures
        setup_test_template_path!

        # Pre-load all example handler files
        load_example_handler_files!

        # Verify example handlers are available
        verify_test_setup!

        @loaded = true
        log_info("✅ Test environment setup complete: #{loaded_handler_count} example handlers loaded")
        true
      end

      # Get count of loaded handler classes for verification
      def loaded_handler_count
        return 0 unless @loaded

        count = 0
        ObjectSpace.each_object(Class) do |klass|
          if klass.name&.end_with?('Handler') &&
             klass.ancestors.any? { |ancestor| ancestor.name&.include?('StepHandler') }
            count += 1
          end
        rescue StandardError
          # Skip classes that can't be introspected
          next
        end
        count
      end

      # Get list of loaded example handler class names
      def loaded_handler_names
        return [] unless @loaded

        names = []
        ObjectSpace.each_object(Class) do |klass|
          if klass.name&.end_with?('Handler') &&
             klass.ancestors.any? { |ancestor| ancestor.name&.include?('StepHandler') }
            names << klass.name
          end
        rescue StandardError
          # Skip classes that can't be introspected
          next
        end
        names.sort
      end

      # Get template discovery info for debugging
      def test_template_info
        return {} unless @loaded

        {
          template_path: ENV['TASKER_TEMPLATE_PATH'],
          fixtures_path: @template_fixtures_path,
          fixtures_exist: Dir.exist?(@template_fixtures_path),
          template_files: Dir.exist?(@template_fixtures_path) ? Dir.glob("#{@template_fixtures_path}/*.yaml").count : 0,
          example_handlers_path: @example_handlers_path,
          examples_exist: Dir.exist?(@example_handlers_path),
          handler_files: Dir.exist?(@example_handlers_path) ? Dir.glob("#{@example_handlers_path}/**/*_handler.rb").count : 0
        }
      end

      private

      def log_info(message)
        if @logger
          @logger.info(message)
        else
          puts message # Fallback if logger not available yet
        end
      end

      def log_debug(message)
        if @logger && @logger.respond_to?(:debug)
          @logger.debug(message)
        elsif ENV['LOG_LEVEL'] == 'debug'
          puts "[DEBUG] #{message}"
        end
      end

      def log_warn(message)
        if @logger
          @logger.warn(message)
        else
          puts "[WARN] #{message}"
        end
      end

      def setup_test_template_path!
        # Override template path to use test fixtures if not already set
        unless ENV['TASKER_TEMPLATE_PATH']
          if Dir.exist?(@template_fixtures_path)
            ENV['TASKER_TEMPLATE_PATH'] = @template_fixtures_path
            log_debug("📁 Set TASKER_TEMPLATE_PATH to: #{@template_fixtures_path}")
          else
            log_warn("⚠️  Test template fixtures directory not found: #{@template_fixtures_path}")
          end
        end
      end

      def load_example_handler_files!
        return unless Dir.exist?(@example_handlers_path)

        handler_files = Dir.glob("#{@example_handlers_path}/**/*_handler.rb")
        log_debug("🔍 Found #{handler_files.count} example handler files")

        loaded_count = 0
        handler_files.each do |handler_file|
          begin
            # Use require instead of require_relative to avoid duplicate loading
            require handler_file
            loaded_count += 1
            log_debug("✅ Loaded handler file: #{File.basename(handler_file)}")
          rescue LoadError => e
            log_warn("❌ Failed to load handler file #{handler_file}: #{e.message}")
          rescue StandardError => e
            log_warn("❌ Error loading handler file #{handler_file}: #{e.class} - #{e.message}")
          end
        end

        log_info("📚 Loaded #{loaded_count}/#{handler_files.count} example handler files")
      end

      def verify_test_setup!
        # Verify template path is set correctly
        template_path = ENV['TASKER_TEMPLATE_PATH']
        if template_path.nil?
          log_warn('⚠️  TASKER_TEMPLATE_PATH not set, template discovery may fail')
        elsif !Dir.exist?(template_path)
          log_warn("⚠️  TASKER_TEMPLATE_PATH directory does not exist: #{template_path}")
        else
          yaml_files = Dir.glob("#{template_path}/*.yaml").count
          log_debug("📄 Found #{yaml_files} YAML template files in #{template_path}")
        end

        # Verify example handlers directory exists
        unless Dir.exist?(@example_handlers_path)
          log_warn("⚠️  Example handlers directory not found: #{@example_handlers_path}")
          return
        end

        # Count available handler classes
        handler_count = loaded_handler_count
        if handler_count.zero?
          log_warn('⚠️  No example handler classes found after loading')
        else
          log_debug("🎯 #{handler_count} example handler classes are available")
        end
      end
    end

    # Main interface - call this to conditionally load test environment
    def self.load_if_test!
      ConditionalLoader.instance.load_if_test_environment!
    end

    # Check if test environment is loaded
    def self.loaded?
      ConditionalLoader.instance.loaded
    end

    # Get test environment info for debugging
    def self.info
      loader = ConditionalLoader.instance
      base_info = {
        should_load: loader.should_load_test_environment?,
        loaded: loader.loaded,
        handler_count: loader.loaded_handler_count
      }

      if loader.loaded
        base_info.merge(loader.test_template_info)
      else
        base_info
      end
    end

    # Get loaded handler names for debugging
    def self.handler_names
      ConditionalLoader.instance.loaded_handler_names
    end
  end
end
