"""Conditional approval workflow handlers.

This package contains step handlers for the conditional approval workflow
demonstrating decision point routing with dynamic step creation.

Workflow Pattern:
1. validate_request: Validate approval request data
2. routing_decision: DECISION POINT - route based on amount
   - Creates different approval steps dynamically:
     * < $1,000: auto_approve
     * $1,000-$4,999: manager_approval
     * >= $5,000: manager_approval + finance_review
3. Dynamic Steps (created by decision point):
   - auto_approve: Auto-approval for small amounts
   - manager_approval: Manager review
   - finance_review: Finance review for large amounts
4. finalize_approval: Convergence step processing all approvals
"""

from __future__ import annotations

from .step_handlers import (
    AutoApproveHandler,
    FinalizeApprovalHandler,
    FinanceReviewHandler,
    ManagerApprovalHandler,
    RoutingDecisionHandler,
    ValidateRequestHandler,
)

__all__ = [
    "ValidateRequestHandler",
    "RoutingDecisionHandler",
    "AutoApproveHandler",
    "ManagerApprovalHandler",
    "FinanceReviewHandler",
    "FinalizeApprovalHandler",
]
