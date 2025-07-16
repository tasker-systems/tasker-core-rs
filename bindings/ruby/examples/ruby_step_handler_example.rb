# frozen_string_literal: true

# Example of how Ruby developers would implement step handlers
# with the new RubyStepHandler architecture

require 'json'

class PaymentProcessingStepHandler
  # Main business logic method - called by RubyStepHandler
  # @param task [Hash] Task data from database
  # @param sequence [Hash] Step sequence with dependencies
  # @param step [Hash] Current step data
  # @return [Hash] Step execution result
  def process(task, sequence, step)
    puts "Processing step: #{step['workflow_step_id']}"
    puts "Task ID: #{task['task_id']}"
    puts "Step sequence length: #{sequence['total_steps']}"

    # Business logic based on step inputs
    inputs = step['inputs'] || {}
    amount = inputs['amount'] || 0

    # Simulate payment processing
    if amount.positive?
      {
        status: 'success',
        payment_id: "pay_#{Time.now.to_i}",
        amount_processed: amount,
        currency: 'USD'
      }
    else
      {
        status: 'error',
        error_message: "Invalid amount: #{amount}"
      }
    end
  end

  # Optional result transformation method
  # @param step [Hash] Current step data
  # @param process_output [Hash] Result from process() method
  # @param initial_results [Hash] Previous results if retry
  # @return [Hash] Transformed result
  def process_results(step, process_output, _initial_results = nil)
    puts "Processing results for step: #{step['workflow_step_id']}"

    # Transform results if needed
    if process_output[:status] == 'success'
      # Add audit trail
      process_output.merge({
                             processed_at: Time.now.iso8601,
                             step_id: step['workflow_step_id'],
                             retry_count: step['attempts'] || 0
                           })
    else
      # Log error details
      puts "Step failed: #{process_output[:error_message]}"
      process_output
    end
  end
end

# Example usage (this would be called by the orchestration system)
# handler = PaymentProcessingStepHandler.new
#
# # Register with Rust core
# TaskerCore.register_step_handler(
#   handler.method(:process),
#   handler.method(:process_results)
# )
