# typed: false
# frozen_string_literal: true

# == Schema Information
#
# Table name: tasker_named_steps
#
#  named_step_uuid        :uuid             not null, primary key, default(uuid_generate_v7())
#  dependent_system_uuid  :uuid             not null
#  name                   :string(128)      not null
#  description            :string(255)      nullable
#  created_at             :datetime         not null
#  updated_at             :datetime         not null
#
# Foreign Keys
#
#  named_steps_dependent_system_uuid_foreign  (dependent_system_uuid => tasker_dependent_systems.dependent_system_uuid)
#
module TaskerCore
  module Database
    module Models
      class NamedStep < ApplicationRecord
        self.primary_key = :named_step_uuid
        belongs_to :dependent_system, foreign_key: :dependent_system_uuid, primary_key: :dependent_system_uuid
        has_many :workflow_steps, foreign_key: :named_step_uuid, primary_key: :named_step_uuid, dependent: :nullify
        validates :name, presence: true, uniqueness: { scope: :dependent_system_uuid }

        def self.create_named_steps_from_templates(templates)
          templates.map do |template|
            dependent_system = TaskerCore::Database::Models::DependentSystem.find_or_create_by!(name: template.dependent_system)
            named_step = find_or_create_by!(name: template.name,
                                            dependent_system_uuid: dependent_system.dependent_system_uuid)
            named_step
          end
        end
      end
    end
  end
end
