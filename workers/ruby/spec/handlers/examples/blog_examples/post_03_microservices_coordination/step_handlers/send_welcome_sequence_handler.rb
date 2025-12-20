# frozen_string_literal: true

module Microservices
  module StepHandlers
    # Send welcome sequence handler demonstrating multi-step coordination
    #
    # This handler demonstrates:
    # - Accessing results from multiple prior steps (billing + preferences)
    # - Personalized messaging based on plan and preferences
    # - Multi-channel notification simulation (email, SMS, in-app)
    # - Conditional logic based on prior step results
    class SendWelcomeSequenceHandler < TaskerCore::StepHandler::Base
      # Welcome message templates by plan
      WELCOME_TEMPLATES = {
        'free' => {
          subject: 'Welcome to Our Platform!',
          greeting: 'Thanks for joining us',
          highlights: ['Get started with basic features', 'Explore your dashboard', 'Join our community'],
          upgrade_prompt: 'Upgrade to Pro for advanced features'
        },
        'pro' => {
          subject: 'Welcome to Pro!',
          greeting: 'Thanks for upgrading to Pro',
          highlights: ['Access advanced analytics', 'Priority support', 'API access', 'Custom integrations'],
          upgrade_prompt: 'Consider Enterprise for dedicated support'
        },
        'enterprise' => {
          subject: 'Welcome to Enterprise!',
          greeting: 'Welcome to your Enterprise account',
          highlights: ['Dedicated account manager', 'Custom SLA', 'Advanced security features', 'Priority support 24/7'],
          upgrade_prompt: nil
        }
      }.freeze

      def call(context)
        logger.info "📧 SendWelcomeSequenceHandler: Sending welcome sequence - task_uuid=#{context.task_uuid}"

        # Get results from prior steps
        user_data = context.get_dependency_result('create_user_account')
        billing_data = context.get_dependency_result('setup_billing_profile')
        preferences_data = context.get_dependency_result('initialize_preferences')

        # Validate required data
        validate_prior_step_results!(user_data, billing_data, preferences_data)

        user_id = user_data['user_id'] || user_data[:user_id]
        email = user_data['email'] || user_data[:email]
        plan = user_data['plan'] || user_data[:plan] || 'free'

        logger.info "   User ID: #{user_id}"
        logger.info "   Email: #{email}"
        logger.info "   Plan: #{plan}"

        # Get user preferences for notification channels
        prefs = extract_preferences(preferences_data)

        # Simulate notification service API calls
        result = simulate_notification_service_send(user_id, email, plan, prefs, billing_data)

        logger.info "✅ SendWelcomeSequenceHandler: Welcome sequence sent"
        logger.info "   Channels: #{result[:channels_used].join(', ')}"
        logger.info "   Messages: #{result[:messages_sent]}"

        TaskerCore::Types::StepHandlerCallResult.success(
          result: result,
          metadata: {
            operation: 'send_welcome_sequence',
            service: 'notification_service',
            plan: plan,
            channels_used: result[:channels_used].count,
            created_at: Time.now.utc.iso8601,
            handler_class: self.class.name
          }
        )
      rescue StandardError => e
        logger.error "❌ SendWelcomeSequenceHandler: Welcome sequence failed - #{e.class.name}: #{e.message}"
        raise
      end

      private

      def validate_prior_step_results!(user_data, billing_data, preferences_data)
        unless user_data
          raise TaskerCore::Errors::PermanentError.new(
            'User data not found from create_user_account step',
            error_code: 'MISSING_USER_DATA'
          )
        end

        unless billing_data
          raise TaskerCore::Errors::PermanentError.new(
            'Billing data not found from setup_billing_profile step',
            error_code: 'MISSING_BILLING_DATA'
          )
        end

        unless preferences_data
          raise TaskerCore::Errors::PermanentError.new(
            'Preferences data not found from initialize_preferences step',
            error_code: 'MISSING_PREFERENCES_DATA'
          )
        end
      end

      def extract_preferences(preferences_data)
        prefs_hash = preferences_data['preferences'] || preferences_data[:preferences] || {}
        prefs_hash.deep_symbolize_keys
      end

      def simulate_notification_service_send(user_id, email, plan, preferences, billing_data)
        template = WELCOME_TEMPLATES[plan] || WELCOME_TEMPLATES['free']

        # Determine which channels to use based on preferences
        channels_used = []
        messages_sent = []

        # Email (always send if email_notifications enabled)
        if preferences[:email_notifications] != false
          channels_used << 'email'
          messages_sent << {
            channel: 'email',
            to: email,
            subject: template[:subject],
            body: build_email_body(template, user_id, plan, billing_data),
            sent_at: Time.now.utc.iso8601
          }
        end

        # In-app notification (always send)
        channels_used << 'in_app'
        messages_sent << {
          channel: 'in_app',
          user_id: user_id,
          title: template[:subject],
          message: template[:greeting],
          sent_at: Time.now.utc.iso8601
        }

        # SMS (only for enterprise plan with phone number)
        if plan == 'enterprise'
          channels_used << 'sms'
          messages_sent << {
            channel: 'sms',
            to: '+1-555-ENTERPRISE', # Simulated phone number
            message: "Welcome to Enterprise! Your account manager will contact you soon.",
            sent_at: Time.now.utc.iso8601
          }
        end

        {
          user_id: user_id,
          plan: plan,
          channels_used: channels_used,
          messages_sent: messages_sent.count,
          welcome_sequence_id: "welcome_#{SecureRandom.hex(6)}",
          status: 'sent',
          sent_at: Time.now.utc.iso8601
        }
      end

      def build_email_body(template, user_id, plan, billing_data)
        body_parts = [
          template[:greeting],
          '',
          'Here are your account highlights:',
          *template[:highlights].map { |h| "• #{h}" }
        ]

        # Add billing information for paid plans
        if plan != 'free'
          billing_id = billing_data['billing_id'] || billing_data[:billing_id]
          next_billing = billing_data['next_billing_date'] || billing_data[:next_billing_date]
          body_parts << ''
          body_parts << "Billing ID: #{billing_id}"
          body_parts << "Next billing date: #{next_billing}"
        end

        # Add upgrade prompt if present
        if template[:upgrade_prompt]
          body_parts << ''
          body_parts << template[:upgrade_prompt]
        end

        body_parts << ''
        body_parts << "User ID: #{user_id}"

        body_parts.join("\n")
      end
    end
  end
end
