# frozen_string_literal: true

module CustomerSuccess
  class ProcessRefundHandler < TaskerCore::TaskHandler::Base
    def initialize_task(task_request)
      validate_refund_request_input(task_request[:context])
      super
    end

    def metadata
      super.merge(
        workflow_type: 'customer_success_process_refund',
        namespace: 'customer_success',
        blog_post: 'post_04',
        steps_count: 5,
        description: 'Process customer service refunds with approval workflow',
        average_completion_time: '15s'
      )
    end

    private

    def validate_refund_request_input(context)
      # Validate ticket_id
      ticket_id = context['ticket_id'] || context[:ticket_id]
      if ticket_id.blank?
        raise TaskerCore::Errors::ValidationError.new(
          'Ticket ID is required but was not provided',
          field: 'ticket_id',
          error_code: 'MISSING_TICKET_ID'
        )
      end

      # Validate customer_id
      customer_id = context['customer_id'] || context[:customer_id]
      if customer_id.blank?
        raise TaskerCore::Errors::ValidationError.new(
          'Customer ID is required but was not provided',
          field: 'customer_id',
          error_code: 'MISSING_CUSTOMER_ID'
        )
      end

      # Validate refund_amount
      refund_amount = context['refund_amount'] || context[:refund_amount]
      if refund_amount.blank?
        raise TaskerCore::Errors::ValidationError.new(
          'Refund amount is required but was not provided',
          field: 'refund_amount',
          error_code: 'MISSING_REFUND_AMOUNT'
        )
      end

      # Validate refund amount is positive
      if refund_amount <= 0
        raise TaskerCore::Errors::ValidationError.new(
          "Refund amount must be positive, got: #{refund_amount}",
          field: 'refund_amount',
          error_code: 'INVALID_REFUND_AMOUNT'
        )
      end

      # Validate customer_email if provided
      customer_email = context['customer_email'] || context[:customer_email]
      if customer_email.blank?
        raise TaskerCore::Errors::ValidationError.new(
          'Customer email is required but was not provided',
          field: 'customer_email',
          error_code: 'MISSING_CUSTOMER_EMAIL'
        )
      end

      return if customer_email.match?(/\A[^@\s]+@[^@\s]+\z/)

      raise TaskerCore::Errors::ValidationError.new(
        "Invalid customer email format: #{customer_email}",
        field: 'customer_email',
        error_code: 'INVALID_EMAIL_FORMAT'
      )
    end
  end
end
