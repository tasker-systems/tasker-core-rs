# frozen_string_literal: true

module TaskerCore
  module Utils
    class TemplateLoader
      def self.logger
        TaskerCore::Logging::Logger.instance
      end

      def self.config
        TaskerCore::Config.instance
      end

      def self.registry
        TaskerCore::Registry::TaskTemplateRegistry.instance
      end

      def self.load_templates!
        # Get search paths from configuration (environment-appropriate)
        search_patterns = config.task_template_search_paths
        logger.debug "📁 TaskTemplate search patterns from config: #{search_patterns}"

        task_templates = Dir.glob(search_patterns.join(' '))

        task_templates.map do |pattern|
          registry.register_task_template_from_yaml(pattern)
        end
      end
    end
  end
end
