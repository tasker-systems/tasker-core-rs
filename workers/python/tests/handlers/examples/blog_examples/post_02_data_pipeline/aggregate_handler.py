"""Aggregate metrics handler for data pipeline workflow.

This handler demonstrates DAG convergence - it depends on all 3 transform
steps and aggregates their results into unified business metrics.

TAS-137 Best Practices Demonstrated:
- get_dependency_result() for multiple upstream step results
- DAG convergence pattern with proper validation
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


class AggregateMetricsHandler(StepHandler):
    """Aggregate metrics from all transformed data sources.

    This handler demonstrates DAG convergence - it depends on all 3 transform
    steps and combines their results.

    Dependencies:
        - transform_sales
        - transform_inventory
        - transform_customers

    Output:
        total_revenue: float - Total revenue from sales
        total_inventory_quantity: int - Total inventory on hand
        total_customers: int - Total customer count
        revenue_per_customer: float - Average revenue per customer
        inventory_turnover_indicator: float - Revenue / inventory ratio
    """

    handler_name = "data_pipeline.step_handlers.AggregateMetricsHandler"
    handler_version = "1.0.0"

    def call(self, context: StepContext) -> StepHandlerResult:
        """Aggregate metrics from all transform steps."""
        logger.info(
            "AggregateMetricsHandler: Aggregating metrics from all sources - task_uuid=%s",
            context.task_uuid,
        )

        # TAS-137: Get results from all 3 transform steps (DAG convergence)
        sales_data = context.get_dependency_result("transform_sales")
        inventory_data = context.get_dependency_result("transform_inventory")
        customer_data = context.get_dependency_result("transform_customers")

        # Validate all sources present
        self._validate_all_sources_present(sales_data, inventory_data, customer_data)

        # Aggregate across all sources
        aggregated = self._aggregate_all_sources(sales_data, inventory_data, customer_data)

        logger.info("AggregateMetricsHandler: Aggregated metrics from 3 sources")
        logger.info("   Total revenue: $%.2f", aggregated["total_revenue"])
        logger.info("   Total inventory: %d", aggregated["total_inventory_quantity"])
        logger.info("   Total customers: %d", aggregated["total_customers"])

        return StepHandlerResult.success(
            aggregated,
            metadata={
                "operation": "aggregate_metrics",
                "sources": ["transform_sales", "transform_inventory", "transform_customers"],
                "sources_aggregated": 3,
                "aggregated_at": datetime.now(timezone.utc).isoformat(),
            },
        )

    def _validate_all_sources_present(
        self,
        sales: dict[str, Any] | None,
        inventory: dict[str, Any] | None,
        customers: dict[str, Any] | None,
    ) -> None:
        """Validate that all transform results are present."""
        missing = []
        if not sales:
            missing.append("transform_sales")
        if not inventory:
            missing.append("transform_inventory")
        if not customers:
            missing.append("transform_customers")

        if missing:
            raise PermanentError(
                message=f"Missing transform results: {', '.join(missing)}",
                error_code="MISSING_TRANSFORM_RESULTS",
            )

    def _aggregate_all_sources(
        self,
        sales_data: dict[str, Any],
        inventory_data: dict[str, Any],
        customer_data: dict[str, Any],
    ) -> dict[str, Any]:
        """Aggregate metrics from all data sources."""
        # Extract metrics from each source
        total_revenue = sales_data.get("total_revenue", 0)
        sales_record_count = sales_data.get("record_count", 0)

        total_inventory = inventory_data.get("total_quantity_on_hand", 0)
        reorder_alerts = inventory_data.get("reorder_alerts", 0)

        total_customers = customer_data.get("record_count", 0)
        total_ltv = customer_data.get("total_lifetime_value", 0)

        # Calculate cross-source metrics
        revenue_per_customer = total_revenue / total_customers if total_customers > 0 else 0
        inventory_turnover = total_revenue / total_inventory if total_inventory > 0 else 0

        return {
            "total_revenue": total_revenue,
            "total_inventory_quantity": total_inventory,
            "total_customers": total_customers,
            "total_customer_lifetime_value": total_ltv,
            "sales_transactions": sales_record_count,
            "inventory_reorder_alerts": reorder_alerts,
            "revenue_per_customer": round(revenue_per_customer, 2),
            "inventory_turnover_indicator": round(inventory_turnover, 4),
            "aggregation_complete": True,
            "sources_included": 3,
        }
