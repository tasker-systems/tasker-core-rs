# frozen_string_literal: true

require_relative 'tasker_core/version'
require 'json'
require 'faraday'
require 'dry-events'
require 'dry-struct'
require 'dry-types'
require 'dry-validation'
require 'concurrent-ruby'
require 'timeout'
require 'dotenv'
require 'active_support'
require 'active_support/core_ext'

# TaskerCore - High-performance workflow orchestration for Ruby
#
# This library provides a Ruby interface to the Rust-powered tasker-core orchestration
# system, enabling developers to build complex, distributed workflows with automatic
# retry logic, dependency management, and event-driven execution.
#
# The system consists of two main layers:
# - **Rust Foundation**: High-performance orchestration, state management, and messaging
# - **Ruby Business Logic**: Step handlers, task handlers, and custom workflow logic
#
# These layers communicate via FFI (Foreign Function Interface) and an event-driven
# architecture, providing the performance of Rust with the expressiveness of Ruby.
#
# @example Getting started with TaskerCore
#   # 1. Compile the Rust extension (first time only)
#   bundle exec rake compile
#
#   # 2. Define a step handler for your business logic
#   class ProcessPaymentHandler < TaskerCore::StepHandler::Base
#     def call(task, sequence, step)
#       # Access task context data
#       amount = task.context['amount']
#       currency = task.context['currency']
#
#       # Your business logic here
#       payment_result = charge_payment(amount, currency)
#
#       # Return results to be stored in step.results
#       { payment_id: payment_result.id, status: 'succeeded' }
#     end
#   end
#
#   # 3. Start the worker to process tasks
#   TaskerCore::Worker::Bootstrap.start!(
#     namespaces: ['payments']
#   )
#
# @example Creating and submitting a task
#   # Build a task request
#   request = TaskerCore::Types::TaskRequest.build_test(
#     namespace: "payments",
#     name: "process_payment",
#     context: { amount: 100.00, currency: "USD" }
#   )
#
#   # Submit to orchestration system
#   TaskerCore::FFI.submit_task(request)
#
# @example Using the API step handler for HTTP calls
#   class FetchUserHandler < TaskerCore::StepHandler::Api
#     def call(task, sequence, step)
#       user_id = task.context['user_id']
#
#       # Automatic error classification and retry logic
#       response = get("/users/#{user_id}")
#
#       response.body # Stored in step.results
#     end
#   end
#
# Architecture Overview:
#
# 1. **Task State Machine**: 12 states managing overall workflow orchestration
#    - Initial: Pending, Initializing
#    - Active: EnqueuingSteps, StepsInProcess, EvaluatingResults
#    - Waiting: WaitingForDependencies, WaitingForRetry, BlockedByFailures
#    - Terminal: Complete, Error, Cancelled, ResolvedManually
#
# 2. **Step State Machine**: 9 states managing individual step execution
#    - Pipeline: Pending, Enqueued, InProgress, EnqueuedForOrchestration
#    - Terminal: Complete, Error, Cancelled, ResolvedManually, WaitingForRetry
#
# 3. **Event-Driven Communication**:
#    - Rust → Ruby: Step execution events via EventPoller
#    - Ruby → Rust: Step completion events via EventBridge
#    - PostgreSQL LISTEN/NOTIFY for real-time coordination
#
# Load Order (Important):
# 1. Rust native extension (tasker_worker_rb) - provides FFI and base classes
# 2. Ruby error classes - error hierarchy for classification
# 3. Ruby models - data wrappers for FFI types
# 4. Ruby handlers - step and task handler base classes
# 5. Event system - EventBridge, EventPoller, Subscriber
# 6. Bootstrap - worker initialization and lifecycle
#
# @see TaskerCore::StepHandler::Base For creating custom step handlers
# @see TaskerCore::StepHandler::Api For HTTP-based step handlers
# @see TaskerCore::Worker::Bootstrap For worker initialization
# @see TaskerCore::Handlers For the public API namespace
# @see TaskerCore::Errors For error classification and retry logic
# @see https://github.com/tasker-systems/tasker-core Documentation and examples
module TaskerCore
end

begin
  Dotenv.load
  # Load the compiled Rust extension first (provides base classes)
  require_relative 'tasker_core/tasker_worker_rb'
rescue LoadError => e
  raise LoadError, <<~MSG

    ❌ Failed to load tasker-worker-rb native extension!

    This usually means the Rust extension hasn't been compiled yet.

    To compile the extension:
      cd #{File.dirname(__FILE__)}/../..
      rake compile

    Or if you're using this gem in a Rails application:
      bundle exec rake tasker_core:compile

    Original error: #{e.message}

  MSG
end

# Load Ruby modules after Rust extension (they depend on Rust base classes)
require_relative 'tasker_core/errors'
require_relative 'tasker_core/logger'
require_relative 'tasker_core/internal'
require_relative 'tasker_core/template_discovery'
require_relative 'tasker_core/test_environment'
require_relative 'tasker_core/types'
require_relative 'tasker_core/models'
require_relative 'tasker_core/handlers'
require_relative 'tasker_core/registry'
require_relative 'tasker_core/subscriber'
require_relative 'tasker_core/event_bridge'
require_relative 'tasker_core/worker/event_poller'
require_relative 'tasker_core/bootstrap'

module TaskerCore
  module Worker
  end
end

# Load test environment components if appropriate (before auto-boot)
TaskerCore::TestEnvironment.load_if_test!
