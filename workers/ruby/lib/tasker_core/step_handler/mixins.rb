# frozen_string_literal: true

# TAS-112: Composition Pattern - Mixin Modules
#
# This file exports all step handler mixins. Mixins follow the composition-over-inheritance
# pattern where specialized functionality is added via `include` rather than subclassing.
#
# ## Usage
#
# ```ruby
# class MyApiHandler < TaskerCore::StepHandler::Base
#   include TaskerCore::StepHandler::Mixins::API
#
#   def call(context)
#     response = get('/users')
#     success(result: response.body)
#   end
# end
#
# class MyDecisionHandler < TaskerCore::StepHandler::Base
#   include TaskerCore::StepHandler::Mixins::Decision
#
#   def call(context)
#     decision_success(steps: ['next_step'])
#   end
# end
#
# class MyBatchHandler < TaskerCore::StepHandler::Base
#   include TaskerCore::StepHandler::Mixins::Batchable
#
#   def call(context)
#     configs = create_cursor_configs(1000, 5)
#     create_batches_outcome(worker_template_name: 'worker', cursor_configs: configs, total_items: 1000)
#   end
# end
# ```
#
# ## Combining Mixins
#
# Mixins can be combined when a handler needs multiple capabilities:
#
# ```ruby
# class ApiBatchHandler < TaskerCore::StepHandler::Base
#   include TaskerCore::StepHandler::Mixins::API
#   include TaskerCore::StepHandler::Mixins::Batchable
#
#   def call(context)
#     # Can use both API and Batchable helpers
#   end
# end
# ```

require_relative 'mixins/api'
require_relative 'mixins/decision'
require_relative 'mixins/batchable'

module TaskerCore
  module StepHandler
    module Mixins
      # Re-export for convenient access
      # API = TaskerCore::StepHandler::Mixins::API
      # Decision = TaskerCore::StepHandler::Mixins::Decision
      # Batchable = TaskerCore::StepHandler::Mixins::Batchable
    end
  end
end
