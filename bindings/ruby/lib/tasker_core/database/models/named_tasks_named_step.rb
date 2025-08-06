# typed: false
# frozen_string_literal: true

# == Schema Information
#
# Table name: named_tasks_named_steps
#
#  id                  :integer          not null, primary key
#  default_retry_limit :integer          default(3), not null
#  default_retryable   :boolean          default(TRUE), not null
#  skippable           :boolean          default(FALSE), not null
#  created_at          :datetime         not null
#  updated_at          :datetime         not null
#  named_step_id       :integer          not null
#  named_task_id       :integer          not null
#
# Indexes
#
#  named_tasks_named_steps_named_step_id_index  (named_step_id)
#  named_tasks_named_steps_named_task_id_index  (named_task_id)
#  named_tasks_steps_ids_unique                 (named_task_id,named_step_id) UNIQUE
#
# Foreign Keys
#
#  named_tasks_named_steps_named_step_id_foreign  (named_step_id => named_steps.named_step_id)
#  named_tasks_named_steps_named_task_id_foreign  (named_task_id => named_tasks.named_task_id)
#
module TaskerCore::Database::Models
  class NamedTasksNamedStep < ApplicationRecord
    belongs_to :named_task
    belongs_to :named_step
    validates :named_task_id, uniqueness: { scope: :named_step_id }
    validates :named_step_id, uniqueness: { scope: :named_task_id }

    scope :named_steps_for_named_task, lambda { |named_task_id|
      where(named_task_id: named_task_id).includes(:named_task).includes(:named_step)
    }

    def self.find_or_create(
      named_task,
      named_step,
      options = {
        default_retry_limit: 3,
        default_retryable: true,
        skippable: false
      }
    )
      inst = where(named_task_id: named_task.named_task_id, named_step_id: named_step.named_step_id).first

      inst ||= create({ named_task_id: named_task.named_task_id,
                        named_step_id: named_step.named_step_id }.merge(options))

      inst
    end

    def self.associate_named_step_with_named_task(named_task, template, named_step)
      ntns = named_steps_for_named_task(named_task.named_task_id).where(named_step: { name: named_step.name }).first
      return ntns if ntns

      dependent_system = Tasker::DependentSystem.find_or_create_by!(name: template.dependent_system)
      named_step = Tasker::NamedStep.find_or_create_by!(name: template.name,
                                                        dependent_system_id: dependent_system.dependent_system_id)
      find_or_create(
        named_task,
        named_step,
        {
          default_retry_limit: template.default_retry_limit,
          default_retryable: template.default_retryable,
          skippable: template.skippable
        }
      )
    end

    def task_name
      named_task.name
    end

    def step_name
      named_step.name
    end
  end
end
