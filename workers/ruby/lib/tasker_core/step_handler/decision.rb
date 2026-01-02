# frozen_string_literal: true

require_relative 'base'
require_relative 'mixins'
require_relative '../types/decision_point_outcome'
require_relative '../types/step_handler_call_result'

module TaskerCore
  module StepHandler
    # Decision step handler for TAS-53 Dynamic Workflow Decision Points
    #
    # ## TAS-112: Composition Pattern (DEPRECATED CLASS)
    #
    # This class is provided for backward compatibility. For new code, use the mixin pattern:
    #
    # ```ruby
    # class MyDecisionHandler < TaskerCore::StepHandler::Base
    #   include TaskerCore::StepHandler::Mixins::Decision
    #
    #   def call(context)
    #     amount = context.get_task_field('amount')
    #
    #     if amount < 1000
    #       decision_success(
    #         steps: ['auto_approve'],
    #         result_data: { route_type: 'auto', amount: amount }
    #       )
    #     else
    #       decision_success(
    #         steps: ['manager_approval', 'finance_review'],
    #         result_data: { route_type: 'dual', amount: amount }
    #       )
    #     end
    #   end
    # end
    # ```
    #
    # ## No-Branch Pattern
    #
    # ```ruby
    # def call(context)
    #   if context.get_task_field('skip_approval')
    #     decision_no_branches(result_data: { reason: 'skipped' })
    #   else
    #     decision_success(steps: ['standard_approval'])
    #   end
    # end
    # ```
    class Decision < Base
      include Mixins::Decision
    end
  end
end
