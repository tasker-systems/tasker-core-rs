# frozen_string_literal: true

# TaskerCore execution namespace for step execution implementations
module TaskerCore
  module Execution
    # LEGACY: ZeroMQHandler has been replaced by BatchStepExecutionOrchestrator
    # 
    # For modern ZeroMQ-based step execution, use:
    # - TaskerCore::Orchestration::BatchStepExecutionOrchestrator (concurrent worker pools)
    # - TaskerCore::Orchestration::ZeromqOrchestrator (socket management)
    # - TaskerCore::Config (YAML-based configuration)
    # - TaskerCore::Types (organized type system with validation)
  end
end