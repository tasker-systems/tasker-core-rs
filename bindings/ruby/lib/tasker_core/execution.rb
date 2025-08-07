# frozen_string_literal: true

# Load all execution modules
require_relative 'execution/step_sequence'

module TaskerCore
  # Execution framework for step processing and workflow management
  #
  # This module provides the infrastructure for executing workflow steps
  # in the simplified UUID-based message architecture. It handles step
  # sequence management and dependency tracking using ActiveRecord models.
  #
  # @example Using StepSequence
  #   dependencies = TaskerCore::Database::Models::WorkflowStep.where(
  #     step_uuid: message.ready_dependency_step_uuids
  #   )
  #   sequence = TaskerCore::Execution::StepSequence.new(dependencies)
  #   
  #   handler.call(task, sequence, step)
  #
  module Execution
    # Version information for execution framework
    def self.version_info
      {
        version: '5.5.0',
        architecture: 'UUID-based simplified messages',
        dependencies: 'ActiveRecord models',
        status: 'active'
      }
    end
    
    # Health check for execution framework
    def self.health
      {
        step_sequence: defined?(StepSequence),
        active_record: defined?(ActiveRecord),
        status: 'healthy'
      }
    end
  end
end