/**
 * Example Handlers for E2E Testing.
 *
 * This module exports all example handlers that can be registered
 * for integration testing with the orchestration system.
 *
 * Handler callables in YAML templates use dot notation:
 * - LinearWorkflow.StepHandlers.LinearStep1Handler
 * - TestScenarios.StepHandlers.SuccessStepHandler
 * - TestErrors.StepHandlers.PermanentErrorHandler
 * - DiamondWorkflow.StepHandlers.DiamondStartHandler
 * - ConditionalApproval.StepHandlers.ValidateRequestHandler
 * - BatchProcessing.StepHandlers.CsvAnalyzerHandler
 * - DomainEvents.StepHandlers.ValidateOrderHandler
 * - ResolverTests.StepHandlers.MultiMethodHandler
 */

// Batch Processing Handlers
export {
  CsvAnalyzerHandler,
  CsvBatchProcessorHandler,
  CsvResultsAggregatorHandler,
} from './batch_processing/index.js';
// TAS-91: Blog Example Handlers (Post 01: E-commerce)
// Using prefixed names to avoid conflicts with domain_events handlers
// TAS-91: Blog Example Handlers (Post 02: Data Pipeline)
// TAS-91: Blog Example Handlers (Post 03: Microservices)
// TAS-91: Blog Example Handlers (Post 04: Team Scaling)
export {
  AggregateMetricsHandler as DataPipelineAggregateMetricsHandler,
  CheckRefundPolicyHandler as CustomerSuccessCheckRefundPolicyHandler,
  CreateOrderHandler as EcommerceCreateOrderHandler,
  CreateUserAccountHandler as MicroservicesCreateUserAccountHandler,
  ExecuteRefundWorkflowHandler as CustomerSuccessExecuteRefundWorkflowHandler,
  ExtractCustomerDataHandler as DataPipelineExtractCustomerDataHandler,
  ExtractInventoryDataHandler as DataPipelineExtractInventoryDataHandler,
  ExtractSalesDataHandler as DataPipelineExtractSalesDataHandler,
  GenerateInsightsHandler as DataPipelineGenerateInsightsHandler,
  GetManagerApprovalHandler as CustomerSuccessGetManagerApprovalHandler,
  InitializePreferencesHandler as MicroservicesInitializePreferencesHandler,
  NotifyCustomerHandler as PaymentsNotifyCustomerHandler,
  ProcessGatewayRefundHandler as PaymentsProcessGatewayRefundHandler,
  ProcessPaymentHandler as EcommerceProcessPaymentHandler,
  SendConfirmationHandler as EcommerceSendConfirmationHandler,
  SendWelcomeSequenceHandler as MicroservicesSendWelcomeSequenceHandler,
  SetupBillingProfileHandler as MicroservicesSetupBillingProfileHandler,
  TransformCustomersHandler as DataPipelineTransformCustomersHandler,
  TransformInventoryHandler as DataPipelineTransformInventoryHandler,
  TransformSalesHandler as DataPipelineTransformSalesHandler,
  UpdateInventoryHandler as EcommerceUpdateInventoryHandler,
  UpdatePaymentRecordsHandler as PaymentsUpdatePaymentRecordsHandler,
  UpdateTicketStatusHandler as CustomerSuccessUpdateTicketStatusHandler,
  UpdateUserStatusHandler as MicroservicesUpdateUserStatusHandler,
  ValidateCartHandler as EcommerceValidateCartHandler,
  // Post 04: Team Scaling - Payments namespace
  ValidatePaymentEligibilityHandler as PaymentsValidatePaymentEligibilityHandler,
  // Post 04: Team Scaling - Customer Success namespace
  ValidateRefundRequestHandler as CustomerSuccessValidateRefundRequestHandler,
} from './blog_examples/index.js';
// TAS-125: Checkpoint Yield Handlers
export {
  CheckpointYieldAggregatorHandler,
  CheckpointYieldAnalyzerHandler,
  CheckpointYieldWorkerHandler,
} from './checkpoint_yield/index.js';
// Conditional Approval Handlers
export {
  AutoApproveHandler,
  FinalizeApprovalHandler,
  FinanceReviewHandler,
  ManagerApprovalHandler,
  RoutingDecisionHandler,
  ValidateRequestHandler,
} from './conditional_approval/index.js';
// Diamond Workflow Handlers
export {
  DiamondBranchBHandler,
  DiamondBranchCHandler,
  DiamondEndHandler,
  DiamondStartHandler,
} from './diamond_workflow/index.js';
// Domain Events Handlers
export {
  ProcessPaymentHandler,
  SendNotificationHandler,
  UpdateInventoryHandler,
  ValidateOrderHandler,
} from './domain_events/index.js';
// Linear Workflow Handlers
export {
  LinearStep1Handler,
  LinearStep2Handler,
  LinearStep3Handler,
  LinearStep4Handler,
} from './linear_workflow/index.js';
// TAS-93 Phase 5: Resolver Tests Handlers
export { AlternateMethodHandler, MultiMethodHandler } from './resolver_tests/index.js';
// Error Testing Handlers
export {
  PermanentErrorHandler,
  RetryableErrorHandler,
  SuccessHandler,
} from './test_errors/index.js';
// Test Scenarios Handlers
export { SuccessStepHandler } from './test_scenarios/index.js';

import {
  CsvAnalyzerHandler,
  CsvBatchProcessorHandler,
  CsvResultsAggregatorHandler,
} from './batch_processing/index.js';
// TAS-91: Blog Example Handlers (Post 01: E-commerce)
// TAS-91: Blog Example Handlers (Post 02: Data Pipeline)
// TAS-91: Blog Example Handlers (Post 03: Microservices)
// TAS-91: Blog Example Handlers (Post 04: Team Scaling)
import {
  CheckRefundPolicyHandler as CustomerSuccessCheckRefundPolicyHandler,
  ExecuteRefundWorkflowHandler as CustomerSuccessExecuteRefundWorkflowHandler,
  GetManagerApprovalHandler as CustomerSuccessGetManagerApprovalHandler,
  UpdateTicketStatusHandler as CustomerSuccessUpdateTicketStatusHandler,
  // Post 04: Team Scaling
  ValidateRefundRequestHandler as CustomerSuccessValidateRefundRequestHandler,
  AggregateMetricsHandler as DataPipelineAggregateMetricsHandler,
  ExtractCustomerDataHandler as DataPipelineExtractCustomerDataHandler,
  ExtractInventoryDataHandler as DataPipelineExtractInventoryDataHandler,
  ExtractSalesDataHandler as DataPipelineExtractSalesDataHandler,
  GenerateInsightsHandler as DataPipelineGenerateInsightsHandler,
  TransformCustomersHandler as DataPipelineTransformCustomersHandler,
  TransformInventoryHandler as DataPipelineTransformInventoryHandler,
  TransformSalesHandler as DataPipelineTransformSalesHandler,
  CreateOrderHandler as EcommerceCreateOrderHandler,
  ProcessPaymentHandler as EcommerceProcessPaymentHandler,
  SendConfirmationHandler as EcommerceSendConfirmationHandler,
  UpdateInventoryHandler as EcommerceUpdateInventoryHandler,
  ValidateCartHandler as EcommerceValidateCartHandler,
  CreateUserAccountHandler as MicroservicesCreateUserAccountHandler,
  InitializePreferencesHandler as MicroservicesInitializePreferencesHandler,
  SendWelcomeSequenceHandler as MicroservicesSendWelcomeSequenceHandler,
  SetupBillingProfileHandler as MicroservicesSetupBillingProfileHandler,
  UpdateUserStatusHandler as MicroservicesUpdateUserStatusHandler,
  NotifyCustomerHandler as PaymentsNotifyCustomerHandler,
  ProcessGatewayRefundHandler as PaymentsProcessGatewayRefundHandler,
  UpdatePaymentRecordsHandler as PaymentsUpdatePaymentRecordsHandler,
  ValidatePaymentEligibilityHandler as PaymentsValidatePaymentEligibilityHandler,
} from './blog_examples/index.js';
import {
  CheckpointYieldAggregatorHandler,
  CheckpointYieldAnalyzerHandler,
  CheckpointYieldWorkerHandler,
} from './checkpoint_yield/index.js';
import {
  AutoApproveHandler,
  FinalizeApprovalHandler,
  FinanceReviewHandler,
  ManagerApprovalHandler,
  RoutingDecisionHandler,
  ValidateRequestHandler,
} from './conditional_approval/index.js';
import {
  DiamondBranchBHandler,
  DiamondBranchCHandler,
  DiamondEndHandler,
  DiamondStartHandler,
} from './diamond_workflow/index.js';
import {
  ProcessPaymentHandler,
  SendNotificationHandler,
  UpdateInventoryHandler,
  ValidateOrderHandler,
} from './domain_events/index.js';
import {
  LinearStep1Handler,
  LinearStep2Handler,
  LinearStep3Handler,
  LinearStep4Handler,
} from './linear_workflow/index.js';
// TAS-93 Phase 5: Resolver Tests Handlers
import { AlternateMethodHandler, MultiMethodHandler } from './resolver_tests/index.js';
import {
  PermanentErrorHandler,
  RetryableErrorHandler,
  SuccessHandler,
} from './test_errors/index.js';
import { SuccessStepHandler } from './test_scenarios/index.js';

/**
 * Array of all example handler classes for easy registration.
 */
export const ALL_EXAMPLE_HANDLERS = [
  // Linear Workflow
  LinearStep1Handler,
  LinearStep2Handler,
  LinearStep3Handler,
  LinearStep4Handler,
  // Test Scenarios
  SuccessStepHandler,
  // Error Testing
  SuccessHandler,
  PermanentErrorHandler,
  RetryableErrorHandler,
  // Diamond Workflow
  DiamondStartHandler,
  DiamondBranchBHandler,
  DiamondBranchCHandler,
  DiamondEndHandler,
  // Conditional Approval
  ValidateRequestHandler,
  RoutingDecisionHandler,
  AutoApproveHandler,
  ManagerApprovalHandler,
  FinanceReviewHandler,
  FinalizeApprovalHandler,
  // Batch Processing
  CsvAnalyzerHandler,
  CsvBatchProcessorHandler,
  CsvResultsAggregatorHandler,
  // TAS-125: Checkpoint Yield
  CheckpointYieldAnalyzerHandler,
  CheckpointYieldWorkerHandler,
  CheckpointYieldAggregatorHandler,
  // Domain Events
  ValidateOrderHandler,
  ProcessPaymentHandler,
  UpdateInventoryHandler,
  SendNotificationHandler,
  // TAS-93 Phase 5: Resolver Tests
  MultiMethodHandler,
  AlternateMethodHandler,
  // TAS-91: Blog Examples (Post 01: E-commerce)
  EcommerceValidateCartHandler,
  EcommerceProcessPaymentHandler,
  EcommerceUpdateInventoryHandler,
  EcommerceCreateOrderHandler,
  EcommerceSendConfirmationHandler,
  // TAS-91: Blog Examples (Post 02: Data Pipeline)
  DataPipelineExtractSalesDataHandler,
  DataPipelineExtractInventoryDataHandler,
  DataPipelineExtractCustomerDataHandler,
  DataPipelineTransformSalesHandler,
  DataPipelineTransformInventoryHandler,
  DataPipelineTransformCustomersHandler,
  DataPipelineAggregateMetricsHandler,
  DataPipelineGenerateInsightsHandler,
  // TAS-91: Blog Examples (Post 03: Microservices)
  MicroservicesCreateUserAccountHandler,
  MicroservicesSetupBillingProfileHandler,
  MicroservicesInitializePreferencesHandler,
  MicroservicesSendWelcomeSequenceHandler,
  MicroservicesUpdateUserStatusHandler,
  // TAS-91: Blog Examples (Post 04: Team Scaling - Customer Success)
  CustomerSuccessValidateRefundRequestHandler,
  CustomerSuccessCheckRefundPolicyHandler,
  CustomerSuccessGetManagerApprovalHandler,
  CustomerSuccessExecuteRefundWorkflowHandler,
  CustomerSuccessUpdateTicketStatusHandler,
  // TAS-91: Blog Examples (Post 04: Team Scaling - Payments)
  PaymentsValidatePaymentEligibilityHandler,
  PaymentsProcessGatewayRefundHandler,
  PaymentsUpdatePaymentRecordsHandler,
  PaymentsNotifyCustomerHandler,
];
