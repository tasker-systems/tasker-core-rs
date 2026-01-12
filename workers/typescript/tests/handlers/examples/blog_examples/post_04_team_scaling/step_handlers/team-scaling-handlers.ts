/**
 * Team Scaling Step Handlers for Blog Post 04.
 *
 * Demonstrates namespace-based workflow organization with two distinct namespaces:
 *
 * Customer Success namespace (process_refund workflow):
 * 1. ValidateRefundRequest: Validate customer refund request details
 * 2. CheckRefundPolicy: Verify request complies with policies
 * 3. GetManagerApproval: Route to manager for approval if needed
 * 4. ExecuteRefundWorkflow: Call payments team's workflow (cross-namespace)
 * 5. UpdateTicketStatus: Update customer support ticket
 *
 * Payments namespace (process_refund workflow):
 * 1. ValidatePaymentEligibility: Check if payment can be refunded
 * 2. ProcessGatewayRefund: Execute refund through payment processor
 * 3. UpdatePaymentRecords: Update internal payment status
 * 4. NotifyCustomer: Send refund confirmation
 *
 * TAS-137 Best Practices Demonstrated:
 * - getInput() for task context access
 * - getInputOr() for task context with defaults
 * - getDependencyResult() for upstream step results
 * - getDependencyField() for nested field extraction
 * - ErrorType.PERMANENT_ERROR for non-recoverable failures
 */

import { StepHandler } from '../../../../../../src/handler/base.js';
import { ErrorType } from '../../../../../../src/types/error-type.js';
import type { StepContext } from '../../../../../../src/types/step-context.js';
import type { StepHandlerResult } from '../../../../../../src/types/step-handler-result.js';

// =============================================================================
// Types
// =============================================================================

interface ValidationResult {
  request_validated: boolean;
  ticket_id: string;
  customer_id: string;
  ticket_status: string;
  customer_tier: string;
  original_purchase_date: string;
  payment_id: string;
}

interface PolicyCheckResult {
  policy_checked: boolean;
  requires_approval: boolean;
  customer_tier: string;
  refund_window_days: number;
  days_since_purchase: number;
  within_refund_window: boolean;
  max_allowed_amount: number;
}

interface ApprovalResult {
  approval_obtained: boolean;
  approval_required: boolean;
  auto_approved: boolean;
  approval_id: string | null;
  manager_id: string | null;
}

interface DelegationResult {
  task_delegated: boolean;
  delegated_task_id: string;
  correlation_id: string;
}

interface PaymentValidationResult {
  payment_validated: boolean;
  payment_id: string;
  refund_amount: number;
  original_amount: number;
  payment_method: string;
  gateway_provider: string;
}

interface RefundResult {
  refund_processed: boolean;
  refund_id: string;
  payment_id: string;
  refund_amount: number;
  gateway_transaction_id: string;
  estimated_arrival: string;
}

// =============================================================================
// Configuration
// =============================================================================

// Refund policy rules by customer tier
const REFUND_POLICIES: Record<
  string,
  { window_days: number; requires_approval: boolean; max_amount: number }
> = {
  standard: { window_days: 30, requires_approval: true, max_amount: 10_000 },
  gold: { window_days: 60, requires_approval: false, max_amount: 50_000 },
  premium: { window_days: 90, requires_approval: false, max_amount: 100_000 },
};

// =============================================================================
// Helper Functions
// =============================================================================

function generateId(prefix: string): string {
  const hex = Math.random().toString(16).substring(2, 14);
  return `${prefix}_${hex}`;
}

function generateUuid(): string {
  return 'xxxxxxxx-xxxx-4xxx-yxxx-xxxxxxxxxxxx'.replace(/[xy]/g, (c) => {
    const r = (Math.random() * 16) | 0;
    const v = c === 'x' ? r : (r & 0x3) | 0x8;
    return v.toString(16);
  });
}

function determineCustomerTier(customerId: string): string {
  if (customerId.toLowerCase().includes('vip') || customerId.toLowerCase().includes('premium')) {
    return 'premium';
  } else if (customerId.toLowerCase().includes('gold')) {
    return 'gold';
  }
  return 'standard';
}

// =============================================================================
// Customer Success Namespace Handlers
// =============================================================================

/**
 * Validate customer refund request details.
 *
 * First step in customer success workflow.
 */
export class ValidateRefundRequestHandler extends StepHandler {
  static handlerName = 'TeamScaling.CustomerSuccess.StepHandlers.ValidateRefundRequestHandler';
  static handlerVersion = '1.0.0';

  async call(context: StepContext): Promise<StepHandlerResult> {
    // TAS-137: Use getInput() for task context access
    const ticketId = context.getInput('ticket_id') as string | undefined;
    const customerId = context.getInput('customer_id') as string | undefined;
    const refundAmount = context.getInput('refund_amount') as number | undefined;
    const _refundReason = context.getInput('refund_reason') as string | undefined;

    // Validate required fields
    const missingFields: string[] = [];
    if (!ticketId) missingFields.push('ticket_id');
    if (!customerId) missingFields.push('customer_id');
    if (!refundAmount) missingFields.push('refund_amount');

    if (missingFields.length > 0) {
      return this.failure(
        `Missing required fields for refund validation: ${missingFields.join(', ')}`,
        ErrorType.PERMANENT_ERROR,
        false
      );
    }

    // After validation, we know these fields exist
    const validTicketId = ticketId as string;
    const validCustomerId = customerId as string;
    const validRefundAmount = refundAmount as number;

    // Simulate customer service system validation
    const serviceResponse = this.simulateCustomerServiceValidation(
      validTicketId,
      validCustomerId,
      validRefundAmount
    );

    // Check request validity
    if (serviceResponse.status === 'closed') {
      return this.failure(
        'Cannot process refund for closed ticket',
        ErrorType.PERMANENT_ERROR,
        false
      );
    }
    if (serviceResponse.status === 'cancelled') {
      return this.failure(
        'Cannot process refund for cancelled ticket',
        ErrorType.PERMANENT_ERROR,
        false
      );
    }
    if (serviceResponse.status === 'duplicate') {
      return this.failure(
        'Cannot process refund for duplicate ticket',
        ErrorType.PERMANENT_ERROR,
        false
      );
    }

    const now = new Date().toISOString();

    return this.success(
      {
        request_validated: true,
        ticket_id: serviceResponse.ticket_id,
        customer_id: serviceResponse.customer_id,
        ticket_status: serviceResponse.status,
        customer_tier: serviceResponse.customer_tier,
        original_purchase_date: serviceResponse.purchase_date,
        payment_id: serviceResponse.payment_id,
        validation_timestamp: now,
        namespace: 'customer_success',
      },
      {
        operation: 'validate_refund_request',
        service: 'customer_service_platform',
        ticket_id: validTicketId,
        customer_tier: serviceResponse.customer_tier,
      }
    );
  }

  private simulateCustomerServiceValidation(
    ticketId: string,
    customerId: string,
    _refundAmount: number
  ): {
    status: string;
    ticket_id: string;
    customer_id: string;
    customer_tier: string;
    purchase_date: string;
    payment_id: string;
  } {
    if (ticketId.includes('ticket_closed')) {
      return {
        status: 'closed',
        ticket_id: ticketId,
        customer_id: customerId,
        customer_tier: 'standard',
        purchase_date: '',
        payment_id: '',
      };
    }
    if (ticketId.includes('ticket_cancelled')) {
      return {
        status: 'cancelled',
        ticket_id: ticketId,
        customer_id: customerId,
        customer_tier: 'standard',
        purchase_date: '',
        payment_id: '',
      };
    }
    if (ticketId.includes('ticket_duplicate')) {
      return {
        status: 'duplicate',
        ticket_id: ticketId,
        customer_id: customerId,
        customer_tier: 'standard',
        purchase_date: '',
        payment_id: '',
      };
    }

    const purchaseDate = new Date(Date.now() - 30 * 24 * 60 * 60 * 1000).toISOString();
    return {
      status: 'open',
      ticket_id: ticketId,
      customer_id: customerId,
      customer_tier: determineCustomerTier(customerId),
      purchase_date: purchaseDate,
      payment_id: generateId('pay'),
    };
  }
}

/**
 * Check if refund request complies with policy rules.
 *
 * Depends on validate_refund_request.
 */
export class CheckRefundPolicyHandler extends StepHandler {
  static handlerName = 'TeamScaling.CustomerSuccess.StepHandlers.CheckRefundPolicyHandler';
  static handlerVersion = '1.0.0';

  async call(context: StepContext): Promise<StepHandlerResult> {
    // TAS-137: Validate dependency result exists
    const validationResult = context.getDependencyResult(
      'validate_refund_request'
    ) as ValidationResult | null;

    if (!validationResult?.request_validated) {
      return this.failure(
        'Request validation must be completed before policy check',
        ErrorType.PERMANENT_ERROR,
        false
      );
    }

    // TAS-137: Use getDependencyField() for nested extraction
    const customerTier =
      (context.getDependencyField('validate_refund_request', 'customer_tier') as string) ||
      'standard';
    const purchaseDateStr = context.getDependencyField(
      'validate_refund_request',
      'original_purchase_date'
    ) as string;
    const refundAmount = context.getInput('refund_amount') as number;

    // Check policy compliance
    const policy = REFUND_POLICIES[customerTier] || REFUND_POLICIES.standard;
    const purchaseDate = new Date(purchaseDateStr);
    const daysSincePurchase = Math.floor(
      (Date.now() - purchaseDate.getTime()) / (24 * 60 * 60 * 1000)
    );
    const withinWindow = daysSincePurchase <= policy.window_days;
    const withinAmountLimit = refundAmount <= policy.max_amount;

    if (!withinWindow) {
      return this.failure(
        `Refund request outside policy window: ${daysSincePurchase} days (max: ${policy.window_days} days)`,
        ErrorType.PERMANENT_ERROR,
        false
      );
    }

    if (!withinAmountLimit) {
      return this.failure(
        `Refund amount exceeds policy limit: $${(refundAmount / 100).toFixed(2)} (max: $${(policy.max_amount / 100).toFixed(2)})`,
        ErrorType.PERMANENT_ERROR,
        false
      );
    }

    const now = new Date().toISOString();

    return this.success(
      {
        policy_checked: true,
        policy_compliant: true,
        customer_tier: customerTier,
        refund_window_days: policy.window_days,
        days_since_purchase: daysSincePurchase,
        within_refund_window: withinWindow,
        requires_approval: policy.requires_approval,
        max_allowed_amount: policy.max_amount,
        policy_checked_at: now,
        namespace: 'customer_success',
      },
      {
        operation: 'check_refund_policy',
        service: 'policy_engine',
        customer_tier: customerTier,
        requires_approval: policy.requires_approval,
      }
    );
  }
}

/**
 * Get manager approval for refund if required.
 *
 * Depends on check_refund_policy.
 */
export class GetManagerApprovalHandler extends StepHandler {
  static handlerName = 'TeamScaling.CustomerSuccess.StepHandlers.GetManagerApprovalHandler';
  static handlerVersion = '1.0.0';

  async call(context: StepContext): Promise<StepHandlerResult> {
    // TAS-137: Validate dependency result exists
    const policyResult = context.getDependencyResult(
      'check_refund_policy'
    ) as PolicyCheckResult | null;

    if (!policyResult?.policy_checked) {
      return this.failure(
        'Policy check must be completed before approval',
        ErrorType.PERMANENT_ERROR,
        false
      );
    }

    const requiresApproval = context.getDependencyField(
      'check_refund_policy',
      'requires_approval'
    ) as boolean;
    const customerTier = context.getDependencyField(
      'check_refund_policy',
      'customer_tier'
    ) as string;
    const ticketId = context.getDependencyField('validate_refund_request', 'ticket_id') as string;
    const customerId = context.getDependencyField(
      'validate_refund_request',
      'customer_id'
    ) as string;

    const now = new Date().toISOString();

    if (requiresApproval) {
      // Simulate approval scenarios
      if (ticketId.includes('ticket_denied')) {
        return this.failure('Manager denied refund request', ErrorType.PERMANENT_ERROR, false);
      }
      if (ticketId.includes('ticket_pending')) {
        return this.failure('Waiting for manager approval', ErrorType.RETRYABLE_ERROR, true);
      }

      // Approval granted
      return this.success(
        {
          approval_obtained: true,
          approval_required: true,
          auto_approved: false,
          approval_id: generateId('appr'),
          manager_id: `mgr_${Math.floor(Math.random() * 5) + 1}`,
          manager_notes: `Approved refund request for customer ${customerId}`,
          approved_at: now,
          namespace: 'customer_success',
        },
        {
          operation: 'get_manager_approval',
          service: 'approval_portal',
          approval_required: true,
        }
      );
    } else {
      // Auto-approved
      return this.success(
        {
          approval_obtained: true,
          approval_required: false,
          auto_approved: true,
          approval_id: null,
          manager_id: null,
          manager_notes: `Auto-approved for customer tier ${customerTier}`,
          approved_at: now,
          namespace: 'customer_success',
        },
        {
          operation: 'get_manager_approval',
          service: 'approval_portal',
          approval_required: false,
          auto_approved: true,
        }
      );
    }
  }
}

/**
 * Execute cross-namespace refund workflow delegation.
 *
 * Key demonstration of cross-namespace workflow coordination.
 */
export class ExecuteRefundWorkflowHandler extends StepHandler {
  static handlerName = 'TeamScaling.CustomerSuccess.StepHandlers.ExecuteRefundWorkflowHandler';
  static handlerVersion = '1.0.0';

  async call(context: StepContext): Promise<StepHandlerResult> {
    // TAS-137: Validate approval dependency exists
    const approvalResult = context.getDependencyResult(
      'get_manager_approval'
    ) as ApprovalResult | null;

    if (!approvalResult?.approval_obtained) {
      return this.failure(
        'Manager approval must be obtained before executing refund',
        ErrorType.PERMANENT_ERROR,
        false
      );
    }

    const paymentId = context.getDependencyField('validate_refund_request', 'payment_id') as string;
    if (!paymentId) {
      return this.failure(
        'Payment ID not found in validation results',
        ErrorType.PERMANENT_ERROR,
        false
      );
    }

    const _approvalId = context.getDependencyField('get_manager_approval', 'approval_id') as
      | string
      | null;
    const _refundAmount = context.getInput('refund_amount') as number;
    const _refundReason = context.getInputOr('refund_reason', 'customer_request') as string;
    const _customerEmail = context.getInputOr('customer_email', 'customer@example.com') as string;
    const _ticketId = context.getInput('ticket_id') as string;
    const correlationId =
      (context.getInput('correlation_id') as string) || `cs-${generateId('corr')}`;

    // Simulate task creation in payments namespace
    const taskId = `task_${generateUuid()}`;
    const now = new Date().toISOString();

    return this.success(
      {
        task_delegated: true,
        target_namespace: 'payments',
        target_workflow: 'process_refund',
        delegated_task_id: taskId,
        delegated_task_status: 'created',
        delegation_timestamp: now,
        correlation_id: correlationId,
        namespace: 'customer_success',
      },
      {
        operation: 'execute_refund_workflow',
        service: 'task_delegation',
        target_namespace: 'payments',
        target_workflow: 'process_refund',
        delegated_task_id: taskId,
      }
    );
  }
}

/**
 * Update customer support ticket status.
 *
 * Final step in customer success workflow.
 */
export class UpdateTicketStatusHandler extends StepHandler {
  static handlerName = 'TeamScaling.CustomerSuccess.StepHandlers.UpdateTicketStatusHandler';
  static handlerVersion = '1.0.0';

  async call(context: StepContext): Promise<StepHandlerResult> {
    // TAS-137: Validate delegation dependency exists
    const delegationResult = context.getDependencyResult(
      'execute_refund_workflow'
    ) as DelegationResult | null;

    if (!delegationResult?.task_delegated) {
      return this.failure(
        'Refund workflow must be executed before updating ticket',
        ErrorType.PERMANENT_ERROR,
        false
      );
    }

    const ticketId = context.getDependencyField('validate_refund_request', 'ticket_id') as string;
    const _customerId = context.getDependencyField(
      'validate_refund_request',
      'customer_id'
    ) as string;
    const delegatedTaskId = context.getDependencyField(
      'execute_refund_workflow',
      'delegated_task_id'
    ) as string;
    const correlationId = context.getDependencyField(
      'execute_refund_workflow',
      'correlation_id'
    ) as string;
    const refundAmount = context.getInput('refund_amount') as number;

    // Simulate update scenarios
    if (ticketId.includes('ticket_locked')) {
      return this.failure(
        'Ticket locked by another agent, will retry',
        ErrorType.RETRYABLE_ERROR,
        true
      );
    }
    if (ticketId.includes('ticket_update_error')) {
      return this.failure('System error updating ticket', ErrorType.RETRYABLE_ERROR, true);
    }

    const now = new Date().toISOString();

    return this.success(
      {
        ticket_updated: true,
        ticket_id: ticketId,
        previous_status: 'in_progress',
        new_status: 'resolved',
        resolution_note: `Refund of $${(refundAmount / 100).toFixed(2)} processed successfully. Delegated task ID: ${delegatedTaskId}. Correlation ID: ${correlationId}`,
        updated_at: now,
        refund_completed: true,
        delegated_task_id: delegatedTaskId,
        namespace: 'customer_success',
      },
      {
        operation: 'update_ticket_status',
        service: 'customer_service_platform',
        ticket_id: ticketId,
        new_status: 'resolved',
      }
    );
  }
}

// =============================================================================
// Payments Namespace Handlers
// =============================================================================

/**
 * Validate payment eligibility for refund.
 *
 * First step in payments workflow.
 */
export class ValidatePaymentEligibilityHandler extends StepHandler {
  static handlerName = 'TeamScaling.Payments.StepHandlers.ValidatePaymentEligibilityHandler';
  static handlerVersion = '1.0.0';

  async call(context: StepContext): Promise<StepHandlerResult> {
    const paymentId = context.getInput('payment_id') as string | undefined;
    const refundAmount = context.getInput('refund_amount') as number | undefined;
    const _partialRefund = context.getInputOr('partial_refund', false) as boolean;

    // Validate required fields
    const missingFields: string[] = [];
    if (!paymentId) missingFields.push('payment_id');
    if (!refundAmount) missingFields.push('refund_amount');

    if (missingFields.length > 0) {
      return this.failure(
        `Missing required fields for payment validation: ${missingFields.join(', ')}`,
        ErrorType.PERMANENT_ERROR,
        false
      );
    }

    // After validation, we know these fields exist
    const validPaymentId = paymentId as string;
    const validRefundAmount = refundAmount as number;

    if (validRefundAmount <= 0) {
      return this.failure(
        `Refund amount must be positive, got: ${validRefundAmount}`,
        ErrorType.PERMANENT_ERROR,
        false
      );
    }

    if (!/^pay_[a-zA-Z0-9_]+$/.test(validPaymentId)) {
      return this.failure(
        `Invalid payment ID format: ${validPaymentId}`,
        ErrorType.PERMANENT_ERROR,
        false
      );
    }

    // Simulate validation scenarios
    if (validPaymentId.includes('pay_test_insufficient')) {
      return this.failure(
        'Insufficient funds available for refund',
        ErrorType.PERMANENT_ERROR,
        false
      );
    }
    if (validPaymentId.includes('pay_test_processing')) {
      return this.failure(
        'Payment is still processing, cannot refund yet',
        ErrorType.RETRYABLE_ERROR,
        true
      );
    }
    if (validPaymentId.includes('pay_test_ineligible')) {
      return this.failure(
        'Payment is not eligible for refund: past refund window',
        ErrorType.PERMANENT_ERROR,
        false
      );
    }

    const now = new Date().toISOString();
    const _transactionDate = new Date(Date.now() - 5 * 24 * 60 * 60 * 1000).toISOString();

    return this.success(
      {
        payment_validated: true,
        payment_id: validPaymentId,
        original_amount: validRefundAmount + 1000,
        refund_amount: validRefundAmount,
        payment_method: 'credit_card',
        gateway_provider: 'MockPaymentGateway',
        eligibility_status: 'eligible',
        validation_timestamp: now,
        namespace: 'payments',
      },
      {
        operation: 'validate_payment_eligibility',
        service: 'payment_gateway',
        payment_id: validPaymentId,
        gateway_provider: 'MockPaymentGateway',
      }
    );
  }
}

/**
 * Process refund through payment gateway.
 *
 * Depends on validate_payment_eligibility.
 */
export class ProcessGatewayRefundHandler extends StepHandler {
  static handlerName = 'TeamScaling.Payments.StepHandlers.ProcessGatewayRefundHandler';
  static handlerVersion = '1.0.0';

  async call(context: StepContext): Promise<StepHandlerResult> {
    const validationResult = context.getDependencyResult(
      'validate_payment_eligibility'
    ) as PaymentValidationResult | null;

    if (!validationResult?.payment_validated) {
      return this.failure(
        'Payment validation must be completed before processing refund',
        ErrorType.PERMANENT_ERROR,
        false
      );
    }

    const paymentId = context.getDependencyField(
      'validate_payment_eligibility',
      'payment_id'
    ) as string;
    const refundAmount = context.getDependencyField(
      'validate_payment_eligibility',
      'refund_amount'
    ) as number;

    // Simulate gateway scenarios
    if (paymentId.includes('pay_test_gateway_timeout')) {
      return this.failure('Gateway timeout, will retry', ErrorType.RETRYABLE_ERROR, true);
    }
    if (paymentId.includes('pay_test_gateway_error')) {
      return this.failure('Gateway refund failed: Gateway error', ErrorType.PERMANENT_ERROR, false);
    }
    if (paymentId.includes('pay_test_rate_limit')) {
      return this.failure('Gateway rate limited, will retry', ErrorType.RETRYABLE_ERROR, true);
    }

    const now = new Date();
    const estimatedArrival = new Date(now.getTime() + 5 * 24 * 60 * 60 * 1000).toISOString();

    return this.success(
      {
        refund_processed: true,
        refund_id: generateId('rfnd'),
        payment_id: paymentId,
        refund_amount: refundAmount,
        refund_status: 'processed',
        gateway_transaction_id: generateId('gtx'),
        gateway_provider: 'MockPaymentGateway',
        processed_at: now.toISOString(),
        estimated_arrival: estimatedArrival,
        namespace: 'payments',
      },
      {
        operation: 'process_gateway_refund',
        service: 'payment_gateway',
        gateway_provider: 'MockPaymentGateway',
      }
    );
  }
}

/**
 * Update internal payment records after refund.
 *
 * Depends on process_gateway_refund.
 */
export class UpdatePaymentRecordsHandler extends StepHandler {
  static handlerName = 'TeamScaling.Payments.StepHandlers.UpdatePaymentRecordsHandler';
  static handlerVersion = '1.0.0';

  async call(context: StepContext): Promise<StepHandlerResult> {
    const refundResult = context.getDependencyResult(
      'process_gateway_refund'
    ) as RefundResult | null;

    if (!refundResult?.refund_processed) {
      return this.failure(
        'Gateway refund must be completed before updating records',
        ErrorType.PERMANENT_ERROR,
        false
      );
    }

    const paymentId = context.getDependencyField('process_gateway_refund', 'payment_id') as string;
    const refundId = context.getDependencyField('process_gateway_refund', 'refund_id') as string;
    const _refundAmount = context.getDependencyField(
      'process_gateway_refund',
      'refund_amount'
    ) as number;
    const _gatewayTransactionId = context.getDependencyField(
      'process_gateway_refund',
      'gateway_transaction_id'
    ) as string;
    const _refundReason = context.getInputOr('refund_reason', 'customer_request') as string;

    // Simulate update scenarios
    if (paymentId.includes('pay_test_record_lock')) {
      return this.failure('Payment record locked, will retry', ErrorType.RETRYABLE_ERROR, true);
    }
    if (paymentId.includes('pay_test_record_error')) {
      return this.failure('Record update failed: Database error', ErrorType.RETRYABLE_ERROR, true);
    }

    const now = new Date().toISOString();

    return this.success(
      {
        records_updated: true,
        payment_id: paymentId,
        refund_id: refundId,
        record_id: generateId('rec'),
        payment_status: 'refunded',
        refund_status: 'completed',
        history_entries_created: 2,
        updated_at: now,
        namespace: 'payments',
      },
      {
        operation: 'update_payment_records',
        service: 'payment_record_system',
        payment_id: paymentId,
      }
    );
  }
}

/**
 * Send refund confirmation notification to customer.
 *
 * Final step in payments workflow.
 */
export class NotifyCustomerHandler extends StepHandler {
  static handlerName = 'TeamScaling.Payments.StepHandlers.NotifyCustomerHandler';
  static handlerVersion = '1.0.0';

  async call(context: StepContext): Promise<StepHandlerResult> {
    const refundResult = context.getDependencyResult(
      'process_gateway_refund'
    ) as RefundResult | null;

    if (!refundResult?.refund_processed) {
      return this.failure(
        'Refund must be processed before sending notification',
        ErrorType.PERMANENT_ERROR,
        false
      );
    }

    const customerEmail = context.getInput('customer_email') as string | undefined;
    if (!customerEmail) {
      return this.failure(
        'Customer email is required for notification',
        ErrorType.PERMANENT_ERROR,
        false
      );
    }

    if (!/^[^@\s]+@[^@\s]+$/.test(customerEmail)) {
      return this.failure(
        `Invalid customer email format: ${customerEmail}`,
        ErrorType.PERMANENT_ERROR,
        false
      );
    }

    const refundId = context.getDependencyField('process_gateway_refund', 'refund_id') as string;
    const refundAmount = context.getDependencyField(
      'process_gateway_refund',
      'refund_amount'
    ) as number;

    // Simulate notification scenarios
    if (customerEmail.includes('@test_bounce')) {
      return this.failure('Customer email bounced', ErrorType.PERMANENT_ERROR, false);
    }
    if (customerEmail.includes('@test_invalid')) {
      return this.failure('Invalid customer email address', ErrorType.PERMANENT_ERROR, false);
    }
    if (customerEmail.includes('@test_rate_limit')) {
      return this.failure(
        'Email service rate limited, will retry',
        ErrorType.RETRYABLE_ERROR,
        true
      );
    }

    const now = new Date().toISOString();

    return this.success(
      {
        notification_sent: true,
        customer_email: customerEmail,
        message_id: generateId('msg'),
        notification_type: 'refund_confirmation',
        sent_at: now,
        delivery_status: 'delivered',
        refund_id: refundId,
        refund_amount: refundAmount,
        namespace: 'payments',
      },
      {
        operation: 'notify_customer',
        service: 'email_service',
        customer_email: customerEmail,
      }
    );
  }
}
