# typed: false
# frozen_string_literal: true

# == Schema Information
#
# Table name: tasker_task_namespaces
#
#  task_namespace_uuid :uuid             not null, primary key, default(uuid_generate_v7())
#  name                :string(64)       not null
#  description         :string(255)      nullable
#  created_at          :datetime         not null
#  updated_at          :datetime         not null
#
module TaskerCore
  module Database
    module Models
      class TaskNamespace < ApplicationRecord
        self.primary_key = :task_namespace_uuid

        has_many :named_tasks, foreign_key: :task_namespace_uuid, primary_key: :task_namespace_uuid, dependent: :nullify

        validates :name, presence: true, uniqueness: true, length: { maximum: 64 }
        validates :description, length: { maximum: 255 }

        # Find or create default namespace - always works even if not seeded
        def self.default
          find_or_create_by!(name: 'default')
        end

        # Scope for non-default namespaces
        scope :custom, -> { where.not(name: 'default') }

        # Check if this is the default namespace
        def default?
          name == 'default'
        end
      end
    end
  end
end
