# frozen_string_literal: true

# Orchestration module containing the revolutionary concurrent execution architecture
module TaskerCore
  module Orchestration
    # Module for production-grade concurrent step execution using ZeroMQ and concurrent-ruby
    
    # Load orchestration components
    autoload :BatchStepExecutionOrchestrator, 'tasker_core/orchestration/batch_step_execution_orchestrator'
    autoload :EnhancedHandlerRegistry, 'tasker_core/orchestration/enhanced_handler_registry'
  end
end