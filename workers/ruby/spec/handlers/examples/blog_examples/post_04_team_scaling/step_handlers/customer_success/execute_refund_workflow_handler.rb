# frozen_string_literal: true

module CustomerSuccess
  module StepHandlers
    class ExecuteRefundWorkflowHandler < TaskerCore::StepHandler::Base
      def call(task, sequence, _step)
        # Extract and validate inputs
        inputs = extract_and_validate_inputs(task, sequence, _step)

        logger.info "🔄 ExecuteRefundWorkflowHandler: Executing cross-namespace refund workflow - task_uuid=#{task.task_uuid}, target=#{inputs[:namespace]}.#{inputs[:workflow_name]}"

        # This is the key Post 04 pattern: Cross-namespace workflow coordination
        # Customer Success team calls Payments team's workflow via internal task creation
        delegation_result = create_payments_task(inputs)

        logger.info "✅ ExecuteRefundWorkflowHandler: Payments workflow delegated - delegated_task_id=#{delegation_result[:task_id]}"

        # Return standardized StepHandlerCallResult
        TaskerCore::Types::StepHandlerCallResult.success(
          result: {
            task_delegated: true,
            target_namespace: inputs[:namespace],
            target_workflow: inputs[:workflow_name],
            delegated_task_id: delegation_result[:task_id],
            delegated_task_status: delegation_result[:status],
            delegation_timestamp: Time.now.utc.iso8601,
            correlation_id: delegation_result[:correlation_id],
            namespace: 'customer_success'
          },
          metadata: {
            operation: 'execute_refund_workflow',
            execution_hints: {
              target_namespace: inputs[:namespace],
              target_workflow: inputs[:workflow_name],
              delegated_task_id: delegation_result[:task_id],
              correlation_id: delegation_result[:correlation_id]
            },
            http_headers: {
              'X-Target-Namespace' => inputs[:namespace],
              'X-Target-Workflow' => inputs[:workflow_name],
              'X-Delegated-Task-ID' => delegation_result[:task_id],
              'X-Correlation-ID' => delegation_result[:correlation_id]
            },
            input_refs: {
              approval_result: 'sequence.get_manager_approval.result',
              validation_result: 'sequence.validate_refund_request.result',
              policy_check: 'sequence.check_refund_policy.result'
            }
          }
        )
      rescue StandardError => e
        logger.error "❌ ExecuteRefundWorkflowHandler: Workflow delegation failed - #{e.class.name}: #{e.message}"
        raise
      end

      private

      # Extract and validate inputs from task and previous steps
      def extract_and_validate_inputs(task, sequence, _step)
        context = task.context.deep_symbolize_keys

        # Get approval results from previous step
        approval_result = sequence.get_results('get_manager_approval')
        approval_result = approval_result.deep_symbolize_keys if approval_result

        unless approval_result&.dig(:approval_obtained)
          raise TaskerCore::Errors::PermanentError.new(
            'Manager approval must be obtained before executing refund',
            error_code: 'MISSING_APPROVAL'
          )
        end

        # Get validation results to extract payment_id
        validation_result = sequence.get_results('validate_refund_request')
        validation_result = validation_result.deep_symbolize_keys if validation_result

        payment_id = validation_result&.dig(:payment_id)
        unless payment_id
          raise TaskerCore::Errors::PermanentError.new(
            'Payment ID not found in validation results',
            error_code: 'MISSING_PAYMENT_ID'
          )
        end

        # Map customer success context to payments workflow input
        # This demonstrates how different teams have different data models
        {
          namespace: 'payments',
          workflow_name: 'process_refund',
          workflow_version: '2.1.0',
          context: {
            # Map customer service ticket to payment ID
            payment_id: payment_id,
            refund_amount: context[:refund_amount],
            refund_reason: context[:refund_reason] || 'customer_request',
            customer_email: context[:customer_email] || 'customer@example.com',
            # Include cross-team coordination metadata
            initiated_by: 'customer_success',
            approval_id: approval_result[:approval_id],
            ticket_id: context[:ticket_id],
            correlation_id: context[:correlation_id] || generate_correlation_id
          }
        }
      end

      # Create task in payments namespace (simulates cross-namespace coordination)
      def create_payments_task(inputs)
        # In a real implementation, this would make an HTTP call to the payments service
        # or use the Tasker API to create a task in the payments namespace
        # For this demo, we simulate the task creation response

        task_id = "task_#{SecureRandom.uuid}"
        correlation_id = inputs[:context][:correlation_id]

        logger.info "📤 Creating task in #{inputs[:namespace]} namespace: #{inputs[:workflow_name]}"
        logger.info "   Context: payment_id=#{inputs[:context][:payment_id]}, refund_amount=#{inputs[:context][:refund_amount]}"
        logger.info "   Correlation ID: #{correlation_id}"

        # Simulate task creation success
        {
          task_id: task_id,
          status: 'created',
          correlation_id: correlation_id,
          namespace: inputs[:namespace],
          workflow_name: inputs[:workflow_name],
          workflow_version: inputs[:workflow_version],
          created_at: Time.now.utc.iso8601
        }
      end

      # Generate correlation ID for cross-team tracking
      def generate_correlation_id
        "cs-#{SecureRandom.hex(8)}"
      end
    end
  end
end
