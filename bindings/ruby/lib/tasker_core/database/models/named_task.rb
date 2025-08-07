# typed: false
# frozen_string_literal: true

# == Schema Information
#
# Table name: tasker_named_tasks
#
#  description           :string(255)
#  name                  :string(64)       not null
#  version               :string(16)       not null
#  configuration         :jsonb
#  created_at            :datetime         not null
#  updated_at            :datetime         not null
#  named_task_id         :integer          not null, primary key
#  task_namespace_id     :integer          not null
#
# Indexes
#
#  named_tasks_name_index                          (name)
#  named_tasks_namespace_name_version_unique       (task_namespace_id,name,version) UNIQUE
#  named_tasks_task_namespace_id_index             (task_namespace_id)
#  named_tasks_version_index                       (version)
#  named_tasks_configuration_gin_index             (configuration) USING gin
#
# Foreign Keys
#
#  fk_rails_...  (task_namespace_id => tasker_task_namespaces.task_namespace_id)
#
module TaskerCore
  module Database
    module Models
      class NamedTask < ApplicationRecord
        self.primary_key = :named_task_id

        belongs_to :task_namespace
        has_many :tasks, dependent: :nullify

        validates :name, presence: true, length: { maximum: 64 }
        validates :version, presence: true, format: {
          with: /\A\d+\.\d+\.\d+\z/,
          message: 'must be in semver format (e.g., 1.0.0)'
        }, length: { maximum: 16 }
        validates :task_namespace_id, uniqueness: { scope: %i[name version] }

        # Default version for new tasks
        DEFAULT_VERSION = '0.1.0'

        # Default namespace name
        DEFAULT_NAMESPACE = 'default'

        # Find or create with namespace and version support
        def self.find_or_create_by_full_name!(name:, namespace_name: DEFAULT_NAMESPACE, version: DEFAULT_VERSION)
          namespace = TaskNamespace.find_or_create_by!(name: namespace_name)
          find_or_create_by!(
            task_namespace: namespace,
            name: name,
            version: version
          )
        end

        # Full qualified name: namespace.name@version
        def full_name
          "#{task_namespace.name}.#{name}@#{version}"
        end

        # Short name without version: namespace.name
        def qualified_name
          "#{task_namespace.name}.#{name}"
        end

        # Configuration accessor with defaults
        def config
          @config ||= (configuration || {}).with_indifferent_access
        end

        # Get concurrency setting from configuration
        def concurrent?
          config.fetch('concurrent', true)
        end

        # Update configuration
        def update_config!(new_config)
          self.configuration = (config || {}).deep_merge(new_config)
          save!
          @config = nil # Clear memoized config
        end

        # Check if this uses the default namespace
        def default_namespace?
          task_namespace.default?
        end

        def namespace_name
          task_namespace.name
        end

        # Scope methods for common queries
        scope :in_namespace, lambda { |namespace|
          joins(:task_namespace).where(task_namespace: { name: namespace })
        }

        scope :with_version, ->(version) { where(version: version) }

        scope :latest_versions, lambda {
          select('DISTINCT ON (task_namespace_id, name) *')
            .order(:task_namespace_id, :name, version: :desc)
        }

        # Find latest version of a named task in a namespace
        def self.find_latest_version(namespace_name, task_name)
          in_namespace(namespace_name)
            .where(name: task_name)
            .order(version: :desc)
            .first
        end

        # Check if a specific version exists
        def self.version_exists?(namespace_name, task_name, version)
          in_namespace(namespace_name)
            .exists?(name: task_name, version: version)
        end
      end
    end
  end
end
