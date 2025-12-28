/**
 * Conditional Approval Step Handlers for E2E Testing.
 *
 * Implements the conditional approval workflow with decision points:
 * 1. ValidateRequest: Validate the approval request
 * 2. RoutingDecision: Route based on amount thresholds (decision point)
 * 3. AutoApprove: Automatic approval for small amounts (< $1,000)
 * 4. ManagerApproval: Manager review for medium amounts ($1,000-$4,999)
 * 5. FinanceReview: Finance review for large amounts (>= $5,000)
 * 6. FinalizeApproval: Convergence point for all approval paths
 *
 * Matches Ruby and Python conditional approval implementations for testing parity.
 */

import { StepHandler } from '../../../../../src/handler/base.js';
import { DecisionStepHandler } from '../../../../../src/handler/decision.js';
import type { StepContext } from '../../../../../src/types/step-context.js';
import type {
  DecisionResult,
  StepHandlerResult,
} from '../../../../../src/types/step-handler-result.js';

/**
 * Validate the approval request.
 */
export class ValidateRequestHandler extends StepHandler {
  static handlerName = 'conditional_approval.step_handlers.ValidateRequestHandler';
  static handlerVersion = '1.0.0';

  async call(context: StepContext): Promise<StepHandlerResult> {
    const amount = context.getInput<number>('amount');
    const requester = context.getInput<string>('requester');
    const purpose = context.getInput<string>('purpose');

    const errors: string[] = [];

    if (amount === undefined || amount === null || amount <= 0) {
      errors.push('Amount must be a positive number');
    }

    if (!requester || requester.trim().length === 0) {
      errors.push('Requester is required');
    }

    if (!purpose || purpose.trim().length === 0) {
      errors.push('Purpose is required');
    }

    if (errors.length > 0) {
      return this.failure(errors.join('; '), 'validation_error', false);
    }

    return this.success({
      validated: true,
      amount,
      requester,
      purpose,
      validated_at: new Date().toISOString(),
    });
  }
}

/**
 * Decision point: Route based on amount thresholds.
 *
 * Routing rules:
 * - Amount < $1,000: Create auto_approve_ts
 * - Amount $1,000-$4,999: Create manager_approval_ts
 * - Amount >= $5,000: Create manager_approval_ts + finance_review_ts
 */
export class RoutingDecisionHandler extends DecisionStepHandler {
  static handlerName = 'conditional_approval.step_handlers.RoutingDecisionHandler';
  static handlerVersion = '1.0.0';

  private static readonly SMALL_THRESHOLD = 1000;
  private static readonly LARGE_THRESHOLD = 5000;

  async call(context: StepContext): Promise<DecisionResult> {
    // getDependencyResult() already unwraps the 'result' field, so we get the inner value directly
    const validateResult = context.getDependencyResult('validate_request_ts') as Record<
      string,
      unknown
    > | null;

    if (!validateResult) {
      return this.failure(
        'Missing dependency result from validate_request_ts',
        'dependency_error',
        false
      ) as DecisionResult;
    }

    const amount = validateResult.amount as number;
    const stepsToCreate: string[] = [];
    let routingPath: string;

    if (amount < RoutingDecisionHandler.SMALL_THRESHOLD) {
      stepsToCreate.push('auto_approve_ts');
      routingPath = 'auto_approve';
    } else if (amount < RoutingDecisionHandler.LARGE_THRESHOLD) {
      stepsToCreate.push('manager_approval_ts');
      routingPath = 'manager_approval';
    } else {
      stepsToCreate.push('manager_approval_ts');
      stepsToCreate.push('finance_review_ts');
      routingPath = 'dual_approval';
    }

    return this.decisionSuccess(stepsToCreate, {
      amount,
      routing_path: routingPath,
      steps_created: stepsToCreate,
      thresholds: {
        small: RoutingDecisionHandler.SMALL_THRESHOLD,
        large: RoutingDecisionHandler.LARGE_THRESHOLD,
      },
      decided_at: new Date().toISOString(),
    });
  }
}

/**
 * Automatic approval for small amounts.
 */
export class AutoApproveHandler extends StepHandler {
  static handlerName = 'conditional_approval.step_handlers.AutoApproveHandler';
  static handlerVersion = '1.0.0';

  async call(context: StepContext): Promise<StepHandlerResult> {
    // getDependencyResult() already unwraps the 'result' field, so we get the inner value directly
    const validateResult = context.getDependencyResult('validate_request_ts') as Record<
      string,
      unknown
    > | null;

    if (!validateResult) {
      return this.failure(
        'Missing dependency result from validate_request_ts',
        'dependency_error',
        false
      );
    }

    const amount = validateResult.amount as number;
    const requester = validateResult.requester as string;

    return this.success({
      approved: true,
      approval_type: 'automatic',
      amount,
      requester,
      reason: 'Amount below automatic approval threshold',
      approved_at: new Date().toISOString(),
    });
  }
}

/**
 * Manager approval for medium/large amounts.
 */
export class ManagerApprovalHandler extends StepHandler {
  static handlerName = 'conditional_approval.step_handlers.ManagerApprovalHandler';
  static handlerVersion = '1.0.0';

  async call(context: StepContext): Promise<StepHandlerResult> {
    // getDependencyResult() already unwraps the 'result' field, so we get the inner value directly
    const validateResult = context.getDependencyResult('validate_request_ts') as Record<
      string,
      unknown
    > | null;

    if (!validateResult) {
      return this.failure(
        'Missing dependency result from validate_request_ts',
        'dependency_error',
        false
      );
    }

    const amount = validateResult.amount as number;
    const requester = validateResult.requester as string;
    const purpose = validateResult.purpose as string;

    // Simulate manager approval (always approves in test)
    return this.success({
      approved: true,
      approval_type: 'manager',
      amount,
      requester,
      purpose,
      approver: 'test_manager@example.com',
      approved_at: new Date().toISOString(),
    });
  }
}

/**
 * Finance review for large amounts.
 */
export class FinanceReviewHandler extends StepHandler {
  static handlerName = 'conditional_approval.step_handlers.FinanceReviewHandler';
  static handlerVersion = '1.0.0';

  async call(context: StepContext): Promise<StepHandlerResult> {
    // getDependencyResult() already unwraps the 'result' field, so we get the inner value directly
    const validateResult = context.getDependencyResult('validate_request_ts') as Record<
      string,
      unknown
    > | null;

    if (!validateResult) {
      return this.failure(
        'Missing dependency result from validate_request_ts',
        'dependency_error',
        false
      );
    }

    const amount = validateResult.amount as number;
    const requester = validateResult.requester as string;
    const purpose = validateResult.purpose as string;

    // Simulate finance review (always approves in test)
    return this.success({
      approved: true,
      approval_type: 'finance',
      amount,
      requester,
      purpose,
      reviewer: 'finance_team@example.com',
      audit_id: `AUDIT-${Date.now()}`,
      reviewed_at: new Date().toISOString(),
    });
  }
}

/**
 * Convergence point: Finalize approval after all required approvals.
 */
export class FinalizeApprovalHandler extends StepHandler {
  static handlerName = 'conditional_approval.step_handlers.FinalizeApprovalHandler';
  static handlerVersion = '1.0.0';

  async call(context: StepContext): Promise<StepHandlerResult> {
    // getDependencyResult() already unwraps the 'result' field, so we get the inner value directly
    // Collect approval results from whichever steps were created
    const autoApproveResult = context.getDependencyResult('auto_approve_ts') as Record<
      string,
      unknown
    > | null;
    const managerApprovalResult = context.getDependencyResult('manager_approval_ts') as Record<
      string,
      unknown
    > | null;
    const financeReviewResult = context.getDependencyResult('finance_review_ts') as Record<
      string,
      unknown
    > | null;

    const approvals: Array<{ type: string; approved: boolean; approver?: string }> = [];

    if (autoApproveResult) {
      approvals.push({
        type: 'automatic',
        approved: autoApproveResult.approved as boolean,
      });
    }

    if (managerApprovalResult) {
      approvals.push({
        type: 'manager',
        approved: managerApprovalResult.approved as boolean,
        approver: managerApprovalResult.approver as string,
      });
    }

    if (financeReviewResult) {
      approvals.push({
        type: 'finance',
        approved: financeReviewResult.approved as boolean,
        approver: financeReviewResult.reviewer as string,
      });
    }

    const allApproved = approvals.every((a) => a.approved);

    return this.success({
      final_approved: allApproved,
      approval_count: approvals.length,
      approvals,
      finalized_at: new Date().toISOString(),
    });
  }
}
