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
];
