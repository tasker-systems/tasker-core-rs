# frozen_string_literal: true

module Microservices
  module StepHandlers
    # Setup billing profile handler demonstrating graceful degradation
    #
    # This handler demonstrates:
    # - Accessing prior step results (user_id from create_user_account)
    # - Plan-based logic (enterprise vs free)
    # - Graceful degradation (free plan users skip billing setup)
    # - Inline service simulation
    #
    # TAS-137 Best Practices Demonstrated:
    # - get_dependency_result() for upstream step data
    # - get_dependency_field() for nested field extraction from dependencies
    class SetupBillingProfileHandler < TaskerCore::StepHandler::Base
      # Simulated billing tiers with pricing
      BILLING_TIERS = {
        'free' => { price: 0, features: ['basic_features'], billing_required: false },
        'pro' => { price: 29.99, features: %w[basic_features advanced_analytics], billing_required: true },
        'enterprise' => { price: 299.99,
                          features: %w[basic_features advanced_analytics priority_support custom_integrations], billing_required: true }
      }.freeze

      def call(context)
        logger.info "💳 SetupBillingProfileHandler: Setting up billing profile - task_uuid=#{context.task_uuid}"

        # Get user_id from create_user_account step
        user_data = context.get_dependency_result('create_user_account')
        unless user_data
          raise TaskerCore::Errors::PermanentError.new(
            'User data not found from create_user_account step',
            error_code: 'MISSING_USER_DATA'
          )
        end

        # TAS-137: Use get_dependency_field() for nested field extraction
        user_id = context.get_dependency_field('create_user_account', 'user_id')
        plan = context.get_dependency_field('create_user_account', 'plan') || 'free'

        logger.info "   User ID: #{user_id}"
        logger.info "   Plan: #{plan}"

        # Check if billing setup is required for this plan
        tier_config = BILLING_TIERS[plan] || BILLING_TIERS['free']

        if tier_config[:billing_required]
          # Simulate billing service API call for paid plans
          result = simulate_billing_service_setup(user_id, plan, tier_config)
          logger.info "✅ SetupBillingProfileHandler: Billing profile created - billing_id=#{result[:billing_id]}"
        else
          # Graceful degradation: free plan doesn't need billing setup
          result = {
            user_id: user_id,
            plan: plan,
            billing_required: false,
            status: 'skipped_free_plan',
            message: 'Free plan users do not require billing setup'
          }
          logger.info '✅ SetupBillingProfileHandler: Billing setup skipped for free plan'
        end

        TaskerCore::Types::StepHandlerCallResult.success(
          result: result,
          metadata: {
            operation: 'setup_billing',
            service: 'billing_service',
            plan: plan,
            billing_required: tier_config[:billing_required],
            created_at: Time.now.utc.iso8601,
            handler_class: self.class.name
          }
        )
      rescue StandardError => e
        logger.error "❌ SetupBillingProfileHandler: Billing setup failed - #{e.class.name}: #{e.message}"
        raise
      end

      private

      def simulate_billing_service_setup(user_id, plan, tier_config)
        # Simulate creating a billing profile in the billing service
        {
          billing_id: "billing_#{SecureRandom.hex(6)}",
          user_id: user_id,
          plan: plan,
          price: tier_config[:price],
          currency: 'USD',
          billing_cycle: 'monthly',
          features: tier_config[:features],
          status: 'active',
          next_billing_date: (Time.now.utc + 30.days).iso8601,
          created_at: Time.now.utc.iso8601
        }
      end
    end
  end
end
