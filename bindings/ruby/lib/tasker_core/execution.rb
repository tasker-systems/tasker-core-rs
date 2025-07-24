# frozen_string_literal: true

# TaskerCore execution namespace for step execution implementations
module TaskerCore
  module Execution
    # ZeroMQ-based step execution for language-agnostic orchestration
    autoload :ZeroMQHandler, 'tasker_core/execution/zeromq_handler'
  end
end