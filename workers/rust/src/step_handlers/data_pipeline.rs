//! # Data Pipeline Analytics Handlers
//!
//! Native Rust implementation of the data pipeline analytics workflow pattern.
//! This module demonstrates DAG execution with parallel extraction and aggregation.
//!
//! ## Workflow Pattern (8 steps)
//!
//! **Extract Phase (3 parallel steps):**
//! 1. **ExtractSalesData**: Pull sales records from database
//! 2. **ExtractInventoryData**: Pull inventory records from warehouse system
//! 3. **ExtractCustomerData**: Pull customer records from CRM
//!
//! **Transform Phase (3 sequential steps):**
//! 4. **TransformSales**: Transform sales data for analytics
//! 5. **TransformInventory**: Transform inventory data for analytics
//! 6. **TransformCustomers**: Transform customer data for analytics
//!
//! **Aggregate Phase (1 step - DAG convergence):**
//! 7. **AggregateMetrics**: Combine all transformed data sources
//!
//! **Insights Phase (1 step):**
//! 8. **GenerateInsights**: Create actionable business intelligence
//!
//! ## TAS-91: Blog Post 02 - Data Pipeline Analytics (Rust Implementation)
//!
//! This module matches the Python and TypeScript implementations for cross-language
//! consistency demonstration in Blog Post 02.
//!
//! ## TAS-137 Best Practices Demonstrated
//!
//! - Root DAG nodes: No task context or dependency access needed
//! - `get_dependency_result()` for upstream step results
//! - DAG convergence pattern with multiple dependencies
//! - Proper error classification

use super::{error_result, success_result, RustStepHandler, StepHandlerConfig};
use anyhow::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::HashMap;
use tasker_shared::messaging::StepExecutionResult;
use tasker_shared::types::TaskSequenceStep;
use tracing::info;

// ============================================================================
// Data Types
// ============================================================================

/// Sales record from simulated database
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SalesRecord {
    pub order_id: String,
    pub date: String,
    pub product_id: String,
    pub quantity: i64,
    pub amount: f64,
}

/// Inventory record from simulated warehouse
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct InventoryRecord {
    pub product_id: String,
    pub sku: String,
    pub warehouse: String,
    pub quantity_on_hand: i64,
    pub reorder_point: i64,
}

/// Customer record from simulated CRM
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CustomerRecord {
    pub customer_id: String,
    pub name: String,
    pub tier: String,
    pub lifetime_value: f64,
    pub join_date: String,
}

// ============================================================================
// Sample Data (Simulated External Systems)
// ============================================================================

fn get_sample_sales_data() -> Vec<SalesRecord> {
    vec![
        SalesRecord {
            order_id: "ORD-001".to_string(),
            date: "2025-11-01".to_string(),
            product_id: "PROD-A".to_string(),
            quantity: 5,
            amount: 499.95,
        },
        SalesRecord {
            order_id: "ORD-002".to_string(),
            date: "2025-11-05".to_string(),
            product_id: "PROD-B".to_string(),
            quantity: 3,
            amount: 299.97,
        },
        SalesRecord {
            order_id: "ORD-003".to_string(),
            date: "2025-11-10".to_string(),
            product_id: "PROD-A".to_string(),
            quantity: 2,
            amount: 199.98,
        },
        SalesRecord {
            order_id: "ORD-004".to_string(),
            date: "2025-11-15".to_string(),
            product_id: "PROD-C".to_string(),
            quantity: 10,
            amount: 1499.90,
        },
        SalesRecord {
            order_id: "ORD-005".to_string(),
            date: "2025-11-18".to_string(),
            product_id: "PROD-B".to_string(),
            quantity: 7,
            amount: 699.93,
        },
    ]
}

fn get_sample_inventory_data() -> Vec<InventoryRecord> {
    vec![
        InventoryRecord {
            product_id: "PROD-A".to_string(),
            sku: "SKU-A-001".to_string(),
            warehouse: "WH-01".to_string(),
            quantity_on_hand: 150,
            reorder_point: 50,
        },
        InventoryRecord {
            product_id: "PROD-B".to_string(),
            sku: "SKU-B-002".to_string(),
            warehouse: "WH-01".to_string(),
            quantity_on_hand: 75,
            reorder_point: 25,
        },
        InventoryRecord {
            product_id: "PROD-C".to_string(),
            sku: "SKU-C-003".to_string(),
            warehouse: "WH-02".to_string(),
            quantity_on_hand: 200,
            reorder_point: 100,
        },
        InventoryRecord {
            product_id: "PROD-A".to_string(),
            sku: "SKU-A-001".to_string(),
            warehouse: "WH-02".to_string(),
            quantity_on_hand: 100,
            reorder_point: 50,
        },
        InventoryRecord {
            product_id: "PROD-B".to_string(),
            sku: "SKU-B-002".to_string(),
            warehouse: "WH-03".to_string(),
            quantity_on_hand: 50,
            reorder_point: 25,
        },
    ]
}

fn get_sample_customer_data() -> Vec<CustomerRecord> {
    vec![
        CustomerRecord {
            customer_id: "CUST-001".to_string(),
            name: "Alice Johnson".to_string(),
            tier: "gold".to_string(),
            lifetime_value: 5000.0,
            join_date: "2024-01-15".to_string(),
        },
        CustomerRecord {
            customer_id: "CUST-002".to_string(),
            name: "Bob Smith".to_string(),
            tier: "silver".to_string(),
            lifetime_value: 2500.0,
            join_date: "2024-03-20".to_string(),
        },
        CustomerRecord {
            customer_id: "CUST-003".to_string(),
            name: "Carol White".to_string(),
            tier: "premium".to_string(),
            lifetime_value: 15000.0,
            join_date: "2023-11-10".to_string(),
        },
        CustomerRecord {
            customer_id: "CUST-004".to_string(),
            name: "David Brown".to_string(),
            tier: "standard".to_string(),
            lifetime_value: 500.0,
            join_date: "2025-01-05".to_string(),
        },
        CustomerRecord {
            customer_id: "CUST-005".to_string(),
            name: "Eve Davis".to_string(),
            tier: "gold".to_string(),
            lifetime_value: 7500.0,
            join_date: "2024-06-12".to_string(),
        },
    ]
}

// ============================================================================
// Extract Handlers (Parallel - No Dependencies)
// ============================================================================

/// Extracts sales data from simulated database.
///
/// This handler runs in parallel with other extract handlers.
/// No dependencies - root node in the DAG.
#[derive(Debug)]
pub struct ExtractSalesDataHandler {
    #[expect(
        dead_code,
        reason = "Required for API compatibility with handler config pattern"
    )]
    config: StepHandlerConfig,
}

#[async_trait]
impl RustStepHandler for ExtractSalesDataHandler {
    async fn call(&self, step_data: &TaskSequenceStep) -> Result<StepExecutionResult> {
        let start_time = std::time::Instant::now();
        let step_uuid = step_data.workflow_step.workflow_step_uuid;

        let sales_data = get_sample_sales_data();
        let dates: Vec<&str> = sales_data.iter().map(|r| r.date.as_str()).collect();
        let total_amount: f64 = sales_data.iter().map(|r| r.amount).sum();

        info!(
            "ExtractSalesDataHandler: Extracted {} sales records",
            sales_data.len()
        );

        Ok(success_result(
            step_uuid,
            json!({
                "records": sales_data,
                "extracted_at": chrono::Utc::now().to_rfc3339(),
                "source": "SalesDatabase",
                "total_amount": total_amount,
                "date_range": {
                    "start_date": dates.iter().min().unwrap_or(&""),
                    "end_date": dates.iter().max().unwrap_or(&"")
                }
            }),
            start_time.elapsed().as_millis() as i64,
            Some(HashMap::from([
                ("operation".to_string(), json!("extract_sales")),
                ("source".to_string(), json!("SalesDatabase")),
                ("records_extracted".to_string(), json!(sales_data.len())),
            ])),
        ))
    }

    fn name(&self) -> &str {
        "data_pipeline_extract_sales"
    }

    fn new(config: StepHandlerConfig) -> Self {
        Self { config }
    }
}

/// Extracts inventory data from simulated warehouse system.
///
/// This handler runs in parallel with other extract handlers.
/// No dependencies - root node in the DAG.
#[derive(Debug)]
pub struct ExtractInventoryDataHandler {
    #[expect(
        dead_code,
        reason = "Required for API compatibility with handler config pattern"
    )]
    config: StepHandlerConfig,
}

#[async_trait]
impl RustStepHandler for ExtractInventoryDataHandler {
    async fn call(&self, step_data: &TaskSequenceStep) -> Result<StepExecutionResult> {
        let start_time = std::time::Instant::now();
        let step_uuid = step_data.workflow_step.workflow_step_uuid;

        let inventory_data = get_sample_inventory_data();
        let warehouses: Vec<String> = inventory_data
            .iter()
            .map(|r| r.warehouse.clone())
            .collect::<std::collections::HashSet<_>>()
            .into_iter()
            .collect();
        let total_quantity: i64 = inventory_data.iter().map(|r| r.quantity_on_hand).sum();
        let products_tracked = inventory_data
            .iter()
            .map(|r| r.product_id.clone())
            .collect::<std::collections::HashSet<_>>()
            .len();

        info!(
            "ExtractInventoryDataHandler: Extracted {} inventory records",
            inventory_data.len()
        );

        Ok(success_result(
            step_uuid,
            json!({
                "records": inventory_data,
                "extracted_at": chrono::Utc::now().to_rfc3339(),
                "source": "InventorySystem",
                "total_quantity": total_quantity,
                "warehouses": warehouses,
                "products_tracked": products_tracked
            }),
            start_time.elapsed().as_millis() as i64,
            Some(HashMap::from([
                ("operation".to_string(), json!("extract_inventory")),
                ("source".to_string(), json!("InventorySystem")),
                ("records_extracted".to_string(), json!(inventory_data.len())),
            ])),
        ))
    }

    fn name(&self) -> &str {
        "data_pipeline_extract_inventory"
    }

    fn new(config: StepHandlerConfig) -> Self {
        Self { config }
    }
}

/// Extracts customer data from simulated CRM.
///
/// This handler runs in parallel with other extract handlers.
/// No dependencies - root node in the DAG.
#[derive(Debug)]
pub struct ExtractCustomerDataHandler {
    #[expect(
        dead_code,
        reason = "Required for API compatibility with handler config pattern"
    )]
    config: StepHandlerConfig,
}

#[async_trait]
impl RustStepHandler for ExtractCustomerDataHandler {
    async fn call(&self, step_data: &TaskSequenceStep) -> Result<StepExecutionResult> {
        let start_time = std::time::Instant::now();
        let step_uuid = step_data.workflow_step.workflow_step_uuid;

        let customer_data = get_sample_customer_data();

        // Calculate tier breakdown
        let mut tier_breakdown: HashMap<String, i64> = HashMap::new();
        for customer in &customer_data {
            *tier_breakdown.entry(customer.tier.clone()).or_insert(0) += 1;
        }

        let total_ltv: f64 = customer_data.iter().map(|r| r.lifetime_value).sum();
        let avg_ltv = total_ltv / customer_data.len() as f64;

        info!(
            "ExtractCustomerDataHandler: Extracted {} customer records",
            customer_data.len()
        );

        Ok(success_result(
            step_uuid,
            json!({
                "records": customer_data,
                "extracted_at": chrono::Utc::now().to_rfc3339(),
                "source": "CRMSystem",
                "total_customers": customer_data.len(),
                "total_lifetime_value": total_ltv,
                "tier_breakdown": tier_breakdown,
                "avg_lifetime_value": avg_ltv
            }),
            start_time.elapsed().as_millis() as i64,
            Some(HashMap::from([
                ("operation".to_string(), json!("extract_customers")),
                ("source".to_string(), json!("CRMSystem")),
                ("records_extracted".to_string(), json!(customer_data.len())),
            ])),
        ))
    }

    fn name(&self) -> &str {
        "data_pipeline_extract_customers"
    }

    fn new(config: StepHandlerConfig) -> Self {
        Self { config }
    }
}

// ============================================================================
// Transform Handlers (Sequential - Depend on Extracts)
// ============================================================================

/// Transforms sales data for analytics.
///
/// Dependencies: extract_sales_data
#[derive(Debug)]
pub struct TransformSalesHandler {
    #[expect(
        dead_code,
        reason = "Required for API compatibility with handler config pattern"
    )]
    config: StepHandlerConfig,
}

#[async_trait]
impl RustStepHandler for TransformSalesHandler {
    async fn call(&self, step_data: &TaskSequenceStep) -> Result<StepExecutionResult> {
        let start_time = std::time::Instant::now();
        let step_uuid = step_data.workflow_step.workflow_step_uuid;

        // Get extract results
        let records: Vec<SalesRecord> =
            match step_data.get_dependency_field("extract_sales_data", &["records"]) {
                Ok(records) => records,
                Err(e) => {
                    return Ok(error_result(
                        step_uuid,
                        format!("Sales extraction results not found: {}", e),
                        Some("MISSING_EXTRACT_RESULTS".to_string()),
                        Some("DependencyError".to_string()),
                        false,
                        start_time.elapsed().as_millis() as i64,
                        None,
                    ));
                }
            };

        // Group by date for daily sales
        let mut daily_groups: HashMap<String, Vec<&SalesRecord>> = HashMap::new();
        for record in &records {
            daily_groups
                .entry(record.date.clone())
                .or_default()
                .push(record);
        }

        let mut daily_sales: HashMap<String, serde_json::Value> = HashMap::new();
        for (date, day_records) in &daily_groups {
            let total: f64 = day_records.iter().map(|r| r.amount).sum();
            let count = day_records.len();
            daily_sales.insert(
                date.clone(),
                json!({
                    "total_amount": total,
                    "order_count": count,
                    "avg_order_value": total / count as f64
                }),
            );
        }

        // Group by product for product sales
        let mut product_groups: HashMap<String, Vec<&SalesRecord>> = HashMap::new();
        for record in &records {
            product_groups
                .entry(record.product_id.clone())
                .or_default()
                .push(record);
        }

        let mut product_sales: HashMap<String, serde_json::Value> = HashMap::new();
        for (product_id, product_records) in &product_groups {
            let total_qty: i64 = product_records.iter().map(|r| r.quantity).sum();
            let total_rev: f64 = product_records.iter().map(|r| r.amount).sum();
            product_sales.insert(
                product_id.clone(),
                json!({
                    "total_quantity": total_qty,
                    "total_revenue": total_rev,
                    "order_count": product_records.len()
                }),
            );
        }

        let total_revenue: f64 = records.iter().map(|r| r.amount).sum();

        info!(
            "TransformSalesHandler: Transformed {} sales records",
            records.len()
        );

        Ok(success_result(
            step_uuid,
            json!({
                "record_count": records.len(),
                "daily_sales": daily_sales,
                "product_sales": product_sales,
                "total_revenue": total_revenue,
                "transformation_type": "sales_analytics",
                "source": "extract_sales_data"
            }),
            start_time.elapsed().as_millis() as i64,
            Some(HashMap::from([
                ("operation".to_string(), json!("transform_sales")),
                ("source_step".to_string(), json!("extract_sales_data")),
                ("record_count".to_string(), json!(records.len())),
            ])),
        ))
    }

    fn name(&self) -> &str {
        "data_pipeline_transform_sales"
    }

    fn new(config: StepHandlerConfig) -> Self {
        Self { config }
    }
}

/// Transforms inventory data for analytics.
///
/// Dependencies: extract_inventory_data
#[derive(Debug)]
pub struct TransformInventoryHandler {
    #[expect(
        dead_code,
        reason = "Required for API compatibility with handler config pattern"
    )]
    config: StepHandlerConfig,
}

#[async_trait]
impl RustStepHandler for TransformInventoryHandler {
    async fn call(&self, step_data: &TaskSequenceStep) -> Result<StepExecutionResult> {
        let start_time = std::time::Instant::now();
        let step_uuid = step_data.workflow_step.workflow_step_uuid;

        // Get extract results
        let records: Vec<InventoryRecord> =
            match step_data.get_dependency_field("extract_inventory_data", &["records"]) {
                Ok(records) => records,
                Err(e) => {
                    return Ok(error_result(
                        step_uuid,
                        format!("Inventory extraction results not found: {}", e),
                        Some("MISSING_EXTRACT_RESULTS".to_string()),
                        Some("DependencyError".to_string()),
                        false,
                        start_time.elapsed().as_millis() as i64,
                        None,
                    ));
                }
            };

        // Group by warehouse
        let mut warehouse_groups: HashMap<String, Vec<&InventoryRecord>> = HashMap::new();
        for record in &records {
            warehouse_groups
                .entry(record.warehouse.clone())
                .or_default()
                .push(record);
        }

        let mut warehouse_summary: HashMap<String, serde_json::Value> = HashMap::new();
        for (warehouse, wh_records) in &warehouse_groups {
            let reorder_count = wh_records
                .iter()
                .filter(|r| r.quantity_on_hand <= r.reorder_point)
                .count();
            let total_qty: i64 = wh_records.iter().map(|r| r.quantity_on_hand).sum();
            let product_count = wh_records
                .iter()
                .map(|r| r.product_id.clone())
                .collect::<std::collections::HashSet<_>>()
                .len();
            warehouse_summary.insert(
                warehouse.clone(),
                json!({
                    "total_quantity": total_qty,
                    "product_count": product_count,
                    "reorder_alerts": reorder_count
                }),
            );
        }

        // Group by product
        let mut product_groups: HashMap<String, Vec<&InventoryRecord>> = HashMap::new();
        for record in &records {
            product_groups
                .entry(record.product_id.clone())
                .or_default()
                .push(record);
        }

        let mut product_inventory: HashMap<String, serde_json::Value> = HashMap::new();
        for (product_id, product_records) in &product_groups {
            let total_qty: i64 = product_records.iter().map(|r| r.quantity_on_hand).sum();
            let total_reorder: i64 = product_records.iter().map(|r| r.reorder_point).sum();
            let warehouse_count = product_records
                .iter()
                .map(|r| r.warehouse.clone())
                .collect::<std::collections::HashSet<_>>()
                .len();
            product_inventory.insert(
                product_id.clone(),
                json!({
                    "total_quantity": total_qty,
                    "warehouse_count": warehouse_count,
                    "needs_reorder": total_qty < total_reorder
                }),
            );
        }

        let total_on_hand: i64 = records.iter().map(|r| r.quantity_on_hand).sum();
        let reorder_alerts = product_inventory
            .values()
            .filter(|v| v["needs_reorder"].as_bool().unwrap_or(false))
            .count();

        info!(
            "TransformInventoryHandler: Transformed {} inventory records",
            records.len()
        );

        Ok(success_result(
            step_uuid,
            json!({
                "record_count": records.len(),
                "warehouse_summary": warehouse_summary,
                "product_inventory": product_inventory,
                "total_quantity_on_hand": total_on_hand,
                "reorder_alerts": reorder_alerts,
                "transformation_type": "inventory_analytics",
                "source": "extract_inventory_data"
            }),
            start_time.elapsed().as_millis() as i64,
            Some(HashMap::from([
                ("operation".to_string(), json!("transform_inventory")),
                ("source_step".to_string(), json!("extract_inventory_data")),
                ("record_count".to_string(), json!(records.len())),
            ])),
        ))
    }

    fn name(&self) -> &str {
        "data_pipeline_transform_inventory"
    }

    fn new(config: StepHandlerConfig) -> Self {
        Self { config }
    }
}

/// Transforms customer data for analytics.
///
/// Dependencies: extract_customer_data
#[derive(Debug)]
pub struct TransformCustomersHandler {
    #[expect(
        dead_code,
        reason = "Required for API compatibility with handler config pattern"
    )]
    config: StepHandlerConfig,
}

#[async_trait]
impl RustStepHandler for TransformCustomersHandler {
    async fn call(&self, step_data: &TaskSequenceStep) -> Result<StepExecutionResult> {
        let start_time = std::time::Instant::now();
        let step_uuid = step_data.workflow_step.workflow_step_uuid;

        // Get extract results
        let records: Vec<CustomerRecord> =
            match step_data.get_dependency_field("extract_customer_data", &["records"]) {
                Ok(records) => records,
                Err(e) => {
                    return Ok(error_result(
                        step_uuid,
                        format!("Customer extraction results not found: {}", e),
                        Some("MISSING_EXTRACT_RESULTS".to_string()),
                        Some("DependencyError".to_string()),
                        false,
                        start_time.elapsed().as_millis() as i64,
                        None,
                    ));
                }
            };

        // Group by tier
        let mut tier_groups: HashMap<String, Vec<&CustomerRecord>> = HashMap::new();
        for record in &records {
            tier_groups
                .entry(record.tier.clone())
                .or_default()
                .push(record);
        }

        let mut tier_analysis: HashMap<String, serde_json::Value> = HashMap::new();
        for (tier, tier_records) in &tier_groups {
            let total_ltv: f64 = tier_records.iter().map(|r| r.lifetime_value).sum();
            let count = tier_records.len();
            tier_analysis.insert(
                tier.clone(),
                json!({
                    "customer_count": count,
                    "total_lifetime_value": total_ltv,
                    "avg_lifetime_value": total_ltv / count as f64
                }),
            );
        }

        // Value segmentation
        let high_value = records.iter().filter(|r| r.lifetime_value >= 10000.0).count();
        let medium_value = records
            .iter()
            .filter(|r| r.lifetime_value >= 1000.0 && r.lifetime_value < 10000.0)
            .count();
        let low_value = records.iter().filter(|r| r.lifetime_value < 1000.0).count();

        let total_ltv: f64 = records.iter().map(|r| r.lifetime_value).sum();
        let avg_customer_value = if !records.is_empty() {
            total_ltv / records.len() as f64
        } else {
            0.0
        };

        info!(
            "TransformCustomersHandler: Transformed {} customer records",
            records.len()
        );

        Ok(success_result(
            step_uuid,
            json!({
                "record_count": records.len(),
                "tier_analysis": tier_analysis,
                "value_segments": {
                    "high_value": high_value,
                    "medium_value": medium_value,
                    "low_value": low_value
                },
                "total_lifetime_value": total_ltv,
                "avg_customer_value": avg_customer_value,
                "transformation_type": "customer_analytics",
                "source": "extract_customer_data"
            }),
            start_time.elapsed().as_millis() as i64,
            Some(HashMap::from([
                ("operation".to_string(), json!("transform_customers")),
                ("source_step".to_string(), json!("extract_customer_data")),
                ("record_count".to_string(), json!(records.len())),
            ])),
        ))
    }

    fn name(&self) -> &str {
        "data_pipeline_transform_customers"
    }

    fn new(config: StepHandlerConfig) -> Self {
        Self { config }
    }
}

// ============================================================================
// Aggregate Handler (DAG Convergence)
// ============================================================================

/// Aggregates metrics from all transformed data sources.
///
/// This handler demonstrates DAG convergence - it depends on all 3 transform
/// steps and combines their results.
///
/// Dependencies: transform_sales, transform_inventory, transform_customers
#[derive(Debug)]
pub struct AggregateMetricsHandler {
    #[expect(
        dead_code,
        reason = "Required for API compatibility with handler config pattern"
    )]
    config: StepHandlerConfig,
}

#[async_trait]
impl RustStepHandler for AggregateMetricsHandler {
    async fn call(&self, step_data: &TaskSequenceStep) -> Result<StepExecutionResult> {
        let start_time = std::time::Instant::now();
        let step_uuid = step_data.workflow_step.workflow_step_uuid;

        // Get results from all 3 transform steps (DAG convergence)
        let sales_data: serde_json::Value = step_data
            .get_dependency_result_column_value("transform_sales")
            .ok()
            .flatten()
            .unwrap_or(json!({}));

        let inventory_data: serde_json::Value = step_data
            .get_dependency_result_column_value("transform_inventory")
            .ok()
            .flatten()
            .unwrap_or(json!({}));

        let customer_data: serde_json::Value = step_data
            .get_dependency_result_column_value("transform_customers")
            .ok()
            .flatten()
            .unwrap_or(json!({}));

        // Validate all sources present
        let mut missing = Vec::new();
        if sales_data.as_object().map(|o| o.is_empty()).unwrap_or(true) {
            missing.push("transform_sales");
        }
        if inventory_data
            .as_object()
            .map(|o| o.is_empty())
            .unwrap_or(true)
        {
            missing.push("transform_inventory");
        }
        if customer_data
            .as_object()
            .map(|o| o.is_empty())
            .unwrap_or(true)
        {
            missing.push("transform_customers");
        }

        if !missing.is_empty() {
            return Ok(error_result(
                step_uuid,
                format!("Missing transform results: {}", missing.join(", ")),
                Some("MISSING_TRANSFORM_RESULTS".to_string()),
                Some("DependencyError".to_string()),
                false,
                start_time.elapsed().as_millis() as i64,
                None,
            ));
        }

        // Extract metrics from each source
        let total_revenue = sales_data["total_revenue"].as_f64().unwrap_or(0.0);
        let sales_record_count = sales_data["record_count"].as_i64().unwrap_or(0);

        let total_inventory = inventory_data["total_quantity_on_hand"].as_i64().unwrap_or(0);
        let reorder_alerts = inventory_data["reorder_alerts"].as_i64().unwrap_or(0);

        let total_customers = customer_data["record_count"].as_i64().unwrap_or(0);
        let total_ltv = customer_data["total_lifetime_value"].as_f64().unwrap_or(0.0);

        // Calculate cross-source metrics
        let revenue_per_customer = if total_customers > 0 {
            total_revenue / total_customers as f64
        } else {
            0.0
        };
        let inventory_turnover = if total_inventory > 0 {
            total_revenue / total_inventory as f64
        } else {
            0.0
        };

        info!(
            "AggregateMetricsHandler: Aggregated metrics from 3 sources - revenue=${:.2}, inventory={}, customers={}",
            total_revenue, total_inventory, total_customers
        );

        Ok(success_result(
            step_uuid,
            json!({
                "total_revenue": total_revenue,
                "total_inventory_quantity": total_inventory,
                "total_customers": total_customers,
                "total_customer_lifetime_value": total_ltv,
                "sales_transactions": sales_record_count,
                "inventory_reorder_alerts": reorder_alerts,
                "revenue_per_customer": (revenue_per_customer * 100.0).round() / 100.0,
                "inventory_turnover_indicator": (inventory_turnover * 10000.0).round() / 10000.0,
                "aggregation_complete": true,
                "sources_included": 3
            }),
            start_time.elapsed().as_millis() as i64,
            Some(HashMap::from([
                ("operation".to_string(), json!("aggregate_metrics")),
                (
                    "sources".to_string(),
                    json!(["transform_sales", "transform_inventory", "transform_customers"]),
                ),
                ("sources_aggregated".to_string(), json!(3)),
            ])),
        ))
    }

    fn name(&self) -> &str {
        "data_pipeline_aggregate_metrics"
    }

    fn new(config: StepHandlerConfig) -> Self {
        Self { config }
    }
}

// ============================================================================
// Generate Insights Handler (Final Step)
// ============================================================================

/// Generates business insights from aggregated metrics.
///
/// This handler is the final step in the DAG workflow.
///
/// Dependencies: aggregate_metrics
#[derive(Debug)]
pub struct GenerateInsightsHandler {
    #[expect(
        dead_code,
        reason = "Required for API compatibility with handler config pattern"
    )]
    config: StepHandlerConfig,
}

#[async_trait]
impl RustStepHandler for GenerateInsightsHandler {
    async fn call(&self, step_data: &TaskSequenceStep) -> Result<StepExecutionResult> {
        let start_time = std::time::Instant::now();
        let step_uuid = step_data.workflow_step.workflow_step_uuid;

        // Get aggregated metrics from prior step
        let metrics: serde_json::Value = step_data
            .get_dependency_result_column_value("aggregate_metrics")
            .ok()
            .flatten()
            .unwrap_or(json!({}));

        if metrics.as_object().map(|o| o.is_empty()).unwrap_or(true) {
            return Ok(error_result(
                step_uuid,
                "Aggregated metrics not found".to_string(),
                Some("MISSING_AGGREGATE_RESULTS".to_string()),
                Some("DependencyError".to_string()),
                false,
                start_time.elapsed().as_millis() as i64,
                None,
            ));
        }

        let mut insights = Vec::new();

        // Revenue insights
        let revenue = metrics["total_revenue"].as_f64().unwrap_or(0.0);
        let customers = metrics["total_customers"].as_i64().unwrap_or(0);
        let revenue_per_customer = metrics["revenue_per_customer"].as_f64().unwrap_or(0.0);

        if revenue > 0.0 {
            let recommendation = if revenue_per_customer < 500.0 {
                "Consider upselling strategies"
            } else {
                "Customer spend is healthy"
            };
            insights.push(json!({
                "category": "Revenue",
                "finding": format!("Total revenue of ${} with {} customers", revenue, customers),
                "metric": revenue_per_customer,
                "recommendation": recommendation
            }));
        }

        // Inventory insights
        let inventory_alerts = metrics["inventory_reorder_alerts"].as_i64().unwrap_or(0);
        if inventory_alerts > 0 {
            insights.push(json!({
                "category": "Inventory",
                "finding": format!("{} products need reordering", inventory_alerts),
                "metric": inventory_alerts,
                "recommendation": "Review reorder points and place purchase orders"
            }));
        } else {
            insights.push(json!({
                "category": "Inventory",
                "finding": "All products above reorder points",
                "metric": 0,
                "recommendation": "Inventory levels are healthy"
            }));
        }

        // Customer insights
        let total_ltv = metrics["total_customer_lifetime_value"].as_f64().unwrap_or(0.0);
        let avg_ltv = if customers > 0 {
            total_ltv / customers as f64
        } else {
            0.0
        };

        let recommendation = if avg_ltv > 3000.0 {
            "Focus on retention programs"
        } else {
            "Increase customer engagement"
        };
        insights.push(json!({
            "category": "Customer Value",
            "finding": format!("Average customer lifetime value: ${:.2}", avg_ltv),
            "metric": avg_ltv,
            "recommendation": recommendation
        }));

        // Business health score
        let mut score = 0;
        if revenue_per_customer > 500.0 {
            score += 40;
        }
        if inventory_alerts == 0 {
            score += 30;
        }
        if avg_ltv > 3000.0 {
            score += 30;
        }

        let rating = if score >= 80 {
            "Excellent"
        } else if score >= 60 {
            "Good"
        } else if score >= 40 {
            "Fair"
        } else {
            "Needs Improvement"
        };

        let health_score = json!({
            "score": score,
            "max_score": 100,
            "rating": rating
        });

        info!(
            "GenerateInsightsHandler: Generated {} business insights",
            insights.len()
        );

        Ok(success_result(
            step_uuid,
            json!({
                "insights": insights,
                "health_score": health_score,
                "total_metrics_analyzed": metrics.as_object().map(|o| o.len()).unwrap_or(0),
                "pipeline_complete": true,
                "generated_at": chrono::Utc::now().to_rfc3339()
            }),
            start_time.elapsed().as_millis() as i64,
            Some(HashMap::from([
                ("operation".to_string(), json!("generate_insights")),
                ("source_step".to_string(), json!("aggregate_metrics")),
                ("insights_generated".to_string(), json!(insights.len())),
            ])),
        ))
    }

    fn name(&self) -> &str {
        "data_pipeline_generate_insights"
    }

    fn new(config: StepHandlerConfig) -> Self {
        Self { config }
    }
}
