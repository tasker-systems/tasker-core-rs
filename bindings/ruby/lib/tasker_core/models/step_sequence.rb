# frozen_string_literal: true

module TaskerCore
  module Models
    class StepSequence
      attr_accessor :total_steps, :current_position, :dependencies, :current_step_id, :all_steps

      def find_dependency_step_by_name(name)
        dependencies.find { |step| step.name.to_s == name.to_s }
      end

      def find_step_by_name(name)
        all_steps.find { |step| step.name.to_s == name.to_s }
      end

      def to_h
        {
          total_steps: total_steps,
          current_position: current_position,
          dependencies: dependencies,
          current_step_id: current_step_id,
          all_steps: all_steps
        }
      end
    end
  end
end
