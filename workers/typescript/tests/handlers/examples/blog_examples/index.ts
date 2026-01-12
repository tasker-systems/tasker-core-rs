/**
 * Blog Example Handlers.
 *
 * Exports all handlers from blog post examples.
 * Handler callables in YAML templates use dot notation:
 * - Ecommerce.StepHandlers.ValidateCartHandler
 * - Ecommerce.StepHandlers.ProcessPaymentHandler
 * - Ecommerce.StepHandlers.UpdateInventoryHandler
 * - Ecommerce.StepHandlers.CreateOrderHandler
 * - Ecommerce.StepHandlers.SendConfirmationHandler
 */

// Post 01: E-commerce Order Processing
export {
  CreateOrderHandler,
  ProcessPaymentHandler,
  SendConfirmationHandler,
  UpdateInventoryHandler,
  ValidateCartHandler,
} from './post_01_ecommerce/index.js';
