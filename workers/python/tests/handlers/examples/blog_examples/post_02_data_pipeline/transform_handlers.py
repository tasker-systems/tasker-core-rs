"""Transform handlers for data pipeline workflow.

These handlers run after their corresponding extract handlers and transform
the raw data into analytics-ready formats.

TAS-137 Best Practices Demonstrated:
- get_dependency_result() for upstream step results (auto-unwraps)
- Data transformation logic with proper error handling
"""

from __future__ import annotations

import logging
from collections import defaultdict
from datetime import datetime, timezone
from typing import TYPE_CHECKING, Any

from tasker_core import StepHandler, StepHandlerResult
from tasker_core.errors import PermanentError

if TYPE_CHECKING:
    from tasker_core import StepContext

logger = logging.getLogger(__name__)


# =============================================================================
# Transform Sales Handler
# =============================================================================


class TransformSalesHandler(StepHandler):
    """Transform sales data for analytics.

    Dependencies: extract_sales_data

    Input (from dependency):
        extract_sales_data.records: list[dict] - Raw sales records

    Output:
        record_count: int - Number of records processed
        daily_sales: dict - Sales aggregated by date
        product_sales: dict - Sales aggregated by product
        total_revenue: float - Total revenue
    """

    handler_name = "data_pipeline.step_handlers.TransformSalesHandler"
    handler_version = "1.0.0"

    def call(self, context: StepContext) -> StepHandlerResult:
        """Transform extracted sales data for analytics."""
        logger.info(
            "TransformSalesHandler: Transforming sales data - task_uuid=%s",
            context.task_uuid,
        )

        # TAS-137: Use get_dependency_result() for upstream step results
        extract_results = context.get_dependency_result("extract_sales_data")
        if not extract_results:
            raise PermanentError(
                message="Sales extraction results not found",
                error_code="MISSING_EXTRACT_RESULTS",
            )

        # Transform the data
        transformed = self._transform_sales_data(extract_results)

        logger.info(
            "TransformSalesHandler: Transformed %d sales records",
            transformed["record_count"],
        )

        return StepHandlerResult.success(
            transformed,
            metadata={
                "operation": "transform_sales",
                "source_step": "extract_sales_data",
                "transformation_applied": True,
                "record_count": transformed["record_count"],
                "transformed_at": datetime.now(timezone.utc).isoformat(),
            },
        )

    def _transform_sales_data(self, extract_results: dict[str, Any]) -> dict[str, Any]:
        """Transform raw sales data into analytics format."""
        raw_records = extract_results.get("records", [])

        # Group by date for daily sales
        daily_groups: dict[str, list[dict[str, Any]]] = defaultdict(list)
        for record in raw_records:
            daily_groups[record["date"]].append(record)

        daily_sales = {}
        for date, day_records in daily_groups.items():
            total = sum(r["amount"] for r in day_records)
            daily_sales[date] = {
                "total_amount": total,
                "order_count": len(day_records),
                "avg_order_value": total / len(day_records),
            }

        # Group by product for product sales
        product_groups: dict[str, list[dict[str, Any]]] = defaultdict(list)
        for record in raw_records:
            product_groups[record["product_id"]].append(record)

        product_sales = {}
        for product_id, product_records in product_groups.items():
            product_sales[product_id] = {
                "total_quantity": sum(r["quantity"] for r in product_records),
                "total_revenue": sum(r["amount"] for r in product_records),
                "order_count": len(product_records),
            }

        total_revenue = sum(r["amount"] for r in raw_records)

        return {
            "record_count": len(raw_records),
            "daily_sales": daily_sales,
            "product_sales": product_sales,
            "total_revenue": total_revenue,
            "transformation_type": "sales_analytics",
            "source": "extract_sales_data",
        }


# =============================================================================
# Transform Inventory Handler
# =============================================================================


class TransformInventoryHandler(StepHandler):
    """Transform inventory data for analytics.

    Dependencies: extract_inventory_data

    Input (from dependency):
        extract_inventory_data.records: list[dict] - Raw inventory records

    Output:
        record_count: int - Number of records processed
        warehouse_summary: dict - Inventory by warehouse
        product_inventory: dict - Inventory by product
        reorder_alerts: int - Products needing reorder
    """

    handler_name = "data_pipeline.step_handlers.TransformInventoryHandler"
    handler_version = "1.0.0"

    def call(self, context: StepContext) -> StepHandlerResult:
        """Transform extracted inventory data for analytics."""
        logger.info(
            "TransformInventoryHandler: Transforming inventory data - task_uuid=%s",
            context.task_uuid,
        )

        # TAS-137: Use get_dependency_result() for upstream step results
        extract_results = context.get_dependency_result("extract_inventory_data")
        if not extract_results:
            raise PermanentError(
                message="Inventory extraction results not found",
                error_code="MISSING_EXTRACT_RESULTS",
            )

        # Transform the data
        transformed = self._transform_inventory_data(extract_results)

        logger.info(
            "TransformInventoryHandler: Transformed %d inventory records",
            transformed["record_count"],
        )

        return StepHandlerResult.success(
            transformed,
            metadata={
                "operation": "transform_inventory",
                "source_step": "extract_inventory_data",
                "transformation_applied": True,
                "record_count": transformed["record_count"],
                "transformed_at": datetime.now(timezone.utc).isoformat(),
            },
        )

    def _transform_inventory_data(self, extract_results: dict[str, Any]) -> dict[str, Any]:
        """Transform raw inventory data into analytics format."""
        raw_records = extract_results.get("records", [])

        # Group by warehouse
        warehouse_groups: dict[str, list[dict[str, Any]]] = defaultdict(list)
        for record in raw_records:
            warehouse_groups[record["warehouse"]].append(record)

        warehouse_summary = {}
        for warehouse, wh_records in warehouse_groups.items():
            reorder_count = sum(
                1 for r in wh_records if r["quantity_on_hand"] <= r["reorder_point"]
            )
            warehouse_summary[warehouse] = {
                "total_quantity": sum(r["quantity_on_hand"] for r in wh_records),
                "product_count": len({r["product_id"] for r in wh_records}),
                "reorder_alerts": reorder_count,
            }

        # Group by product
        product_groups: dict[str, list[dict[str, Any]]] = defaultdict(list)
        for record in raw_records:
            product_groups[record["product_id"]].append(record)

        product_inventory = {}
        for product_id, product_records in product_groups.items():
            total_qty = sum(r["quantity_on_hand"] for r in product_records)
            total_reorder = sum(r["reorder_point"] for r in product_records)
            product_inventory[product_id] = {
                "total_quantity": total_qty,
                "warehouse_count": len({r["warehouse"] for r in product_records}),
                "needs_reorder": total_qty < total_reorder,
            }

        total_on_hand = sum(r["quantity_on_hand"] for r in raw_records)
        reorder_alerts = sum(1 for data in product_inventory.values() if data["needs_reorder"])

        return {
            "record_count": len(raw_records),
            "warehouse_summary": warehouse_summary,
            "product_inventory": product_inventory,
            "total_quantity_on_hand": total_on_hand,
            "reorder_alerts": reorder_alerts,
            "transformation_type": "inventory_analytics",
            "source": "extract_inventory_data",
        }


# =============================================================================
# Transform Customers Handler
# =============================================================================


class TransformCustomersHandler(StepHandler):
    """Transform customer data for analytics.

    Dependencies: extract_customer_data

    Input (from dependency):
        extract_customer_data.records: list[dict] - Raw customer records

    Output:
        record_count: int - Number of records processed
        tier_analysis: dict - Analysis by customer tier
        value_segments: dict - Customer value segmentation
        total_lifetime_value: float - Total LTV across all customers
    """

    handler_name = "data_pipeline.step_handlers.TransformCustomersHandler"
    handler_version = "1.0.0"

    def call(self, context: StepContext) -> StepHandlerResult:
        """Transform extracted customer data for analytics."""
        logger.info(
            "TransformCustomersHandler: Transforming customer data - task_uuid=%s",
            context.task_uuid,
        )

        # TAS-137: Use get_dependency_result() for upstream step results
        extract_results = context.get_dependency_result("extract_customer_data")
        if not extract_results:
            raise PermanentError(
                message="Customer extraction results not found",
                error_code="MISSING_EXTRACT_RESULTS",
            )

        # Transform the data
        transformed = self._transform_customer_data(extract_results)

        logger.info(
            "TransformCustomersHandler: Transformed %d customer records",
            transformed["record_count"],
        )

        return StepHandlerResult.success(
            transformed,
            metadata={
                "operation": "transform_customers",
                "source_step": "extract_customer_data",
                "transformation_applied": True,
                "record_count": transformed["record_count"],
                "transformed_at": datetime.now(timezone.utc).isoformat(),
            },
        )

    def _transform_customer_data(self, extract_results: dict[str, Any]) -> dict[str, Any]:
        """Transform raw customer data into analytics format."""
        raw_records = extract_results.get("records", [])

        # Group by tier
        tier_groups: dict[str, list[dict[str, Any]]] = defaultdict(list)
        for record in raw_records:
            tier_groups[record["tier"]].append(record)

        tier_analysis = {}
        for tier, tier_records in tier_groups.items():
            total_ltv = sum(r["lifetime_value"] for r in tier_records)
            tier_analysis[tier] = {
                "customer_count": len(tier_records),
                "total_lifetime_value": total_ltv,
                "avg_lifetime_value": total_ltv / len(tier_records),
            }

        # Value segmentation
        value_segments = {
            "high_value": sum(1 for r in raw_records if r["lifetime_value"] >= 10000),
            "medium_value": sum(1 for r in raw_records if 1000 <= r["lifetime_value"] < 10000),
            "low_value": sum(1 for r in raw_records if r["lifetime_value"] < 1000),
        }

        total_ltv = sum(r["lifetime_value"] for r in raw_records)

        return {
            "record_count": len(raw_records),
            "tier_analysis": tier_analysis,
            "value_segments": value_segments,
            "total_lifetime_value": total_ltv,
            "avg_customer_value": total_ltv / len(raw_records) if raw_records else 0,
            "transformation_type": "customer_analytics",
            "source": "extract_customer_data",
        }
