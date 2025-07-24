# frozen_string_literal: true

module TaskerCore
  module Orchestration
    # Enhanced handler registry supporting flexible callable interfaces
    # 
    # This registry extends the base TaskHandlerRegistry to support the revolutionary
    # .call(task, sequence, step) interface, enabling registration of Procs, Lambdas,
    # classes with call methods, and any callable object.
    #
    # @example Register a Proc
    #   registry = EnhancedHandlerRegistry.new
    #   registry.register_proc('OrderProcessor') do |task, sequence, step|
    #     { status: 'completed', output: process_order(task.context) }
    #   end
    #
    # @example Register a Lambda
    #   order_validator = ->(task, sequence, step) do
    #     validate_order_data(task.context, step.handler_config)
    #   end
    #   registry.register_callable('OrderValidator', order_validator)
    #
    # @example Register a class-based callable
    #   class PaymentProcessor
    #     def call(task, sequence, step)
    #       process_payment(task.context[:payment_info])
    #     end
    #   end
    #   registry.register_callable('PaymentProcessor', PaymentProcessor.new)
    class EnhancedHandlerRegistry
      
      def initialize
        @callables = {}
        @validation_enabled = true
        
        logger&.info "EnhancedHandlerRegistry initialized with callable support"
      end

      # Register a callable object directly
      # @param handler_class [String] The handler class name/identifier
      # @param callable [Object] Any object that responds to .call(task, sequence, step)
      # @return [void]
      def register_callable(handler_class, callable)
        validate_callable_interface!(callable) if @validation_enabled
        @callables[handler_class.to_s] = callable
        
        logger&.debug "Registered callable for #{handler_class}: #{callable.class.name}"
      end
      
      # Register a Proc/Lambda for step processing
      # @param handler_class [String] The handler class name/identifier
      # @param block [Proc] Block that will be called with (task, sequence, step)
      # @return [void]
      def register_proc(handler_class, &block)
        raise ArgumentError, "Block required for register_proc" unless block_given?
        register_callable(handler_class, block)
      end

      # Register a class that has a call method
      # @param handler_class [String] The handler class name/identifier
      # @param klass [Class] Class that responds to .call or can be instantiated to create callable
      # @return [void]
      def register_class(handler_class, klass)
        if klass.respond_to?(:call)
          # Class itself is callable (has class method .call)
          register_callable(handler_class, klass)
        else
          # Instantiate the class and register the instance
          instance = klass.new
          register_callable(handler_class, instance)
        end
      end
      
      # Get callable for step execution using priority resolution order
      # @param handler_class [String] The handler class name/identifier
      # @return [Object, nil] Callable object or nil if none found
      def get_callable_for_class(handler_class)
        handler_key = handler_class.to_s
        
        # Priority 1: Direct callable registration (Procs, Lambdas, etc.)
        return @callables[handler_key] if @callables.key?(handler_key)
        
        # Priority 2: Class with .call method
        begin
          klass = handler_key.constantize
          return klass if klass.respond_to?(:call)
        rescue NameError
          # Class doesn't exist, continue to next priority
        end
        
        # Priority 3: Instance with .call method
        instance = get_handler_instance(handler_key)
        return instance if instance&.respond_to?(:call)
        
        # Priority 4: Legacy .process method wrapped in callable
        if instance&.respond_to?(:process)
          logger&.debug "Wrapping legacy .process method for #{handler_key}"
          return create_process_wrapper(instance)
        end
        
        nil
      end

      # Get handler instance using traditional resolution
      # @param handler_class [String] The handler class name/identifier
      # @return [Object, nil] Handler instance or nil if not found
      def get_handler_instance(handler_class)
        # Direct class instantiation
        begin
          klass = handler_class.to_s.constantize
          klass.new
        rescue NameError => e
          logger&.warn "Could not instantiate handler class #{handler_class}: #{e.message}"
          nil
        rescue => e
          logger&.error "Error instantiating #{handler_class}: #{e.message}"
          nil
        end
      end

      # Remove a registered callable
      # @param handler_class [String] The handler class name/identifier
      # @return [Object, nil] The removed callable or nil if not found
      def unregister_callable(handler_class)
        @callables.delete(handler_class.to_s)
      end

      # List all registered callables
      # @return [Hash] Hash of handler_class => callable
      def list_callables
        @callables.dup
      end

      # Check if a callable is registered for a handler class
      # @param handler_class [String] The handler class name/identifier
      # @return [Boolean] True if callable is registered
      def callable_registered?(handler_class)
        @callables.key?(handler_class.to_s)
      end

      # Clear all registered callables
      # @return [void]
      def clear_callables!
        @callables.clear
        logger&.info "Cleared all registered callables"
      end

      # Enable or disable callable validation
      # @param enabled [Boolean] Whether to validate callable interfaces
      # @return [void]
      def validation_enabled=(enabled)
        @validation_enabled = !!enabled
        logger&.debug "Callable validation #{enabled ? 'enabled' : 'disabled'}"
      end

      # Get statistics about registered callables
      # @return [Hash] Statistics about the registry
      def stats
        {
          total_callables: @callables.size,
          callable_types: @callables.values.group_by(&:class).transform_values(&:size),
          validation_enabled: @validation_enabled,
          handler_classes: @callables.keys
        }
      end

      private
      
      # Use TaskerCore singleton logger for thread-safe logging
      def logger
        TaskerCore::Logging::Logger.instance
      end
      
      # Validate that a callable has the correct interface
      def validate_callable_interface!(callable)
        unless callable.respond_to?(:call)
          raise ArgumentError, "Callable must respond to .call method"
        end
        
        # Check arity if possible (some callables don't support arity inspection)
        if callable.respond_to?(:arity)
          expected_arity = 3 # (task, sequence, step)
          actual_arity = callable.arity
          
          # Handle variable arity (-1 means accepts any number of args)
          if actual_arity >= 0 && actual_arity != expected_arity
            logger&.warn "Callable arity is #{actual_arity}, expected #{expected_arity} (task, sequence, step)"
          end
        end
        
        # Additional validation for common callable types
        case callable
        when Proc, Method
          # These are always valid if they respond to call
          true
        when Class
          # Class should have a call method
          unless callable.respond_to?(:call)
            raise ArgumentError, "Class #{callable.name} must have a .call class method"
          end
        else
          # Instance should have call method (already checked above)
          true
        end
      end

      # Create a wrapper that adapts .process method to .call interface
      def create_process_wrapper(handler_instance)
        ->(task, sequence, step) do
          # Adapt the new call interface to the legacy process interface
          # This maintains backward compatibility while enabling the new architecture
          begin
            handler_instance.process(task, sequence, step)
          rescue ArgumentError => e
            # Handle cases where .process expects different arguments
            logger&.warn "Process method argument mismatch for #{handler_instance.class.name}: #{e.message}"
            
            # Try with just the task if that's what the handler expects
            if handler_instance.method(:process).arity == 1
              handler_instance.process(task)
            else
              raise e
            end
          end
        end
      end
    end
  end
end