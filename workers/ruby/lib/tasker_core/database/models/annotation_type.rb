# typed: false
# frozen_string_literal: true

# == Schema Information
#
# Table name: annotation_types
#
#  description        :string(255)
#  name               :string(64)       not null
#  created_at         :datetime         not null
#  updated_at         :datetime         not null
#  annotation_type_id :integer          not null, primary key
#
# Indexes
#
#  annotation_types_name_index   (name)
#  annotation_types_name_unique  (name) UNIQUE
#

module TaskerCore
  module Database
    module Models
      class AnnotationType < ApplicationRecord
        self.primary_key = :annotation_type_id
        has_many :task_annotations, dependent: :destroy
        validates :name, presence: true, uniqueness: true
      end
    end
  end
end
