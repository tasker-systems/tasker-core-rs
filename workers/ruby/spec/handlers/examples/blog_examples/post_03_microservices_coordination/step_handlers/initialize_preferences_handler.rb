# frozen_string_literal: true

module Microservices
  module StepHandlers
    # Initialize user preferences handler demonstrating default fallbacks
    #
    # This handler demonstrates:
    # - Accessing prior step results
    # - Default preferences with fallback values
    # - Inline service simulation
    # - Optional preferences handling
    #
    # TAS-137 Best Practices Demonstrated:
    # - get_dependency_result() for upstream step data
    # - get_dependency_field() for nested field extraction from dependencies
    # - get_input() for task context access
    class InitializePreferencesHandler < TaskerCore::StepHandler::Base
      # Default preference templates by plan
      DEFAULT_PREFERENCES = {
        'free' => {
          email_notifications: true,
          marketing_emails: false,
          product_updates: true,
          weekly_digest: false,
          theme: 'light',
          language: 'en',
          timezone: 'UTC'
        },
        'pro' => {
          email_notifications: true,
          marketing_emails: true,
          product_updates: true,
          weekly_digest: true,
          theme: 'dark',
          language: 'en',
          timezone: 'UTC',
          api_notifications: true
        },
        'enterprise' => {
          email_notifications: true,
          marketing_emails: true,
          product_updates: true,
          weekly_digest: true,
          theme: 'dark',
          language: 'en',
          timezone: 'UTC',
          api_notifications: true,
          audit_logs: true,
          advanced_reports: true
        }
      }.freeze

      def call(context)
        logger.info "⚙️  InitializePreferencesHandler: Initializing user preferences - task_uuid=#{context.task_uuid}"

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

        # TAS-137: Use get_input() for task context access with nested path
        user_info = context.get_input_or('user_info', {})
        user_info = user_info.deep_symbolize_keys if user_info.is_a?(Hash)
        custom_prefs = user_info[:preferences] || {}

        # Simulate preferences service API call
        result = simulate_preferences_service_initialize(user_id, plan, custom_prefs)

        logger.info "✅ InitializePreferencesHandler: Preferences initialized - preferences_id=#{result[:preferences_id]}"
        logger.info "   Preferences: #{result[:preferences].keys.join(', ')}"

        TaskerCore::Types::StepHandlerCallResult.success(
          result: result,
          metadata: {
            operation: 'initialize_preferences',
            service: 'preferences_service',
            plan: plan,
            custom_preferences_count: custom_prefs.keys.count,
            created_at: Time.now.utc.iso8601,
            handler_class: self.class.name
          }
        )
      rescue StandardError => e
        logger.error "❌ InitializePreferencesHandler: Preferences initialization failed - #{e.class.name}: #{e.message}"
        raise
      end

      private

      def simulate_preferences_service_initialize(user_id, plan, custom_prefs)
        # Get default preferences for plan
        default_prefs = DEFAULT_PREFERENCES[plan] || DEFAULT_PREFERENCES['free']

        # Merge custom preferences with defaults (custom takes precedence)
        final_prefs = default_prefs.merge(custom_prefs)

        # Simulate creating preferences in the preferences service
        {
          preferences_id: "prefs_#{SecureRandom.hex(6)}",
          user_id: user_id,
          plan: plan,
          preferences: final_prefs,
          defaults_applied: default_prefs.keys.count,
          customizations: custom_prefs.keys.count,
          status: 'active',
          created_at: Time.now.utc.iso8601,
          updated_at: Time.now.utc.iso8601
        }
      end
    end
  end
end
