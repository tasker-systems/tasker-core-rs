/**
 * Domain Events Examples
 *
 * Complete examples demonstrating how to create custom publishers
 * and subscribers for the Tasker domain event system.
 *
 * ## Publishers
 *
 * Publishers transform step execution results into business-focused
 * domain events. They are declared in YAML templates via the `publisher:`
 * field and registered at bootstrap time.
 *
 * - {@link PaymentEventPublisher} - Payment event transformation with validation
 * - {@link RefundEventPublisher} - Refund-specific event transformation
 *
 * ## Subscribers
 *
 * Subscribers handle fast (in-process) domain events for internal processing.
 * They subscribe to event patterns and receive events via the event poller.
 *
 * ### Logging
 * - {@link AuditLoggingSubscriber} - Comprehensive audit logging for all events
 * - {@link PaymentLoggingSubscriber} - Payment-specific structured logging
 *
 * ### Metrics
 * - {@link MetricsSubscriber} - Event counting and latency histograms
 * - {@link PaymentMetricsSubscriber} - Payment-specific metrics tracking
 *
 * ## Usage Example
 *
 * ```typescript
 * import {
 *   PublisherRegistry,
 *   SubscriberRegistry,
 *   InProcessDomainEventPoller,
 * } from 'tasker-core/handler';
 *
 * import {
 *   PaymentEventPublisher,
 *   AuditLoggingSubscriber,
 *   MetricsSubscriber,
 * } from 'tasker-core/examples/domain-events';
 *
 * // Register publishers (matches YAML publisher: field)
 * PublisherRegistry.instance().register(new PaymentEventPublisher());
 *
 * // Register subscribers
 * SubscriberRegistry.instance().register(AuditLoggingSubscriber);
 * SubscriberRegistry.instance().register(MetricsSubscriber);
 *
 * // Create and start the event poller
 * const poller = new InProcessDomainEventPoller();
 * SubscriberRegistry.instance().startAll(poller);
 * poller.start();
 *
 * // Later: export metrics
 * console.log(MetricsSubscriber.exportMetrics());
 * ```
 *
 * @module examples/domain-events
 * @see TAS-112
 */

// Logging subscribers
export { AuditLoggingSubscriber, PaymentLoggingSubscriber } from './logging-subscriber.js';
// Metrics subscribers
export { MetricsSubscriber, PaymentMetricsSubscriber } from './metrics-subscriber.js';
// Publishers
export { PaymentEventPublisher, RefundEventPublisher } from './payment-publisher.js';
