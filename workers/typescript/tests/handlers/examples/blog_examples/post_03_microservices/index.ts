/**
 * Microservices Coordination Example Handlers (Blog Post 03).
 *
 * Exports all handlers for the user registration workflow.
 */

export {
  CreateUserAccountHandler,
  InitializePreferencesHandler,
  SendWelcomeSequenceHandler,
  SetupBillingProfileHandler,
  UpdateUserStatusHandler,
} from './step_handlers/microservices-handlers.js';
