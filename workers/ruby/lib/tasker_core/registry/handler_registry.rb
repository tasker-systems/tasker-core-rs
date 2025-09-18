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

      private

      def bootstrap_handlers!
        logger.info("🔧 Bootstrapping Ruby handler registry")

        registered_count = 0

        # Discover and register handlers from task templates
        discover_handler_classes.each do |handler_class_name|
          handler_class = Object.const_get(handler_class_name)
          register_handler(handler_class_name, handler_class)
          registered_count += 1
        rescue NameError
          logger.debug("⚠️ Handler class not found: #{handler_class_name}")
        rescue StandardError => e
          logger.warn("❌ Failed to register handler #{handler_class_name}: #{e.message}")
        end

        logger.info("✅ Handler registry bootstrapped with #{registered_count} handlers")
      end

      def discover_handler_classes
        # This would typically scan task template configurations
        # For now, return known handler classes from examples
        [
          'LinearStep1Handler',
          'LinearStep2Handler',
          'LinearStep3Handler',
          'LinearStep4Handler',
          'DiamondStartHandler',
          'DiamondBranchBHandler',
          'DiamondBranchCHandler',
          'DiamondEndHandler',
          'ValidateOrderHandler',
          'ReserveInventoryHandler',
          'ProcessPaymentHandler',
          'ShipOrderHandler'
        ]
      end
    end
  end
end
