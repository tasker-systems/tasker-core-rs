# typed: false
# frozen_string_literal: true

module TaskerCore
  module Database
    module Models
      class WorkflowStep < ApplicationRecord
        PROVIDES_EDGE_NAME = 'provides'

        self.primary_key = :workflow_step_uuid
        after_initialize :init_defaults, if: :new_record?
        belongs_to :task, foreign_key: :task_uuid, primary_key: :task_uuid
        belongs_to :named_step, foreign_key: :named_step_uuid, primary_key: :named_step_uuid
        has_many  :incoming_edges,
                  class_name: 'WorkflowStepEdge',
                  foreign_key: :to_step_uuid,
                  primary_key: :workflow_step_uuid,
                  dependent: :destroy,
                  inverse_of: :to_step
        has_many  :outgoing_edges,
                  class_name: 'WorkflowStepEdge',
                  foreign_key: :from_step_uuid,
                  primary_key: :workflow_step_uuid,
                  dependent: :destroy,
                  inverse_of: :from_step
        has_many :parents, through: :incoming_edges, source: :from_step
        has_many :children, through: :outgoing_edges, source: :to_step
        has_many :siblings, through: :outgoing_edges, source: :from_step
        has_many :workflow_step_transitions, foreign_key: :workflow_step_uuid, primary_key: :workflow_step_uuid,
                                             inverse_of: :workflow_step, dependent: :destroy

        validates :named_step_uuid, uniqueness: { scope: :task_uuid, message: 'must be unique within the same task' }
        validates :workflow_step_uuid, presence: true, uniqueness: true
        validate :name_uniqueness_within_task

        delegate :name, to: :named_step

        has_one :step_dag_relationship, class_name: 'TaskerCore::StepDagRelationship', primary_key: :workflow_step_uuid
        # NOTE: step_readiness_status is now accessed via function-based approach, not ActiveRecord association

        # Optimized scopes for efficient querying using state machine transitions
        scope :completed, lambda {
          joins(:workflow_step_transitions)
            .where(
              workflow_step_transitions: {
                most_recent: true,
                to_state: [
                  Constants::WorkflowStepStatuses::COMPLETE,
                  Constants::WorkflowStepStatuses::RESOLVED_MANUALLY
                ]
              }
            )
        }

        scope :failed, lambda {
          joins(:workflow_step_transitions)
            .where(
              workflow_step_transitions: {
                most_recent: true,
                to_state: Constants::WorkflowStepStatuses::ERROR
              }
            )
        }

        scope :pending, lambda {
          # Include steps with no transitions (initial state) AND steps with pending/in_progress transitions
          where.missing(:workflow_step_transitions)
               .or(
                 joins(:workflow_step_transitions)
                   .where(
                     workflow_step_transitions: {
                       most_recent: true,
                       to_state: [
                         Constants::WorkflowStepStatuses::PENDING,
                         Constants::WorkflowStepStatuses::IN_PROGRESS
                       ]
                     }
                   )
               )
        }

        scope :for_task, lambda { |task|
          where(task_uuid: task.task_uuid)
        }

        # Scopes workflow steps by their current state using state machine transitions
        #
        # @scope class
        # @param state [String, nil] The state to filter by. If nil, returns all steps with current state information
        # @return [ActiveRecord::Relation] Steps with current state, optionally filtered by specific state
        scope :by_current_state, lambda { |state = nil|
          relation = joins(<<-SQL.squish)
        INNER JOIN (
          SELECT DISTINCT ON (workflow_step_uuid) workflow_step_uuid, to_state
          FROM tasker_workflow_step_transitions
          WHERE most_recent = true
          ORDER BY workflow_step_uuid, sort_key DESC
        ) current_transitions ON current_transitions.workflow_step_uuid = tasker_workflow_steps.workflow_step_uuid
          SQL

          if state.present?
            relation.where(current_transitions: { to_state: state })
          else
            relation
          end
        }

        # Scopes workflow steps completed since a specific time
        #
        # @scope class
        # @param since_time [Time] The earliest completion time to include
        # @return [ActiveRecord::Relation] Steps completed since the specified time
        scope :completed_since, lambda { |since_time|
          joins(:workflow_step_transitions)
            .where('tasker_workflow_step_transitions.most_recent = ? AND tasker_workflow_step_transitions.to_state = ?', true, 'complete')
            .where('tasker_workflow_step_transitions.created_at > ?', since_time)
        }

        # Scopes workflow steps that failed since a specific time
        #
        # @scope class
        # @param since_time [Time] The earliest failure time to include
        # @return [ActiveRecord::Relation] Steps that failed since the specified time
        scope :failed_since, lambda { |since_time|
          joins(:workflow_step_transitions)
            .where('tasker_workflow_step_transitions.most_recent = ? AND tasker_workflow_step_transitions.to_state = ?', true, 'error')
            .where('tasker_workflow_step_transitions.created_at > ?', since_time)
        }

        # Scopes workflow steps for tasks created since a specific time
        #
        # @scope class
        # @param since_time [Time] The earliest task creation time to include
        # @return [ActiveRecord::Relation] Steps for tasks created since the specified time
        scope :for_tasks_since, lambda { |since_time|
          joins(:task).where('tasker_tasks.created_at > ?', since_time)
        }

        # Efficient method to get task completion statistics using ActiveRecord scopes
        # This avoids the N+1 query problem while working with the state machine system
        #
        # @param task [Task] The task to analyze
        # @return [Hash] Hash with completion statistics and latest completion time
        def self.task_completion_stats(task)
          # Use efficient ActiveRecord queries with the state machine
          task_steps = for_task(task)

          # Get completion statistics with optimized queries
          total_steps = task_steps.count
          completed_steps = task_steps.completed
          failed_steps = task_steps.failed

          # Calculate counts
          completed_count = completed_steps.count
          failed_count = failed_steps.count

          # For pending count, calculate as total minus completed and failed
          # This handles the case where new steps don't have transitions yet
          pending_count = total_steps - completed_count - failed_count

          # Get latest completion time from completed steps
          latest_completion_time = completed_steps.maximum(:processed_at)

          {
            total_steps: total_steps,
            completed_steps: completed_count,
            failed_steps: failed_count,
            pending_steps: pending_count,
            latest_completion_time: latest_completion_time,
            all_complete: completed_count == total_steps && total_steps.positive?
          }
        end

        # Finds a WorkflowStep with the given name by traversing the DAG efficiently
        # @param steps [Array<WorkflowStep>] Collection of steps to search through
        # @param name [String] Name of the step to find
        # @return [WorkflowStep, nil] The first matching step found or nil if none exists
        def self.find_step_by_name(steps, name)
          StepFinder.find_by_name(steps, name)
        end

        # State machine integration
        def state_machine
          @state_machine ||= TaskerCore::StateMachine::StepStateMachine.new(
            self,
            transition_class: TaskerCore::Database::Models::WorkflowStepTransition,
            association_name: :workflow_step_transitions
          )
        end

        # Status is now entirely managed by the state machine
        def status
          if new_record?
            # For new records, return the initial state
            TaskerCore::Constants::WorkflowStepStatuses::PENDING
          else
            # For persisted records, use state machine
            state_machine.current_state
          end
        end

        def step_uuid
          workflow_step_uuid
        end

        # Service class to find steps by name
        # Reduces complexity by organizing step search logic
        class StepFinder
          class << self
            # Find step by name in provided collection or task hierarchy
            #
            # @param steps [Array<WorkflowStep>] Collection of steps to search through
            # @param name [String] Name of the step to find
            # @return [WorkflowStep, nil] The first matching step found or nil if none exists
            def find_by_name(steps, name)
              return nil if steps.empty? || name.nil?

              # First check direct match in provided steps
              direct_match = find_direct_match(steps, name)
              return direct_match if direct_match

              # Fall back to task-wide search using DAG relationships
              find_in_task_hierarchy(steps, name)
            end

            private

            # Find direct match in provided steps
            #
            # @param steps [Array<WorkflowStep>] Collection of steps
            # @param name [String] Name to search for
            # @return [WorkflowStep, nil] Matching step or nil
            def find_direct_match(steps, name)
              steps.find { |step| step.name == name }
            end

            # Find step in task hierarchy using efficient DAG traversal
            #
            # @param steps [Array<WorkflowStep>] Collection of steps to get task context
            # @param name [String] Name to search for
            # @return [WorkflowStep, nil] Matching step or nil
            def find_in_task_hierarchy(steps, name)
              task_uuids = extract_task_uuids(steps)

              task_uuids.each do |task_uuid|
                found_step = find_in_single_task(task_uuid, name)
                return found_step if found_step
              end

              nil
            end

            # Extract unique task IDs from steps
            #
            # @param steps [Array<WorkflowStep>] Collection of steps
            # @return [Array<Integer>] Unique task IDs
            def extract_task_uuids(steps)
              steps.map(&:task_uuid).uniq
            end

            # Find step by name in a single task
            #
            # @param task_uuid [UUID] Task UUID to search in
            # @param name [String] Name to search for
            # @return [WorkflowStep, nil] Matching step or nil
            def find_in_single_task(task_uuid, name)
              # Get all workflow steps for this task with their DAG relationships
              all_task_steps = WorkflowStep.joins(:named_step)
                                           .includes(:step_dag_relationship)
                                           .where(task_uuid: task_uuid)

              # Find step by name using simple lookup instead of recursive traversal
              all_task_steps.joins(:named_step)
                            .find_by(named_steps: { name: name })
            end
          end
        end

        def self.get_steps_for_task(task, templates)
          named_steps = NamedStep.create_named_steps_from_templates(templates)
          steps =
            templates.map do |template|
              named_step = named_steps.find { |ns| template.name == ns.name }
              NamedTasksNamedStep.associate_named_step_with_named_task(task.named_task, template, named_step)
              step = where(task_uuid: task.task_uuid, named_step_uuid: named_step.named_step_uuid).first
              step ||= build_default_step!(task, template, named_step)
              step
            end
          set_up_dependent_steps(steps, templates)
        end

        def self.set_up_dependent_steps(steps, templates)
          templates.each do |template|
            next if template.all_dependencies.empty?

            dependent_step = steps.find { |step| step.name == template.name }
            template.all_dependencies.each do |dependency|
              provider_step = steps.find { |step| step.name == dependency }
              unless provider_step.outgoing_edges.exists?(to_step: dependent_step)
                provider_step.add_provides_edge!(dependent_step)
              end
            end
          end
          steps
        end

        def self.build_default_step!(task, template, named_step)
          # Create the step first without status
          step_attributes = {
            task_uuid: task.task_uuid,
            named_step_uuid: named_step.named_step_uuid,
            retryable: template.default_retryable,
            retry_limit: template.default_retry_limit,
            skippable: template.skippable,
            in_process: false,
            inputs: task.context,
            processed: false,
            attempts: 0,
            results: {}
          }

          step = new(step_attributes)
          step.save!

          # REMOVED: Automatic state machine initialization to prevent duplicate key violations
          # The state machine will initialize naturally when accessed, and factories may
          # have already created transitions through their own setup
          # step.state_machine.initialize_state_machine!

          step
        end

        def self.get_viable_steps(task, sequence)
          # Get step UUIDs from sequence
          step_uuids = sequence.steps.map(&:workflow_step_uuid)

          # Use function-based approach for high-performance readiness checking
          ready_statuses = StepReadinessStatus.for_task(task.task_uuid, step_uuids)
          ready_step_uuids = ready_statuses.select(&:ready_for_execution).map(&:workflow_step_uuid)

          # Return WorkflowStep objects for ready steps
          WorkflowStep.where(workflow_step_uuid: ready_step_uuids)
                      .includes(:named_step)
        end

        def add_provides_edge!(to_step)
          outgoing_edges.create!(to_step: to_step, name: PROVIDES_EDGE_NAME)
        end

        # Helper method to get step readiness status using function-based approach
        def step_readiness_status
          @step_readiness_status ||= StepReadinessStatus.for_task(task_uuid, [workflow_step_uuid]).first
        end

        def complete?
          # Use function-based approach for consistent state checking
          step_readiness_status&.current_state&.in?([
                                                      Constants::WorkflowStepStatuses::COMPLETE,
                                                      Constants::WorkflowStepStatuses::RESOLVED_MANUALLY
                                                    ]) || false
        end

        def in_progress?
          # Use function-based approach for consistent state checking
          step_readiness_status&.current_state == Constants::WorkflowStepStatuses::IN_PROGRESS
        end

        def pending?
          # Use function-based approach for consistent state checking
          step_readiness_status&.current_state == Constants::WorkflowStepStatuses::PENDING
        end

        def in_error?
          # Use function-based approach for consistent state checking
          step_readiness_status&.current_state == Constants::WorkflowStepStatuses::ERROR
        end

        def cancelled?
          # Use function-based approach for consistent state checking
          step_readiness_status&.current_state == Constants::WorkflowStepStatuses::CANCELLED
        end

        def ready_status?
          # Use function-based approach for efficient ready status checking
          Constants::UNREADY_WORKFLOW_STEP_STATUSES.exclude?(
            step_readiness_status&.current_state || Constants::WorkflowStepStatuses::PENDING
          )
        end

        def ready?
          # Use function-based approach's comprehensive readiness calculation
          step_readiness_status&.ready_for_execution || false
        end

        # Function-based predicate methods
        def dependencies_satisfied?
          # Use function-based approach's pre-calculated dependency analysis
          step_readiness_status&.dependencies_satisfied || false
        end

        def retry_eligible?
          # Use function-based approach's retry/backoff calculation
          step_readiness_status&.retry_eligible || false
        end

        def has_retry_attempts?
          # Check if step has made retry attempts
          (step_readiness_status&.attempts || 0).positive?
        end

        def retry_exhausted?
          # Check if step has exhausted retry attempts
          return false unless step_readiness_status

          attempts = step_readiness_status.attempts || 0
          retry_limit = step_readiness_status.retry_limit || 3
          attempts >= retry_limit
        end

        def waiting_for_backoff?
          # Check if step is waiting for backoff period to expire
          return false unless step_readiness_status&.next_retry_at

          step_readiness_status.next_retry_at > Time.current
        end

        def can_retry_now?
          # Comprehensive check if step can be retried right now
          return false unless in_error?
          return false unless retry_eligible?
          return false if waiting_for_backoff?

          true
        end

        def root_step?
          # Check if this is a root step (no dependencies)
          (step_readiness_status&.total_parents || 0).zero?
        end

        def leaf_step?
          # Check if this is a leaf step using DAG relationship view
          step_dag_relationship&.child_count&.zero?
        end

        def reload
          # Override reload to ensure step readiness status is refreshed
          super.tap do
            @step_readiness_status = nil # Reset cached readiness status
          end
        end

        # Ensures step_uuid is set with UUID v7
        #
        # @return [String] The step UUID
        def ensure_step_uuid
          return workflow_step_uuid if workflow_step_uuid.present?

          self.workflow_step_uuid = SecureRandom.uuid_v7
        end

        private

        # Initializes default values for a new workflow step
        #
        # @return [void]
        def init_defaults
          return unless new_record?

          # Ensure workflow_step_uuid is set using UUID v7
          ensure_step_uuid
        end

        # Custom validation to ensure step names are unique within a task
        def name_uniqueness_within_task
          return unless named_step && task

          # Find all steps within the same task that have the same name
          matching_steps = self.class.where(task_uuid: task_uuid)
                               .joins(:named_step)
                               .where(named_step: { name: name })
                               .where.not(workflow_step_uuid: workflow_step_uuid) # Exclude self when updating

          errors.add(:base, "Step name '#{name}' must be unique within the same task") if matching_steps.exists?
        end
      end
    end
  end
end
