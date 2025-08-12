# typed: false
# frozen_string_literal: true

# == Schema Information
#
# Table name: dependent_system_object_maps
#
#  remote_id_one                  :string(128)      not null
#  remote_id_two                  :string(128)      not null
#  created_at                     :datetime         not null
#  updated_at                     :datetime         not null
#  dependent_system_object_map_id :bigint           not null, primary key
#  dependent_system_one_id        :integer          not null
#  dependent_system_two_id        :integer          not null
#
# Indexes
#
#  dependent_system_object_maps_dependent_system_one_id_dependent_  (dependent_system_one_id,dependent_system_two_id,remote_id_one,remote_id_two) UNIQUE
#  dependent_system_object_maps_dependent_system_one_id_index       (dependent_system_one_id)
#  dependent_system_object_maps_dependent_system_two_id_index       (dependent_system_two_id)
#  dependent_system_object_maps_remote_id_one_index                 (remote_id_one)
#  dependent_system_object_maps_remote_id_two_index                 (remote_id_two)
#
# Foreign Keys
#
#  dependent_system_object_maps_dependent_system_one_id_foreign  (dependent_system_one_id => dependent_systems.dependent_system_id)
#  dependent_system_object_maps_dependent_system_two_id_foreign  (dependent_system_two_id => dependent_systems.dependent_system_id)
#

module TaskerCore
  module Database
    module Models
      class DependentSystemObjectMap < ApplicationRecord
        self.primary_key = :dependent_system_object_map_id
        belongs_to  :dependent_system_one,
                    class_name: 'TaskerCore::Database::Models::DependentSystem',
                    foreign_key: :dependent_system_one_uuid,
                    primary_key: :dependent_system_uuid
        belongs_to  :dependent_system_two,
                    class_name: 'TaskerCore::Database::Models::DependentSystem',
                    foreign_key: :dependent_system_two_uuid,
                    primary_key: :dependent_system_uuid
        validates :remote_id_one, presence: true
        validates :remote_id_two, presence: true

        def self.find_or_create(system_one_name, system_one_id, system_two_name, system_two_id)
          system_one = TaskerCore::Database::Models::DependentSystem.find_or_create_by!(name: system_one_name)
          system_two = TaskerCore::Database::Models::DependentSystem.find_or_create_by!(name: system_two_name)
          # these could be in either order
          inst = where(
            remote_id_one: system_one_id,
            remote_id_two: system_two_id,
            dependent_system_one_uuid: system_one.dependent_system_uuid,
            dependent_system_two_uuid: system_two.dependent_system_uuid
          ).or(
            where(
              remote_id_one: system_two_id,
              remote_id_two: system_one_id,
              dependent_system_one_uuid: system_two.dependent_system_uuid,
              dependent_system_two_uuid: system_one.dependent_system_uuid
            )
          ).first
          inst ||= create(
            remote_id_one: system_one_id,
            remote_id_two: system_two_id,
            dependent_system_one: system_one,
            dependent_system_two: system_two
          )
          inst
        end
      end
    end
  end
end
