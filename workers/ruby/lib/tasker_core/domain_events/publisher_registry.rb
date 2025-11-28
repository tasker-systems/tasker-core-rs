# frozen_string_literal: true

require 'singleton'

module TaskerCore
  module DomainEvents
    # TAS-65: Registry for custom domain event publishers
    #
    # Maps publisher names (from YAML configuration) to their Ruby implementations.
    # Publishers are registered at bootstrap time and validated against task templates.
    #
    # @example Registering publishers at bootstrap
    #   registry = TaskerCore::DomainEvents::PublisherRegistry.instance
    #
    #   # Register custom publishers
    #   registry.register(PaymentEventPublisher.new)
    #   registry.register(InventoryEventPublisher.new)
    #
    #   # Validate against loaded templates
    #   required = ['PaymentEventPublisher', 'InventoryEventPublisher', 'MissingPublisher']
    #   registry.validate_required!(required)
    #   # => raises ValidationError: Missing publishers: MissingPublisher
    #
    # @example Looking up publishers
    #   publisher = registry.get('PaymentEventPublisher')
    #   publisher.transform_payload(step_result, event_declaration)
    #
    # @example Using default publisher
    #   # Returns DefaultPublisher for unregistered names
    #   publisher = registry.get_or_default('UnknownPublisher')
    #
    class PublisherRegistry
      include Singleton

      attr_reader :logger, :publishers, :default_publisher

      def initialize
        @logger = TaskerCore::Logger.instance
        @publishers = {}
        @default_publisher = DefaultPublisher.new
        @frozen = false
      end

      # Register a custom publisher
      #
      # @param publisher [BasePublisher] The publisher instance to register
      # @return [BasePublisher, nil] The previous publisher with the same name, if any
      # @raise [RegistryFrozenError] If the registry has been frozen
      def register(publisher)
        raise RegistryFrozenError, 'Registry is frozen after validation' if @frozen

        name = publisher.name
        logger.info "Registering domain event publisher: #{name}"
        previous = @publishers[name]
        @publishers[name] = publisher
        previous
      end

      # Get a publisher by name
      #
      # @param name [String] The publisher name
      # @return [BasePublisher, nil] The publisher, or nil if not found
      def get(name)
        @publishers[name]
      end

      # Get a publisher by name, or return the default if not found
      #
      # @param name [String, nil] The publisher name
      # @return [BasePublisher] The publisher or default
      def get_or_default(name)
        return @default_publisher if name.nil? || name == 'default'

        @publishers[name] || begin
          logger.warn "Publisher #{name} not found, using default"
          @default_publisher
        end
      end

      # Get a publisher by name with strict mode (no fallback)
      #
      # @param name [String] The publisher name
      # @return [BasePublisher] The publisher
      # @raise [PublisherNotFoundError] If the publisher is not registered
      def get_strict(name)
        return @default_publisher if name == 'default'

        @publishers[name] || raise(
          PublisherNotFoundError.new(name, registered_names)
        )
      end

      # Check if a publisher is registered
      #
      # @param name [String] The publisher name
      # @return [Boolean]
      def registered?(name)
        @publishers.key?(name) || name == 'default'
      end

      # Get all registered publisher names
      #
      # @return [Array<String>]
      def registered_names
        @publishers.keys
      end

      # Get count of registered publishers
      #
      # @return [Integer]
      def count
        @publishers.size
      end

      # Check if registry has no custom publishers
      #
      # @return [Boolean]
      def empty?
        @publishers.empty?
      end

      # Unregister a publisher by name
      #
      # @param name [String] The publisher name
      # @return [BasePublisher, nil] The removed publisher, if any
      # @raise [RegistryFrozenError] If the registry has been frozen
      def unregister(name)
        raise RegistryFrozenError, 'Registry is frozen after validation' if @frozen

        logger.info "Unregistering domain event publisher: #{name}"
        @publishers.delete(name)
      end

      # Clear all registered publishers
      #
      # @raise [RegistryFrozenError] If the registry has been frozen
      def clear
        raise RegistryFrozenError, 'Registry is frozen after validation' if @frozen

        logger.info 'Clearing all domain event publishers'
        @publishers.clear
      end

      # TAS-65: Validate that all required publishers are registered
      #
      # Implements "loud failure validation" - validates at init time that all
      # publisher names referenced in task templates exist in the registry.
      # After validation, the registry is frozen to prevent changes.
      #
      # @param required_publishers [Array<String>] Publisher names from YAML configs
      # @return [true] If all required publishers are registered
      # @raise [ValidationError] If some publishers are missing
      def validate_required!(required_publishers)
        missing = []

        required_publishers.each do |name|
          next if name == 'default'
          next if registered?(name)

          missing << name
        end

        if missing.any?
          raise ValidationError.new(missing, registered_names)
        end

        @frozen = true
        logger.info "Publisher validation passed. Registered: #{registered_names.join(', ')}"
        true
      end

      # Check if the registry is frozen
      #
      # @return [Boolean]
      def frozen?
        @frozen
      end

      # Reset the registry (for testing)
      #
      # @note This unfreezes the registry - use only in tests
      def reset!
        @publishers.clear
        @frozen = false
        logger.info 'Publisher registry reset'
      end
    end

    # Default publisher that passes step result through unchanged
    class DefaultPublisher < BasePublisher
      def name
        'default'
      end

      # Default implementation: return step result as payload
      def transform_payload(step_result, event_declaration, step_context = nil)
        step_result[:result] || {}
      end
    end

    # Error raised when registry validation fails
    class ValidationError < StandardError
      attr_reader :missing_publishers, :registered_publishers

      def initialize(missing, registered)
        @missing_publishers = missing
        @registered_publishers = registered
        super("Missing publishers: #{missing.join(', ')}. Registered: #{registered.join(', ')}")
      end
    end

    # Error raised when a publisher is not found in strict mode
    class PublisherNotFoundError < StandardError
      attr_reader :publisher_name, :registered_publishers

      def initialize(name, registered)
        @publisher_name = name
        @registered_publishers = registered
        super("Publisher '#{name}' not found. Registered: #{registered.join(', ')}")
      end
    end

    # Error raised when trying to modify a frozen registry
    class RegistryFrozenError < StandardError; end
  end
end
