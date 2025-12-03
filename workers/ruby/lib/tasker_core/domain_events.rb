# frozen_string_literal: true

# TAS-65: Domain Events Module
#
# Provides infrastructure for custom domain event publishers.
# Domain events are business events (e.g., "order.processed", "payment.completed")
# published after step execution based on YAML declarations.
#
# @example Using the domain events infrastructure
#   # Register custom publishers at bootstrap
#   require 'tasker_core/domain_events'
#
#   class PaymentEventPublisher < TaskerCore::DomainEvents::BasePublisher
#     def name
#       'PaymentEventPublisher'
#     end
#
#     def transform_payload(step_result, event_declaration, step_context)
#       {
#         payment_id: step_result[:result][:payment_id],
#         amount: step_result[:result][:amount],
#         status: step_result[:success] ? 'succeeded' : 'failed'
#       }
#     end
#   end
#
#   TaskerCore::DomainEvents::PublisherRegistry.instance.register(
#     PaymentEventPublisher.new
#   )
#
module TaskerCore
  module DomainEvents
    # Load domain events components
  end
end

# Publishers
require_relative 'domain_events/base_publisher'
require_relative 'domain_events/publisher_registry'

# Subscribers
require_relative 'domain_events/base_subscriber'
require_relative 'domain_events/subscriber_registry'
