/**
 * Blog Post 04: Team Scaling - Namespace-based workflow organization.
 *
 * This post demonstrates how namespaces enable team autonomy by allowing
 * multiple teams to use the same logical workflow names without conflicts.
 *
 * Two namespaces demonstrated:
 * - customer_success: Customer-facing refund requests with approval workflow
 * - payments: Direct payment gateway refunds
 */

export {
  CheckRefundPolicyHandler,
  ExecuteRefundWorkflowHandler,
  GetManagerApprovalHandler,
  NotifyCustomerHandler,
  ProcessGatewayRefundHandler,
  UpdatePaymentRecordsHandler,
  UpdateTicketStatusHandler,
  // Payments namespace handlers
  ValidatePaymentEligibilityHandler,
  // Customer Success namespace handlers
  ValidateRefundRequestHandler,
} from './step_handlers/team-scaling-handlers.js';
