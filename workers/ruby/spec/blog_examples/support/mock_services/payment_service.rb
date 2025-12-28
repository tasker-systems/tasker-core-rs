# frozen_string_literal: true

require_relative 'base_mock_service'

# Mock Payment Service
# Simulates payment processing for e-commerce blog examples
class MockPaymentService < BaseMockService
  # Standard payment processing errors
  class PaymentError < StandardError; end
  class InsufficientFundsError < PaymentError; end
  class CardDeclinedError < PaymentError; end
  class NetworkError < PaymentError; end

  # Process a payment
  # @param amount [Float] Payment amount
  # @param method [String] Payment method ('credit_card', 'debit_card', 'paypal', etc.)
  # @param currency [String] Currency code (default: 'USD')
  # @param card_last_four [String] Last four digits of card (optional)
  # @param customer_id [Integer] Customer ID (optional)
  # @return [Hash] Payment result
  def self.process_payment(amount:, method:, currency: 'USD', **)
    instance = new
    instance.process_payment_call(
      amount: amount,
      method: method,
      currency: currency,
      **
    )
  end

  # Instance method for processing payment
  def process_payment_call(amount:, method:, currency: 'USD', **options)
    # Ensure amount is stored as a numeric value for test assertions
    numeric_amount = amount.is_a?(String) ? amount.to_f : amount

    log_call(:process_payment, {
               amount: numeric_amount,
               method: method,
               currency: currency,
               **options
             })

    default_response = {
      payment_id: generate_id('pay'),
      status: 'succeeded',
      amount_charged: amount,
      currency: currency,
      payment_method_type: method,
      transaction_id: generate_id('txn'),
      processed_at: generate_timestamp,
      fees: self.class.calculate_fees(amount),
      **options.slice(:customer_id, :card_last_four)
    }

    handle_response(:process_payment, default_response)
  end

  # Refund a payment
  # @param payment_id [String] Original payment ID
  # @param amount [Float] Refund amount (optional, defaults to full amount)
  # @param reason [String] Refund reason
  # @return [Hash] Refund result
  def self.refund_payment(payment_id:, amount: nil, reason: 'requested_by_customer')
    instance = new
    instance.log_call(:refund_payment, {
                        payment_id: payment_id,
                        amount: amount,
                        reason: reason
                      })

    default_response = {
      refund_id: instance.generate_id('ref'),
      payment_id: payment_id,
      status: 'succeeded',
      amount_refunded: amount || 100.00, # Default refund amount
      reason: reason,
      processed_at: instance.generate_timestamp
    }

    instance.handle_response(:refund_payment, default_response)
  end

  # Get payment status
  # @param payment_id [String] Payment ID to check
  # @return [Hash] Payment status
  def self.get_payment_status(payment_id:)
    instance = new
    instance.log_call(:get_payment_status, { payment_id: payment_id })

    default_response = {
      payment_id: payment_id,
      status: 'succeeded',
      amount: 100.00,
      currency: 'USD',
      created_at: instance.generate_timestamp,
      updated_at: instance.generate_timestamp
    }

    instance.handle_response(:get_payment_status, default_response)
  end

  # Verify payment method (for validation before processing)
  # @param method [String] Payment method
  # @param details [Hash] Payment method details
  # @return [Hash] Verification result
  def self.verify_payment_method(method:, **details)
    instance = new
    instance.log_call(:verify_payment_method, {
                        method: method,
                        **details
                      })

    default_response = {
      valid: true,
      method: method,
      verified_at: instance.generate_timestamp,
      **details.slice(:card_last_four, :expiry_month, :expiry_year)
    }

    instance.handle_response(:verify_payment_method, default_response)
  end

  # Calculate processing fees
  # @param amount [Float] Transaction amount
  # @return [Hash] Fee breakdown
  def self.calculate_fees(amount)
    # Convert amount to float to handle BigDecimal, String, or other numeric types
    numeric_amount = amount.to_f

    processing_fee = (numeric_amount * 0.029).round(2) # 2.9%
    fixed_fee = 0.30

    {
      processing_fee: processing_fee,
      fixed_fee: fixed_fee,
      total_fees: processing_fee + fixed_fee
    }
  end
end
