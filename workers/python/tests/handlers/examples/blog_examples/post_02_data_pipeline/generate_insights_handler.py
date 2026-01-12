"""Generate insights handler for data pipeline workflow.

This handler is the final step in the DAG, generating business insights
from aggregated metrics.

TAS-137 Best Practices Demonstrated:
- get_dependency_result() for upstream step results (auto-unwraps)
- Final DAG node processing
"""

from __future__ import annotations

import logging
from datetime import datetime, timezone
from typing import TYPE_CHECKING, Any

from tasker_core import StepHandler, StepHandlerResult
from tasker_core.errors import PermanentError

if TYPE_CHECKING:
    from tasker_core import StepContext

logger = logging.getLogger(__name__)


class GenerateInsightsHandler(StepHandler):
    """Generate business insights from aggregated metrics.

    This handler is the final step in the DAG workflow.

    Dependencies:
        - aggregate_metrics

    Output:
        insights: list[dict] - Business insights with recommendations
        health_score: dict - Overall business health score
        total_metrics_analyzed: int - Number of metrics analyzed
        pipeline_complete: bool - Pipeline completion flag
    """

    handler_name = "data_pipeline.step_handlers.GenerateInsightsHandler"
    handler_version = "1.0.0"

    def call(self, context: StepContext) -> StepHandlerResult:
        """Generate business insights from aggregated metrics."""
        logger.info(
            "GenerateInsightsHandler: Generating business insights - task_uuid=%s",
            context.task_uuid,
        )

        # TAS-137: Get aggregated metrics from prior step
        metrics = context.get_dependency_result("aggregate_metrics")
        if not metrics:
            raise PermanentError(
                message="Aggregated metrics not found",
                error_code="MISSING_AGGREGATE_RESULTS",
            )

        # Generate insights
        insights_result = self._generate_business_insights(metrics)

        logger.info(
            "GenerateInsightsHandler: Generated %d business insights",
            len(insights_result["insights"]),
        )
        for idx, insight in enumerate(insights_result["insights"], 1):
            logger.info("   %d. %s: %s", idx, insight["category"], insight["finding"])

        return StepHandlerResult.success(
            insights_result,
            metadata={
                "operation": "generate_insights",
                "source_step": "aggregate_metrics",
                "insights_generated": len(insights_result["insights"]),
                "generated_at": datetime.now(timezone.utc).isoformat(),
            },
        )

    def _generate_business_insights(self, metrics: dict[str, Any]) -> dict[str, Any]:
        """Generate business insights from aggregated metrics."""
        insights = []

        # Revenue insights
        revenue = metrics.get("total_revenue", 0)
        customers = metrics.get("total_customers", 0)
        revenue_per_customer = metrics.get("revenue_per_customer", 0)

        if revenue > 0:
            recommendation = (
                "Consider upselling strategies"
                if revenue_per_customer < 500
                else "Customer spend is healthy"
            )
            insights.append(
                {
                    "category": "Revenue",
                    "finding": f"Total revenue of ${revenue} with {customers} customers",
                    "metric": revenue_per_customer,
                    "recommendation": recommendation,
                }
            )

        # Inventory insights
        inventory_alerts = metrics.get("inventory_reorder_alerts", 0)
        if inventory_alerts > 0:
            insights.append(
                {
                    "category": "Inventory",
                    "finding": f"{inventory_alerts} products need reordering",
                    "metric": inventory_alerts,
                    "recommendation": "Review reorder points and place purchase orders",
                }
            )
        else:
            insights.append(
                {
                    "category": "Inventory",
                    "finding": "All products above reorder points",
                    "metric": 0,
                    "recommendation": "Inventory levels are healthy",
                }
            )

        # Customer insights
        total_ltv = metrics.get("total_customer_lifetime_value", 0)
        avg_ltv = total_ltv / customers if customers > 0 else 0

        recommendation = (
            "Focus on retention programs" if avg_ltv > 3000 else "Increase customer engagement"
        )
        insights.append(
            {
                "category": "Customer Value",
                "finding": f"Average customer lifetime value: ${avg_ltv:.2f}",
                "metric": avg_ltv,
                "recommendation": recommendation,
            }
        )

        # Business health score
        health_score = self._calculate_health_score(revenue_per_customer, inventory_alerts, avg_ltv)

        return {
            "insights": insights,
            "health_score": health_score,
            "total_metrics_analyzed": len(metrics.keys()),
            "pipeline_complete": True,
            "generated_at": datetime.now(timezone.utc).isoformat(),
        }

    def _calculate_health_score(
        self,
        revenue_per_customer: float,
        inventory_alerts: int,
        avg_ltv: float,
    ) -> dict[str, Any]:
        """Calculate overall business health score."""
        score = 0
        if revenue_per_customer > 500:
            score += 40  # Revenue health
        if inventory_alerts == 0:
            score += 30  # Inventory health
        if avg_ltv > 3000:
            score += 30  # Customer health

        return {
            "score": score,
            "max_score": 100,
            "rating": self._rating_from_score(score),
        }

    def _rating_from_score(self, score: int) -> str:
        """Convert numeric score to rating string."""
        if score >= 80:
            return "Excellent"
        elif score >= 60:
            return "Good"
        elif score >= 40:
            return "Fair"
        else:
            return "Needs Improvement"
