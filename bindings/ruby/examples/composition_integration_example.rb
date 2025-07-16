#!/usr/bin/env ruby
# frozen_string_literal: true

#
# Comprehensive Integration Example: Ruby Step Handler with Composition System
#
# This example demonstrates how to create Ruby step handlers that work
# with the new composition-based architecture where:
# - Rust provides the orchestration foundation
# - Ruby provides business logic through proper inheritance
# - TaskHandlerRegistry manages handler discovery
# - Configuration is loaded from YAML files
#

require 'json'
require 'yaml'
require 'logger'

# Assuming the gem is loaded, we'd normally do:
# require 'tasker_core'
# For this example, we'll simulate the required classes

# Load the TaskerCore Ruby library
require_relative '../lib/tasker_core'

#
# Step 1: Define our business logic step handler
#
class PaymentProcessingStepHandler < TaskerCore::StepHandler::Base
  def initialize(config: {}, logger: nil)
    @config = config || {}
    @logger = logger || Logger.new($stdout)
    super
  end

  # Main business logic method - Rails engine compatible signature
  # @param task [RubyTask] Task model instance with context data
  # @param sequence [RubyStepSequence] Step sequence for navigation
  # @param step [RubyStep] Current step being processed
  # @return [Hash] Step results
  def process(task, sequence, step)
    @logger.info "Processing payment step: #{step.workflow_step_id}"
    @logger.info "Task ID: #{task.task_id}"
    @logger.info "Step sequence position: #{sequence.current_position}/#{sequence.total_steps}"

    # Extract inputs from step
    inputs = step.inputs || {}
    amount = inputs['amount'] || 0
    currency = inputs['currency'] || 'USD'
    payment_method = inputs['payment_method'] || 'credit_card'

    @logger.info "Processing payment: #{amount} #{currency} via #{payment_method}"

    # Simulate payment processing based on amount
    if amount <= 0
      raise TaskerCore::PermanentError.new(
        "Invalid payment amount: #{amount}",
        error_code: 'INVALID_AMOUNT',
        error_category: 'validation'
      )
    elsif amount > 10_000
      raise TaskerCore::RetryableError.new(
        'Payment amount exceeds limit, requires approval',
        error_category: 'external_service',
        retry_after: 300 # 5 minutes
      )
    else
      # Successful payment processing
      {
        status: 'completed',
        payment_id: "pay_#{Time.now.to_i}_#{rand(1000)}",
        amount_processed: amount,
        currency: currency,
        payment_method: payment_method,
        processed_at: Time.now.iso8601,
        processor: 'example_payment_gateway',
        transaction_fee: (amount * 0.029).round(2)
      }
    end
  end

  # Optional result transformation method
  # @param step [RubyStep] Current step being processed
  # @param process_output [Hash] Result from process() method
  # @param initial_results [Hash] Previous results if this is a retry
  # @return [Hash] Transformed result
  def process_results(step, process_output, initial_results = nil)
    @logger.info "Processing results for step: #{step.workflow_step_id}"

    # Add audit trail and metadata
    enhanced_results = process_output.merge({
                                              step_id: step.workflow_step_id,
                                              handler: self.class.name,
                                              processed_at: Time.now.iso8601,
                                              retry_count: step.attempts || 0,
                                              previous_results: initial_results,
                                              config_version: @config['version'] || '1.0.0'
                                            })

    # Log success or failure
    if process_output[:status] == 'completed'
      @logger.info "Payment processed successfully: #{process_output[:payment_id]}"
    else
      @logger.error "Payment processing failed: #{process_output[:error_message]}"
    end

    enhanced_results
  end
end

#
# Step 2: Define a notification step handler
#
class NotificationStepHandler < TaskerCore::StepHandler::Base
  def initialize(config: {}, logger: nil)
    @config = config || {}
    @logger = logger || Logger.new($stdout)
    super
  end

  def process(task, _sequence, step)
    @logger.info "Processing notification step: #{step.workflow_step_id}"

    # Extract inputs from step
    inputs = step.inputs || {}
    recipient = inputs['recipient'] || 'user@example.com'
    message_type = inputs['message_type'] || 'email'
    template = inputs['template'] || 'default'

    @logger.info "Sending #{message_type} notification to #{recipient} using #{template} template"

    # Simulate notification sending
    case message_type
    when 'email'
      send_email_notification(recipient, template, task, step)
    when 'sms'
      send_sms_notification(recipient, template, task, step)
    when 'push'
      send_push_notification(recipient, template, task, step)
    else
      raise TaskerCore::PermanentError.new(
        "Unsupported notification type: #{message_type}",
        error_code: 'UNSUPPORTED_TYPE',
        error_category: 'validation'
      )
    end
  end

  private

  def send_email_notification(recipient, template, _task, _step)
    # Simulate email sending
    notification_id = "email_#{Time.now.to_i}_#{rand(1000)}"

    {
      status: 'completed',
      notification_id: notification_id,
      recipient: recipient,
      message_type: 'email',
      template: template,
      sent_at: Time.now.iso8601,
      provider: 'example_email_service'
    }
  end

  def send_sms_notification(recipient, template, _task, _step)
    # Simulate SMS sending with potential failure
    if rand(100) < 5 # 5% failure rate
      raise TaskerCore::RetryableError.new(
        'SMS service temporarily unavailable',
        error_category: 'external_service',
        retry_after: 60
      )
    end

    notification_id = "sms_#{Time.now.to_i}_#{rand(1000)}"

    {
      status: 'completed',
      notification_id: notification_id,
      recipient: recipient,
      message_type: 'sms',
      template: template,
      sent_at: Time.now.iso8601,
      provider: 'example_sms_service'
    }
  end

  def send_push_notification(recipient, template, _task, _step)
    # Simulate push notification
    notification_id = "push_#{Time.now.to_i}_#{rand(1000)}"

    {
      status: 'completed',
      notification_id: notification_id,
      recipient: recipient,
      message_type: 'push',
      template: template,
      sent_at: Time.now.iso8601,
      provider: 'example_push_service'
    }
  end
end

#
# Step 3: Integration example that demonstrates the composition system
#
class CompositionIntegrationExample
  def initialize
    @logger = Logger.new($stdout)
    @logger.level = Logger::INFO
  end

  def run
    @logger.info '=== Composition Integration Example ==='

    # Step 1: Create and register step handlers
    @logger.info 'Step 1: Creating step handlers'

    payment_handler = PaymentProcessingStepHandler.new(
      config: { 'version' => '1.0.0', 'timeout' => 30 },
      logger: @logger
    )

    notification_handler = NotificationStepHandler.new(
      config: { 'version' => '1.0.0', 'timeout' => 15 },
      logger: @logger
    )

    # Step 2: Demonstrate handler registration
    @logger.info 'Step 2: Registering handlers with orchestration system'

    # The handlers automatically register themselves in their initialize method
    # Let's check if they're registered
    registered_handlers = TaskerCore::StepHandler::Base.list_registered_handlers
    @logger.info "Registered handlers: #{registered_handlers['count']} total"

    # Step 3: Create test data using the factory system
    @logger.info 'Step 3: Creating test data with factories'

    # Create foundation data (namespace, named_task, named_step)
    foundation_result = TaskerCore::TestHelpers.create_test_foundation_with_factory({
                                                                                      namespace: 'payment_processing',
                                                                                      task_name: 'payment_workflow',
                                                                                      step_name: 'payment_processing'
                                                                                    })

    if foundation_result['error']
      @logger.error "Failed to create foundation data: #{foundation_result['error']}"
      return
    end

    @logger.info 'Foundation data created successfully'

    # Create a task
    task_result = TaskerCore::TestHelpers.create_test_task_with_factory({
                                                                          context: {
                                                                            customer_id: 'cust_123',
                                                                            order_id: 'ord_456',
                                                                            payment_required: true
                                                                          },
                                                                          tags: {
                                                                            environment: 'development',
                                                                            integration_test: true
                                                                          }
                                                                        })

    if task_result['error']
      @logger.error "Failed to create task: #{task_result['error']}"
      return
    end

    @logger.info "Task created: #{task_result['task_id']}"

    # Create workflow steps
    payment_step_result = TaskerCore::TestHelpers.create_test_workflow_step_with_factory({
                                                                                           task_id: task_result['task_id'],
                                                                                           inputs: {
                                                                                             amount: 99.99,
                                                                                             currency: 'USD',
                                                                                             payment_method: 'credit_card'
                                                                                           }
                                                                                         })

    if payment_step_result['error']
      @logger.error "Failed to create payment step: #{payment_step_result['error']}"
      return
    end

    @logger.info "Payment step created: #{payment_step_result['workflow_step_id']}"

    notification_step_result = TaskerCore::TestHelpers.create_test_workflow_step_with_factory({
                                                                                                task_id: task_result['task_id'],
                                                                                                inputs: {
                                                                                                  recipient: 'customer@example.com',
                                                                                                  message_type: 'email',
                                                                                                  template: 'payment_confirmation'
                                                                                                }
                                                                                              })

    if notification_step_result['error']
      @logger.error "Failed to create notification step: #{notification_step_result['error']}"
      return
    end

    @logger.info "Notification step created: #{notification_step_result['workflow_step_id']}"

    # Step 4: Demonstrate step handler execution
    @logger.info 'Step 4: Demonstrating step handler execution'

    # Create mock task, sequence, and step objects for testing
    mock_task = OpenStruct.new(
      task_id: task_result['task_id'],
      context: task_result['context'],
      tags: task_result['tags']
    )

    mock_payment_step = OpenStruct.new(
      workflow_step_id: payment_step_result['workflow_step_id'],
      inputs: payment_step_result['inputs'],
      attempts: 0
    )

    mock_sequence = OpenStruct.new(
      total_steps: 2,
      current_position: 1
    )

    # Execute payment step
    @logger.info 'Executing payment step handler'
    begin
      payment_result = payment_handler.process(mock_task, mock_sequence, mock_payment_step)
      @logger.info "Payment step result: #{payment_result}"

      # Process results
      final_payment_result = payment_handler.process_results(
        mock_payment_step,
        payment_result,
        nil
      )
      @logger.info "Final payment result: #{final_payment_result}"
    rescue StandardError => e
      @logger.error "Payment step failed: #{e.message}"
      @logger.error "Error class: #{e.class.name}"
    end

    # Execute notification step
    @logger.info 'Executing notification step handler'
    mock_notification_step = OpenStruct.new(
      workflow_step_id: notification_step_result['workflow_step_id'],
      inputs: notification_step_result['inputs'],
      attempts: 0
    )

    mock_sequence.current_position = 2

    begin
      notification_result = notification_handler.process(mock_task, mock_sequence, mock_notification_step)
      @logger.info "Notification step result: #{notification_result}"

      # Process results
      final_notification_result = notification_handler.process_results(
        mock_notification_step,
        notification_result,
        nil
      )
      @logger.info "Final notification result: #{final_notification_result}"
    rescue StandardError => e
      @logger.error "Notification step failed: #{e.message}"
      @logger.error "Error class: #{e.class.name}"
    end

    # Step 5: Demonstrate configuration and metadata
    @logger.info 'Step 5: Handler configuration and metadata'

    @logger.info "Payment handler metadata: #{payment_handler.metadata}"
    @logger.info "Payment handler capabilities: #{payment_handler.capabilities}"

    @logger.info "Notification handler metadata: #{notification_handler.metadata}"
    @logger.info "Notification handler capabilities: #{notification_handler.capabilities}"

    # Step 6: Cleanup
    @logger.info 'Step 6: Cleaning up test data'
    cleanup_result = TaskerCore::TestHelpers.cleanup_test_database
    @logger.info "Cleanup result: #{cleanup_result['status']}"

    @logger.info '=== Integration Example Complete ==='
  end
end

#
# Step 4: Run the example
#
if __FILE__ == $PROGRAM_NAME
  example = CompositionIntegrationExample.new
  example.run
end
