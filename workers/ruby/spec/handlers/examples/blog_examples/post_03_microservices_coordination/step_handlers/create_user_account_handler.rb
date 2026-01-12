# frozen_string_literal: true

module Microservices
  module StepHandlers
    # Create user account handler demonstrating idempotent operations
    #
    # This handler demonstrates:
    # - Input validation (PermanentError for missing required fields)
    # - Idempotent operations (handling 409 conflicts gracefully)
    # - Inline service simulation (no external dependencies)
    # - Proper error classification
    #
    # TAS-137 Best Practices Demonstrated:
    # - get_input_or() for task context with default values
    class CreateUserAccountHandler < TaskerCore::StepHandler::Base
      # Simulated user database (represents existing users in the "user service")
      EXISTING_USERS = {
        'existing@example.com' => {
          id: 'user_existing_001',
          email: 'existing@example.com',
          name: 'Existing User',
          plan: 'free',
          created_at: '2025-01-01T00:00:00Z'
        }
      }.freeze

      def call(context)
        logger.info "👤 CreateUserAccountHandler: Creating user account - task_uuid=#{context.task_uuid}"

        # Extract and validate user info
        user_info = extract_and_validate_user_info(context)

        logger.info "   Email: #{user_info[:email]}"
        logger.info "   Plan: #{user_info[:plan]}"

        # Simulate user service API call
        result = simulate_user_service_create(user_info)

        logger.info "✅ CreateUserAccountHandler: User account #{result[:status]} - user_id=#{result[:user_id]}"

        TaskerCore::Types::StepHandlerCallResult.success(
          result: result,
          metadata: {
            operation: 'create_user',
            service: 'user_service',
            idempotent: result[:status] == 'already_exists',
            created_at: Time.now.utc.iso8601,
            handler_class: self.class.name
          }
        )
      rescue StandardError => e
        logger.error "❌ CreateUserAccountHandler: User creation failed - #{e.class.name}: #{e.message}"
        raise
      end

      private

      def extract_and_validate_user_info(context)
        # TAS-137: Use get_input_or() for task context with default value
        user_info = context.get_input_or('user_info', {})
        user_info = user_info.deep_symbolize_keys if user_info

        # Validate required fields - these are PERMANENT errors (don't retry)
        unless user_info[:email]
          raise TaskerCore::Errors::PermanentError.new(
            'Email is required but was not provided',
            error_code: 'MISSING_EMAIL'
          )
        end

        unless user_info[:name]
          raise TaskerCore::Errors::PermanentError.new(
            'Name is required but was not provided',
            error_code: 'MISSING_NAME'
          )
        end

        # Validate email format
        unless user_info[:email].match?(/\A[\w+\-.]+@[a-z\d\-]+(\.[a-z\d\-]+)*\.[a-z]+\z/i)
          raise TaskerCore::Errors::PermanentError.new(
            "Invalid email format: #{user_info[:email]}",
            error_code: 'INVALID_EMAIL_FORMAT'
          )
        end

        # Return validated user data
        {
          email: user_info[:email],
          name: user_info[:name],
          plan: user_info[:plan] || 'free',
          phone: user_info[:phone],
          source: user_info[:source] || 'web'
        }.compact
      end

      def simulate_user_service_create(user_info)
        # Check if user already exists (simulates 409 conflict)
        if EXISTING_USERS.key?(user_info[:email])
          existing_user = EXISTING_USERS[user_info[:email]]

          # Idempotent operation: if user matches, return existing user
          if user_matches?(existing_user, user_info)
            logger.info "   User already exists, idempotent success: #{existing_user[:id]}"
            return {
              user_id: existing_user[:id],
              email: existing_user[:email],
              plan: existing_user[:plan],
              status: 'already_exists',
              created_at: existing_user[:created_at]
            }
          else
            # User exists but data doesn't match - permanent error
            raise TaskerCore::Errors::PermanentError.new(
              "User with email #{user_info[:email]} already exists with different data",
              error_code: 'USER_CONFLICT'
            )
          end
        end

        # Simulate successful user creation (201 Created)
        {
          user_id: "user_#{SecureRandom.hex(6)}",
          email: user_info[:email],
          name: user_info[:name],
          plan: user_info[:plan],
          status: 'created',
          created_at: Time.now.utc.iso8601
        }
      end

      def user_matches?(existing_user, new_user_info)
        # Check if core attributes match for idempotency
        existing_user[:email] == new_user_info[:email] &&
          existing_user[:name] == new_user_info[:name] &&
          existing_user[:plan] == new_user_info[:plan]
      end
    end
  end
end
