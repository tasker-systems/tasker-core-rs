# frozen_string_literal: true

require 'singleton'

module TaskerCore
  module Internal
    # Simple stub for OrchestrationManager to satisfy StepHandler::Base dependencies
    # In the FFI worker context, we don't need the full Rails engine infrastructure
    class OrchestrationManager
      include Singleton

      def orchestration_system
        # Return a simple stub that satisfies the interface expected by handlers
        NullOrchestrationSystem.new
      end
    end

    # Minimal orchestration system stub for FFI worker context
    class NullOrchestrationSystem
      def method_missing(_method_name, *_args, **_kwargs)
        # Log that the method was called but return nil
        # This allows handlers to work without crashing
        nil
      end

      def respond_to_missing?(_method_name, _include_private = false)
        true
      end
    end
  end
end
