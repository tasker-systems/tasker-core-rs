# typed: false
# frozen_string_literal: true

# == Schema Information
#
# Table name: dependent_systems
#
#  description         :string(255)
#  name                :string(64)       not null
#  created_at          :datetime         not null
#  updated_at          :datetime         not null
#  dependent_system_id :integer          not null, primary key
#
# Indexes
#
#  dependent_systems_name_index   (name)
#  dependent_systems_name_unique  (name) UNIQUE
#
module TaskerCore::Database::Models
  class DependentSystem < ApplicationRecord
    self.primary_key =  :dependent_system_id
    has_many :dependent_system_object_maps, dependent: :destroy
    has_many :named_steps, dependent: :destroy
    validates :name, presence: true, uniqueness: true
  end
end
