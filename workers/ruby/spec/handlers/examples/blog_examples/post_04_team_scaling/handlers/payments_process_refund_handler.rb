# frozen_string_literal: true

module Payments
  class ProcessRefundHandler < TaskerCore::TaskHandler::Base
    def initialize_task(task_request)
      validate_refund_input(task_request[:context])
      super
    end

    def metadata
      super.merge(
        workflow_type: 'payments_process_refund',
        namespace: 'payments',
        blog_post: 'post_04',
        steps_count: 4,
        description: 'Process payment gateway refunds with direct API integration',
        average_completion_time: '10s'
      )
    end

    private

    def validate_refund_input(context)
      # Validate payment_id
      payment_id = context['payment_id'] || context[:payment_id]
      if payment_id.blank?
        raise TaskerCore::Errors::ValidationError.new(
          'Payment ID is required but was not provided',
          field: 'payment_id',
          error_code: 'MISSING_PAYMENT_ID'
        )
      end

      # Validate payment ID format
      unless payment_id.match?(/^pay_[a-zA-Z0-9]+$/)
        raise TaskerCore::Errors::ValidationError.new(
          "Invalid payment ID format: #{payment_id}",
          field: 'payment_id',
          error_code: 'INVALID_PAYMENT_ID'
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
      return unless customer_email.present? && !customer_email.match?(/\A[^@\s]+@[^@\s]+\z/)

      raise TaskerCore::Errors::ValidationError.new(
        "Invalid customer email format: #{customer_email}",
        field: 'customer_email',
        error_code: 'INVALID_EMAIL_FORMAT'
      )
    end
  end
end
