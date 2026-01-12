"""Extract handlers for data pipeline workflow.

These handlers run in parallel (no dependencies) and extract data from simulated
external systems: Sales Database, Inventory System, and CRM.

TAS-137 Best Practices Demonstrated:
- Root DAG nodes: No task context or dependency access needed
- Data sourced from inline sample data (simulated external systems)
- Results passed to downstream transform steps via DAG
"""

from __future__ import annotations

import logging
from datetime import datetime, timezone
from typing import TYPE_CHECKING, Any

from tasker_core import StepHandler, StepHandlerResult

if TYPE_CHECKING:
    from tasker_core import StepContext

logger = logging.getLogger(__name__)

# =============================================================================
# Sample Data (Simulated External Systems)
# =============================================================================

SAMPLE_SALES_DATA: list[dict[str, Any]] = [
    {
        "order_id": "ORD-001",
        "date": "2025-11-01",
        "product_id": "PROD-A",
        "quantity": 5,
        "amount": 499.95,
    },
    {
        "order_id": "ORD-002",
        "date": "2025-11-05",
        "product_id": "PROD-B",
        "quantity": 3,
        "amount": 299.97,
    },
    {
        "order_id": "ORD-003",
        "date": "2025-11-10",
        "product_id": "PROD-A",
        "quantity": 2,
        "amount": 199.98,
    },
    {
        "order_id": "ORD-004",
        "date": "2025-11-15",
        "product_id": "PROD-C",
        "quantity": 10,
        "amount": 1499.90,
    },
    {
        "order_id": "ORD-005",
        "date": "2025-11-18",
        "product_id": "PROD-B",
        "quantity": 7,
        "amount": 699.93,
    },
]

SAMPLE_INVENTORY_DATA: list[dict[str, Any]] = [
    {
        "product_id": "PROD-A",
        "sku": "SKU-A-001",
        "warehouse": "WH-01",
        "quantity_on_hand": 150,
        "reorder_point": 50,
    },
    {
        "product_id": "PROD-B",
        "sku": "SKU-B-002",
        "warehouse": "WH-01",
        "quantity_on_hand": 75,
        "reorder_point": 25,
    },
    {
        "product_id": "PROD-C",
        "sku": "SKU-C-003",
        "warehouse": "WH-02",
        "quantity_on_hand": 200,
        "reorder_point": 100,
    },
    {
        "product_id": "PROD-A",
        "sku": "SKU-A-001",
        "warehouse": "WH-02",
        "quantity_on_hand": 100,
        "reorder_point": 50,
    },
    {
        "product_id": "PROD-B",
        "sku": "SKU-B-002",
        "warehouse": "WH-03",
        "quantity_on_hand": 50,
        "reorder_point": 25,
    },
]

SAMPLE_CUSTOMER_DATA: list[dict[str, Any]] = [
    {
        "customer_id": "CUST-001",
        "name": "Alice Johnson",
        "tier": "gold",
        "lifetime_value": 5000.00,
        "join_date": "2024-01-15",
    },
    {
        "customer_id": "CUST-002",
        "name": "Bob Smith",
        "tier": "silver",
        "lifetime_value": 2500.00,
        "join_date": "2024-03-20",
    },
    {
        "customer_id": "CUST-003",
        "name": "Carol White",
        "tier": "premium",
        "lifetime_value": 15000.00,
        "join_date": "2023-11-10",
    },
    {
        "customer_id": "CUST-004",
        "name": "David Brown",
        "tier": "standard",
        "lifetime_value": 500.00,
        "join_date": "2025-01-05",
    },
    {
        "customer_id": "CUST-005",
        "name": "Eve Davis",
        "tier": "gold",
        "lifetime_value": 7500.00,
        "join_date": "2024-06-12",
    },
]


# =============================================================================
# Extract Sales Data Handler
# =============================================================================


class ExtractSalesDataHandler(StepHandler):
    """Extract sales data from source system (simulated).

    This handler runs in parallel with other extract handlers.
    No dependencies - root node in the DAG.

    Output:
        records: list[dict] - Raw sales records
        extracted_at: str - Extraction timestamp
        total_amount: float - Sum of all sales amounts
        date_range: dict - Start and end dates of data
    """

    handler_name = "data_pipeline.step_handlers.ExtractSalesDataHandler"
    handler_version = "1.0.0"

    def call(self, context: StepContext) -> StepHandlerResult:
        """Extract sales data from simulated database."""
        logger.info(
            "ExtractSalesDataHandler: Extracting sales data - task_uuid=%s",
            context.task_uuid,
        )

        # Simulate data extraction from sales database
        sales_data = self._extract_sales_from_source()

        logger.info(
            "ExtractSalesDataHandler: Extracted %d sales records",
            len(sales_data["records"]),
        )

        return StepHandlerResult.success(
            sales_data,
            metadata={
                "operation": "extract_sales",
                "source": "SalesDatabase",
                "records_extracted": len(sales_data["records"]),
                "extraction_time": datetime.now(timezone.utc).isoformat(),
            },
        )

    def _extract_sales_from_source(self) -> dict[str, Any]:
        """Simulate database query for sales data."""
        dates = [r["date"] for r in SAMPLE_SALES_DATA]
        return {
            "records": SAMPLE_SALES_DATA,
            "extracted_at": datetime.now(timezone.utc).isoformat(),
            "source": "SalesDatabase",
            "total_amount": sum(r["amount"] for r in SAMPLE_SALES_DATA),
            "date_range": {
                "start_date": min(dates),
                "end_date": max(dates),
            },
        }


# =============================================================================
# Extract Inventory Data Handler
# =============================================================================


class ExtractInventoryDataHandler(StepHandler):
    """Extract inventory data from warehouse system (simulated).

    This handler runs in parallel with other extract handlers.
    No dependencies - root node in the DAG.

    Output:
        records: list[dict] - Raw inventory records
        extracted_at: str - Extraction timestamp
        total_quantity: int - Sum of all quantities on hand
        warehouses: list[str] - Unique warehouse IDs
    """

    handler_name = "data_pipeline.step_handlers.ExtractInventoryDataHandler"
    handler_version = "1.0.0"

    def call(self, context: StepContext) -> StepHandlerResult:
        """Extract inventory data from simulated warehouse system."""
        logger.info(
            "ExtractInventoryDataHandler: Extracting inventory data - task_uuid=%s",
            context.task_uuid,
        )

        # Simulate data extraction from inventory system
        inventory_data = self._extract_inventory_from_source()

        logger.info(
            "ExtractInventoryDataHandler: Extracted %d inventory records",
            len(inventory_data["records"]),
        )

        return StepHandlerResult.success(
            inventory_data,
            metadata={
                "operation": "extract_inventory",
                "source": "InventorySystem",
                "records_extracted": len(inventory_data["records"]),
                "warehouses": inventory_data["warehouses"],
                "extraction_time": datetime.now(timezone.utc).isoformat(),
            },
        )

    def _extract_inventory_from_source(self) -> dict[str, Any]:
        """Simulate warehouse query for inventory data."""
        warehouses = list({r["warehouse"] for r in SAMPLE_INVENTORY_DATA})
        return {
            "records": SAMPLE_INVENTORY_DATA,
            "extracted_at": datetime.now(timezone.utc).isoformat(),
            "source": "InventorySystem",
            "total_quantity": sum(r["quantity_on_hand"] for r in SAMPLE_INVENTORY_DATA),
            "warehouses": warehouses,
            "products_tracked": len({r["product_id"] for r in SAMPLE_INVENTORY_DATA}),
        }


# =============================================================================
# Extract Customer Data Handler
# =============================================================================


class ExtractCustomerDataHandler(StepHandler):
    """Extract customer data from CRM system (simulated).

    This handler runs in parallel with other extract handlers.
    No dependencies - root node in the DAG.

    Output:
        records: list[dict] - Raw customer records
        extracted_at: str - Extraction timestamp
        total_customers: int - Total customer count
        tier_breakdown: dict - Count per tier
    """

    handler_name = "data_pipeline.step_handlers.ExtractCustomerDataHandler"
    handler_version = "1.0.0"

    def call(self, context: StepContext) -> StepHandlerResult:
        """Extract customer data from simulated CRM."""
        logger.info(
            "ExtractCustomerDataHandler: Extracting customer data - task_uuid=%s",
            context.task_uuid,
        )

        # Simulate data extraction from CRM
        customer_data = self._extract_customers_from_source()

        logger.info(
            "ExtractCustomerDataHandler: Extracted %d customer records",
            len(customer_data["records"]),
        )

        return StepHandlerResult.success(
            customer_data,
            metadata={
                "operation": "extract_customers",
                "source": "CRMSystem",
                "records_extracted": len(customer_data["records"]),
                "customer_tiers": list(customer_data["tier_breakdown"].keys()),
                "extraction_time": datetime.now(timezone.utc).isoformat(),
            },
        )

    def _extract_customers_from_source(self) -> dict[str, Any]:
        """Simulate CRM query for customer data."""
        # Calculate tier breakdown
        tier_breakdown: dict[str, int] = {}
        for customer in SAMPLE_CUSTOMER_DATA:
            tier = customer["tier"]
            tier_breakdown[tier] = tier_breakdown.get(tier, 0) + 1

        total_ltv = sum(r["lifetime_value"] for r in SAMPLE_CUSTOMER_DATA)
        return {
            "records": SAMPLE_CUSTOMER_DATA,
            "extracted_at": datetime.now(timezone.utc).isoformat(),
            "source": "CRMSystem",
            "total_customers": len(SAMPLE_CUSTOMER_DATA),
            "total_lifetime_value": total_ltv,
            "tier_breakdown": tier_breakdown,
            "avg_lifetime_value": total_ltv / len(SAMPLE_CUSTOMER_DATA),
        }
