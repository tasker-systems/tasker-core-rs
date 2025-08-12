# typed: false
# frozen_string_literal: true

# == Schema Information
#
# Table name: task_annotations
#
#  annotation         :jsonb
#  created_at         :datetime         not null
#  updated_at         :datetime         not null
#  annotation_type_id :integer          not null
#  task_annotation_id :bigint           not null, primary key
#  task_uuid            :bigint           not null
#
# Indexes
#
#  task_annotations_annotation_idx            (annotation) USING gin
#  task_annotations_annotation_idx1           (annotation) USING gin
#  task_annotations_annotation_type_id_index  (annotation_type_id)
#  task_annotations_task_uuid_index             (task_uuid)
#
# Foreign Keys
#
#  task_annotations_annotation_type_id_foreign  (annotation_type_id => annotation_types.annotation_type_id)
#  task_annotations_task_uuid_foreign             (task_uuid => tasks.task_uuid)
#

module TaskerCore
  module Database
    module Models
      class TaskAnnotation < ApplicationRecord
        self.primary_key = :task_annotation_id
        belongs_to :task, foreign_key: :task_uuid, primary_key: :task_uuid
        belongs_to :annotation_type

        delegate :name, to: :annotation_type, prefix: true
      end
    end
  end
end
