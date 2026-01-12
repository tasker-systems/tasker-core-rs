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
 * - DataPipeline.StepHandlers.ExtractSalesDataHandler
 * - DataPipeline.StepHandlers.ExtractInventoryDataHandler
 * - DataPipeline.StepHandlers.ExtractCustomerDataHandler
 * - DataPipeline.StepHandlers.TransformSalesHandler
 * - DataPipeline.StepHandlers.TransformInventoryHandler
 * - DataPipeline.StepHandlers.TransformCustomersHandler
 * - DataPipeline.StepHandlers.AggregateMetricsHandler
 * - DataPipeline.StepHandlers.GenerateInsightsHandler
 * - Microservices.StepHandlers.CreateUserAccountHandler
 * - Microservices.StepHandlers.SetupBillingProfileHandler
 * - Microservices.StepHandlers.InitializePreferencesHandler
 * - Microservices.StepHandlers.SendWelcomeSequenceHandler
 * - Microservices.StepHandlers.UpdateUserStatusHandler
 */

// Post 01: E-commerce Order Processing
export {
  CreateOrderHandler,
  ProcessPaymentHandler,
  SendConfirmationHandler,
  UpdateInventoryHandler,
  ValidateCartHandler,
} from './post_01_ecommerce/index.js';

// Post 02: Data Pipeline Analytics
export {
  AggregateMetricsHandler,
  ExtractCustomerDataHandler,
  ExtractInventoryDataHandler,
  ExtractSalesDataHandler,
  GenerateInsightsHandler,
  TransformCustomersHandler,
  TransformInventoryHandler,
  TransformSalesHandler,
} from './post_02_data_pipeline/index.js';

// Post 03: Microservices Coordination
export {
  CreateUserAccountHandler,
  InitializePreferencesHandler,
  SendWelcomeSequenceHandler,
  SetupBillingProfileHandler,
  UpdateUserStatusHandler,
} from './post_03_microservices/index.js';
