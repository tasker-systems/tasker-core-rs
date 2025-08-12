# typed: false
# frozen_string_literal: true

# == Schema Information
#
# Table name: tasker_dependent_systems
#
#  dependent_system_uuid :uuid             not null, primary key, default(uuid_generate_v7())
#  name                  :string(64)       not null
#  description           :string(255)      nullable
#  created_at            :datetime         not null
#  updated_at            :datetime         not null
#
module TaskerCore
  module Database
    module Models
      class DependentSystem < ApplicationRecord
        self.primary_key = :dependent_system_uuid
        has_many :dependent_system_object_maps, foreign_key: :dependent_system_uuid, primary_key: :dependent_system_uuid, dependent: :destroy
        has_many :named_steps, foreign_key: :dependent_system_uuid, primary_key: :dependent_system_uuid, dependent: :destroy
        validates :name, presence: true, uniqueness: true
      end
    end
  end
end
