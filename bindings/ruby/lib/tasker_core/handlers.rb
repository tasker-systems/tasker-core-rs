# frozen_string_literal: true

module TaskerCore
  # Clean Handlers Domain API
  # 
  # This provides a clean namespace for all handler functionality
  # while preserving critical method signatures for step and task handlers.
  module Handlers
    # Step Handler API with preserved method signatures
    module Steps
      # Import the existing base step handler with preserved signatures
      require_relative 'step_handler/base'
      require_relative 'step_handler/api'
      
      # Re-export with clean namespace
      Base = TaskerCore::StepHandler::Base
      API = TaskerCore::StepHandler::Api
      
      class << self
        # Create a new step handler instance
        # @param handler_class [Class] Handler class to instantiate
        # @param config [Hash] Handler configuration
        # @return [Object] Handler instance
        def create(handler_class, config: {})
          handler_class.new(config: config)
        end
        
        # Validate step handler implementation
        # @param handler_class [Class] Handler class to validate
        # @return [Hash] Validation result
        def validate(handler_class)
          required_methods = %i[process]
          optional_methods = %i[process_results handle]
          
          missing = required_methods.reject { |method| handler_class.method_defined?(method) }
          present_optional = optional_methods.select { |method| handler_class.method_defined?(method) }
          
          {
            valid: missing.empty?,
            missing_required: missing,
            optional_implemented: present_optional,
            handler_class: handler_class.name
          }
        end
      end
    end
    
    # Task Handler API
    module Tasks
      require_relative 'task_handler'
      
      # Re-export with clean namespace  
      Base = TaskHandler
      
      class << self
        # Handle task with preserved signature
        # @param task_id [Integer] Task ID to handle
        # @return [Object] Task handle result
        def handle(task_id)
          Base.new.handle(task_id)
        end
        
        # Create task handler instance
        # @param config [Hash] Handler configuration
        # @return [TaskHandler] Handler instance
        def create(config: {})
          Base.new(config: config)
        end
      end
    end
  end
end