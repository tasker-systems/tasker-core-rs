# frozen_string_literal: true

require 'json'

module TaskerCore
  module Messaging
    class MessageManager
      def self.logger
        TaskerCore::Logging::Logger.instance
      end

      # @param msg_data [TaskerCore::Types::SimpleQueueMessageData] Simple queue message data
      # @return [Array<TaskerCore::Database::Models::Task, TaskerCore::Execution::StepSequence, TaskerCore::Database::Models::WorkflowStep>] Task, sequence, and step
      def self.get_records_from_message(msg_data)
        task = TaskerCore::Database::Models::Task.with_steps_and_transitions.find_by!(task_uuid: msg_data.task_uuid)
        # we load all of the steps beforehand so this select should be only in memory
        step = task.workflow_steps.select { |step| step.step_uuid == msg_data.step_uuid }.first
        dependency_steps = []
        unless msg_data.ready_dependency_step_uuids.empty?
          # same as above, this should be in memory because we loaded them above
          dependency_steps = task.workflow_steps.select { |step| msg_data.ready_dependency_step_uuids.include?(step.step_uuid) }
        end
        sequence = TaskerCore::Execution::StepSequence.new(dependency_steps)
        [task, sequence, step]
      end
    end
  end
end
