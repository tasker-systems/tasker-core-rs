# frozen_string_literal: true

require_relative 'base_mock_service'

# Mock Email Service
# Simulates email delivery for e-commerce blog examples
class MockEmailService < BaseMockService
  # Standard email service errors
  class EmailError < StandardError; end
  class InvalidEmailError < EmailError; end
  class DeliveryError < EmailError; end
  class RateLimitError < EmailError; end

  # Send confirmation email
  # @param to [String] Recipient email address
  # @param template [String] Email template name
  # @param subject [String] Email subject
  # @param data [Hash] Template data
  # @return [Hash] Send result
  def self.send_confirmation(to:, template: 'order_confirmation', subject: nil, **data)
    instance = new
    instance.send_confirmation_email(
      to: to,
      template: template,
      subject: subject,
      **data
    )
  end

  # Instance method for sending confirmation email
  def send_confirmation_email(to:, template: 'order_confirmation', subject: nil, **data)
    log_call(:send_confirmation, {
               to: to,
               template: template,
               subject: subject,
               **data
             })

    default_response = {
      message_id: generate_id('msg'),
      status: 'sent',
      recipient: to,
      template: template,
      subject: subject || self.class.generate_subject(template),
      sent_at: generate_timestamp,
      delivery_status: 'delivered'
    }

    handle_response(:send_confirmation, default_response)
  end

  # Send welcome email
  # @param to [String] Recipient email address
  # @param customer_name [String] Customer name for personalization
  # @param welcome_data [Hash] Additional welcome data
  # @return [Hash] Send result
  def self.send_welcome(to:, customer_name:, **welcome_data)
    instance = new
    instance.log_call(:send_welcome, {
                        to: to,
                        customer_name: customer_name,
                        **welcome_data
                      })

    default_response = {
      message_id: instance.generate_id('msg'),
      status: 'sent',
      recipient: to,
      template: 'welcome_email',
      subject: "Welcome #{customer_name}!",
      sent_at: instance.generate_timestamp,
      personalization: {
        customer_name: customer_name,
        **welcome_data
      }
    }

    instance.handle_response(:send_welcome, default_response)
  end

  # Send notification email
  # @param to [String] Recipient email address
  # @param notification_type [String] Type of notification
  # @param message [String] Notification message
  # @param priority [String] Email priority ('low', 'normal', 'high')
  # @return [Hash] Send result
  def self.send_notification(to:, notification_type:, message:, priority: 'normal')
    instance = new
    instance.log_call(:send_notification, {
                        to: to,
                        notification_type: notification_type,
                        message: message,
                        priority: priority
                      })

    default_response = {
      message_id: instance.generate_id('msg'),
      status: 'sent',
      recipient: to,
      template: 'notification',
      subject: generate_notification_subject(notification_type),
      message: message,
      priority: priority,
      sent_at: instance.generate_timestamp
    }

    instance.handle_response(:send_notification, default_response)
  end

  # Get email delivery status
  # @param message_id [String] Message ID to check
  # @return [Hash] Delivery status
  def self.get_delivery_status(message_id:)
    instance = new
    instance.log_call(:get_delivery_status, { message_id: message_id })

    default_response = {
      message_id: message_id,
      status: 'delivered',
      delivered_at: instance.generate_timestamp,
      bounce_reason: nil,
      opens: 1,
      clicks: 0
    }

    instance.handle_response(:get_delivery_status, default_response)
  end

  # Generate subject line based on template
  # @param template [String] Template name
  # @return [String] Generated subject
  def self.generate_subject(template)
    subjects = {
      'order_confirmation' => 'Your Order Confirmation',
      'shipping_notification' => 'Your Order Has Shipped',
      'welcome_email' => 'Welcome to Our Store!',
      'password_reset' => 'Reset Your Password',
      'newsletter' => 'Weekly Newsletter',
      'promotion' => 'Special Offer Just for You'
    }

    subjects[template] || 'Notification'
  end

  # Generate notification subject
  # @param notification_type [String] Type of notification
  # @return [String] Generated subject
  def self.generate_notification_subject(notification_type)
    subjects = {
      'order_status' => 'Order Status Update',
      'inventory_alert' => 'Inventory Alert',
      'system_maintenance' => 'System Maintenance Notice',
      'security_alert' => 'Security Alert',
      'account_update' => 'Account Update'
    }

    subjects[notification_type] || 'Notification'
  end
end
