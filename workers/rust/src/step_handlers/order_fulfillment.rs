//! # Order Fulfillment Workflow Handlers
//!
//! Native Rust implementation of the order fulfillment workflow pattern that exactly replicates
//! the Ruby handlers in `workers/ruby/spec/handlers/examples/order_fulfillment/`.
//!
//! ## Workflow Pattern
//!
//! This demonstrates a real-world e-commerce order processing workflow:
//!
//! 1. **Validate Order**: Validate customer info, order items, calculate totals, check product availability
//! 2. **Reserve Inventory**: Reserve items in warehouse, generate reservation IDs with expiration times
//! 3. **Process Payment**: Charge payment method through gateway, handle various payment failures
//! 4. **Ship Order**: Create shipping labels, generate tracking numbers, calculate delivery estimates
//!
//! ## Performance Benefits
//!
//! Native Rust implementation provides:
//! - Zero-overhead abstractions for complex business logic
//! - Compile-time type checking for financial calculations
//! - Memory safety for sensitive payment data processing
//! - High-performance JSON processing for external API integrations

use super::{error_result, success_result, RustStepHandler, StepHandlerConfig};
use anyhow::Result;
use async_trait::async_trait;
use chrono::{Datelike, Utc};
use serde_json::{json, Value};
use std::collections::HashMap;
use tasker_shared::messaging::StepExecutionResult;
use tasker_shared::types::TaskSequenceStep;
use tracing::{error, info};

/// Validate Order: Validate customer info, order items, and calculate totals
#[derive(Debug)]
pub struct ValidateOrderHandler {
    #[allow(dead_code)] // api compatibility
    config: StepHandlerConfig,
}

#[async_trait]
impl RustStepHandler for ValidateOrderHandler {
    async fn call(&self, step_data: &TaskSequenceStep) -> Result<StepExecutionResult> {
        let start_time = std::time::Instant::now();
        let step_uuid = step_data.workflow_step.workflow_step_uuid;

        // Extract and validate customer info
        let customer_info = match step_data.get_context_field::<serde_json::Value>("customer") {
            Ok(value) => value,
            Err(e) => {
                error!("Missing customer in task context: {}", e);
                return Ok(error_result(
                    step_uuid,
                    "Customer information is required".to_string(),
                    Some("MISSING_CUSTOMER_INFO".to_string()),
                    Some("ValidationError".to_string()),
                    false, // Not retryable - data validation error
                    start_time.elapsed().as_millis() as i64,
                    Some(HashMap::from([("field".to_string(), json!("customer"))])),
                ));
            }
        };

        let customer_id = match customer_info.get("id").and_then(serde_json::Value::as_i64) {
            Some(id) => id,
            None => {
                return Ok(error_result(
                    step_uuid,
                    "Customer ID is required".to_string(),
                    Some("MISSING_CUSTOMER_ID".to_string()),
                    Some("ValidationError".to_string()),
                    false,
                    start_time.elapsed().as_millis() as i64,
                    None,
                ));
            }
        };

        let _customer_email = match customer_info.get("email").and_then(|v| v.as_str()) {
            Some(email) => email.to_string(),
            None => {
                return Ok(error_result(
                    step_uuid,
                    "Customer email is required".to_string(),
                    Some("MISSING_CUSTOMER_EMAIL".to_string()),
                    Some("ValidationError".to_string()),
                    false,
                    start_time.elapsed().as_millis() as i64,
                    None,
                ));
            }
        };

        // Extract and validate order items
        let order_items =
            match step_data.get_context_field::<Option<Vec<serde_json::Value>>>("items") {
                Ok(value) => match value {
                    Some(items) => items,
                    None => {
                        return Ok(error_result(
                            step_uuid,
                            "Order items must be an array".to_string(),
                            Some("INVALID_ORDER_ITEMS".to_string()),
                            Some("ValidationError".to_string()),
                            false,
                            start_time.elapsed().as_millis() as i64,
                            None,
                        ));
                    }
                },
                Err(_) => {
                    return Ok(error_result(
                        step_uuid,
                        "Order items are required".to_string(),
                        Some("MISSING_ORDER_ITEMS".to_string()),
                        Some("ValidationError".to_string()),
                        false,
                        start_time.elapsed().as_millis() as i64,
                        None,
                    ));
                }
            };

        if order_items.is_empty() {
            return Ok(error_result(
                step_uuid,
                "Order items cannot be empty".to_string(),
                Some("EMPTY_ORDER_ITEMS".to_string()),
                Some("ValidationError".to_string()),
                false,
                start_time.elapsed().as_millis() as i64,
                None,
            ));
        }

        // Validate each order item and calculate totals
        let mut validated_items = Vec::new();
        let mut total_amount = 0.0;

        for (index, item) in order_items.iter().enumerate() {
            let sku = match item.get("sku").and_then(|v| v.as_str()) {
                Some(sku_str) => sku_str,
                None => {
                    return Ok(error_result(
                        step_uuid,
                        format!("Invalid order item at position {}: missing sku", index + 1),
                        Some("INVALID_ORDER_ITEM".to_string()),
                        Some("ValidationError".to_string()),
                        false,
                        start_time.elapsed().as_millis() as i64,
                        Some(HashMap::from([("item_index".to_string(), json!(index))])),
                    ));
                }
            };

            let quantity = match item.get("quantity").and_then(serde_json::Value::as_i64) {
                Some(qty) if qty > 0 => qty,
                Some(qty) => {
                    return Ok(error_result(
                        step_uuid,
                        format!("Invalid quantity {} for product {}", qty, sku),
                        Some("INVALID_QUANTITY".to_string()),
                        Some("ValidationError".to_string()),
                        false,
                        start_time.elapsed().as_millis() as i64,
                        Some(HashMap::from([
                            ("sku".to_string(), json!(sku)),
                            ("quantity".to_string(), json!(qty)),
                        ])),
                    ));
                }
                None => {
                    return Ok(error_result(
                        step_uuid,
                        format!(
                            "Invalid order item at position {}: missing quantity",
                            index + 1
                        ),
                        Some("INVALID_ORDER_ITEM".to_string()),
                        Some("ValidationError".to_string()),
                        false,
                        start_time.elapsed().as_millis() as i64,
                        Some(HashMap::from([("item_index".to_string(), json!(index))])),
                    ));
                }
            };

            let price = match item.get("price").and_then(serde_json::Value::as_f64) {
                Some(p) if p >= 0.0 => p,
                Some(p) => {
                    return Ok(error_result(
                        step_uuid,
                        format!("Invalid price {} for product {}", p, sku),
                        Some("INVALID_PRICE".to_string()),
                        Some("ValidationError".to_string()),
                        false,
                        start_time.elapsed().as_millis() as i64,
                        Some(HashMap::from([
                            ("sku".to_string(), json!(sku)),
                            ("price".to_string(), json!(p)),
                        ])),
                    ));
                }
                None => {
                    return Ok(error_result(
                        step_uuid,
                        format!(
                            "Invalid order item at position {}: missing price",
                            index + 1
                        ),
                        Some("INVALID_ORDER_ITEM".to_string()),
                        Some("ValidationError".to_string()),
                        false,
                        start_time.elapsed().as_millis() as i64,
                        Some(HashMap::from([("item_index".to_string(), json!(index))])),
                    ));
                }
            };

            // Simulate product lookup
            let product_data = match simulate_product_lookup(sku) {
                Ok(data) => data,
                Err(e) => {
                    return Ok(error_result(
                        step_uuid,
                        format!("Product {} not found, error: {}", sku, e),
                        Some("PRODUCT_NOT_FOUND".to_string()),
                        Some("ValidationError".to_string()),
                        false,
                        start_time.elapsed().as_millis() as i64,
                        Some(HashMap::from([("sku".to_string(), json!(sku))])),
                    ));
                }
            };

            let line_total = price * quantity as f64;
            total_amount += line_total;

            validated_items.push(json!({
                "sku": sku,
                "product_name": product_data.name,
                "quantity": quantity,
                "unit_price": price,
                "line_total": line_total,
                "available_stock": product_data.stock,
                "category": product_data.category
            }));
        }

        // Validate reasonable order total
        if total_amount > 50_000.0 {
            return Ok(error_result(
                step_uuid,
                format!(
                    "Order total ${:.2} exceeds maximum allowed value",
                    total_amount
                ),
                Some("ORDER_TOTAL_TOO_HIGH".to_string()),
                Some("ValidationError".to_string()),
                false,
                start_time.elapsed().as_millis() as i64,
                Some(HashMap::from([
                    ("total_amount".to_string(), json!(total_amount)),
                    ("max_allowed".to_string(), json!(50000.0)),
                ])),
            ));
        }

        info!("🎯 VALIDATE_ORDER: Order validation complete - customer_id={}, item_count={}, total=${:.2}",
              customer_id, validated_items.len(), total_amount);

        let mut metadata = HashMap::new();
        metadata.insert("operation".to_string(), json!("validate_order"));
        metadata.insert("item_count".to_string(), json!(validated_items.len()));
        metadata.insert(
            "input_refs".to_string(),
            json!({
                "customer_id": "task.context.customer.id",
                "order_items": "task.context.items"
            }),
        );

        Ok(success_result(
            step_uuid,
            json!({
                "customer_validated": true,
                "customer_id": customer_id,
                "validated_items": validated_items,
                "order_total": total_amount,
                "validation_status": "complete",
                "validated_at": Utc::now().to_rfc3339()
            }),
            start_time.elapsed().as_millis() as i64,
            Some(metadata),
        ))
    }

    fn name(&self) -> &'static str {
        "validate_order"
    }

    fn new(config: StepHandlerConfig) -> Self {
        Self { config }
    }
}

/// Reserve Inventory: Reserve items in warehouse with expiration times
#[derive(Debug)]
pub struct ReserveInventoryHandler {
    #[allow(dead_code)] // api compatibility
    config: StepHandlerConfig,
}

#[async_trait]
impl RustStepHandler for ReserveInventoryHandler {
    async fn call(&self, step_data: &TaskSequenceStep) -> Result<StepExecutionResult> {
        let start_time = std::time::Instant::now();
        let step_uuid = step_data.workflow_step.workflow_step_uuid;

        // Get validated items from previous step
        let validate_order_results = match step_data
            .get_dependency_result_column_value::<serde_json::Value>("validate_order")
        {
            Ok(results) => results,
            Err(e) => {
                error!("Missing result from validate_order: {}", e);
                return Ok(error_result(
                    step_uuid,
                    "validate_order step results not found".to_string(),
                    Some("MISSING_DEPENDENCY_RESULTS".to_string()),
                    Some("DependencyError".to_string()),
                    true, // Retryable - might be available later
                    start_time.elapsed().as_millis() as i64,
                    Some(HashMap::from([(
                        "required_step".to_string(),
                        json!("validate_order"),
                    )])),
                ));
            }
        };

        let validated_items = match validate_order_results
            .get("validated_items")
            .and_then(|v| v.as_array())
        {
            Some(items) => items,
            None => {
                return Ok(error_result(
                    step_uuid,
                    "No validated items found from validate_order step".to_string(),
                    Some("NO_VALIDATED_ITEMS".to_string()),
                    Some("ValidationError".to_string()),
                    false,
                    start_time.elapsed().as_millis() as i64,
                    None,
                ));
            }
        };

        let _customer_id = validate_order_results
            .get("customer_id")
            .and_then(serde_json::Value::as_i64)
            .unwrap_or(0);
        let _order_total = validate_order_results
            .get("order_total")
            .and_then(serde_json::Value::as_f64)
            .unwrap_or(0.0);

        // Generate reservation ID
        let reservation_id = format!(
            "RES-{}-{:X}",
            Utc::now().timestamp(),
            rand::random::<u32>() & 0xFFFF
        );

        // Set reservation expiration (15 minutes from now)
        let expires_at = Utc::now() + chrono::Duration::minutes(15);

        // Reserve each item
        let mut reservations = Vec::new();
        let mut total_value = 0.0;

        for item in validated_items {
            let sku = item
                .get("sku")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");
            let quantity = item
                .get("quantity")
                .and_then(serde_json::Value::as_i64)
                .unwrap_or(0);
            let unit_price = item
                .get("unit_price")
                .and_then(serde_json::Value::as_f64)
                .unwrap_or(0.0);
            let line_total = item
                .get("line_total")
                .and_then(serde_json::Value::as_f64)
                .unwrap_or(0.0);
            let product_name = item
                .get("product_name")
                .and_then(|v| v.as_str())
                .unwrap_or("Unknown");

            // Simulate inventory reservation
            let reservation_result =
                match simulate_inventory_reservation(sku, quantity, &reservation_id) {
                    Ok(result) => result,
                    Err(e) => {
                        return Ok(error_result(
                            step_uuid,
                            format!("Insufficient stock for product {}. {}", sku, e),
                            Some("INSUFFICIENT_STOCK".to_string()),
                            Some("InventoryError".to_string()),
                            true, // Retryable - inventory might be replenished
                            start_time.elapsed().as_millis() as i64,
                            Some(HashMap::from([
                                ("sku".to_string(), json!(sku)),
                                ("requested_quantity".to_string(), json!(quantity)),
                            ])),
                        ));
                    }
                };

            total_value += line_total;

            reservations.push(json!({
                "sku": sku,
                "product_name": product_name,
                "quantity_requested": quantity,
                "quantity_reserved": reservation_result.reserved_quantity,
                "unit_price": unit_price,
                "line_total": line_total,
                "stock_location": reservation_result.location,
                "reservation_reference": reservation_result.reference
            }));
        }

        info!(
            "📦 RESERVE_INVENTORY: Inventory reserved - reservation_id={}, items={}, total=${:.2}",
            reservation_id,
            reservations.len(),
            total_value
        );

        let mut metadata = HashMap::new();
        metadata.insert("operation".to_string(), json!("reserve_inventory"));
        metadata.insert("items_count".to_string(), json!(reservations.len()));
        metadata.insert(
            "input_refs".to_string(),
            json!({
                "validated_items": "sequence.validate_order.result.validated_items",
                "customer_id": "sequence.validate_order.result.customer_id"
            }),
        );

        Ok(success_result(
            step_uuid,
            json!({
                "reservation_id": reservation_id,
                "items_reserved": reservations.len(),
                "reservation_status": "confirmed",
                "total_reserved_value": total_value,
                "expires_at": expires_at.to_rfc3339(),
                "reserved_at": Utc::now().to_rfc3339(),
                "reservation_details": reservations
            }),
            start_time.elapsed().as_millis() as i64,
            Some(metadata),
        ))
    }

    fn name(&self) -> &'static str {
        "reserve_inventory"
    }

    fn new(config: StepHandlerConfig) -> Self {
        Self { config }
    }
}

/// Process Payment: Charge payment method through gateway
#[derive(Debug)]
pub struct ProcessPaymentHandler {
    #[allow(dead_code)] // api compatibility
    config: StepHandlerConfig,
}

#[async_trait]
impl RustStepHandler for ProcessPaymentHandler {
    async fn call(&self, step_data: &TaskSequenceStep) -> Result<StepExecutionResult> {
        let start_time = std::time::Instant::now();
        let step_uuid = step_data.workflow_step.workflow_step_uuid;

        // Get order validation and inventory reservation results
        let validate_order_results = match step_data
            .get_dependency_result_column_value::<serde_json::Value>("validate_order")
        {
            Ok(results) => results,
            Err(_) => {
                return Ok(error_result(
                    step_uuid,
                    "validate_order step results not found".to_string(),
                    Some("MISSING_VALIDATION_RESULTS".to_string()),
                    Some("DependencyError".to_string()),
                    true,
                    start_time.elapsed().as_millis() as i64,
                    None,
                ));
            }
        };

        let reserve_inventory_results = match step_data
            .get_dependency_result_column_value::<serde_json::Value>("reserve_inventory")
        {
            Ok(results) => results,
            Err(_) => {
                return Ok(error_result(
                    step_uuid,
                    "reserve_inventory step results not found".to_string(),
                    Some("MISSING_RESERVATION_RESULTS".to_string()),
                    Some("DependencyError".to_string()),
                    true,
                    start_time.elapsed().as_millis() as i64,
                    None,
                ));
            }
        };

        // Extract payment info from task context
        let payment_info = match step_data.get_context_field::<serde_json::Value>("payment") {
            Ok(info) => info,
            Err(_) => {
                return Ok(error_result(
                    step_uuid,
                    "Payment information is required".to_string(),
                    Some("MISSING_PAYMENT_INFO".to_string()),
                    Some("ValidationError".to_string()),
                    false,
                    start_time.elapsed().as_millis() as i64,
                    None,
                ));
            }
        };

        let payment_method = match payment_info.get("method").and_then(|v| v.as_str()) {
            Some(method) => method,
            None => {
                return Ok(error_result(
                    step_uuid,
                    "Payment method is required".to_string(),
                    Some("MISSING_PAYMENT_METHOD".to_string()),
                    Some("ValidationError".to_string()),
                    false,
                    start_time.elapsed().as_millis() as i64,
                    None,
                ));
            }
        };

        let _payment_token = match payment_info.get("token").and_then(|v| v.as_str()) {
            Some(token) => token,
            None => {
                return Ok(error_result(
                    step_uuid,
                    "Payment token is required".to_string(),
                    Some("MISSING_PAYMENT_TOKEN".to_string()),
                    Some("ValidationError".to_string()),
                    false,
                    start_time.elapsed().as_millis() as i64,
                    None,
                ));
            }
        };

        let amount_to_charge = validate_order_results
            .get("order_total")
            .and_then(serde_json::Value::as_f64)
            .unwrap_or(0.0);
        let _customer_id = validate_order_results
            .get("customer_id")
            .and_then(serde_json::Value::as_i64)
            .unwrap_or(0);
        let _reservation_id = reserve_inventory_results
            .get("reservation_id")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        // Generate payment ID
        let payment_id = format!(
            "PAY-{}-{:X}",
            Utc::now().timestamp(),
            rand::random::<u32>() & 0xFFFFFF
        );

        // Simulate payment gateway interaction
        let gateway_response =
            simulate_payment_gateway_call(amount_to_charge, payment_method, &payment_id);

        if gateway_response.status != "succeeded" {
            return Ok(error_result(
                step_uuid,
                format!(
                    "Payment failed: {}",
                    gateway_response
                        .error_message
                        .unwrap_or("Unknown error".to_string())
                ),
                Some("PAYMENT_DECLINED".to_string()),
                Some("PaymentError".to_string()),
                false, // Payment failures are generally not retryable
                start_time.elapsed().as_millis() as i64,
                Some(HashMap::from([
                    (
                        "gateway_error".to_string(),
                        json!(gateway_response.error_code.unwrap_or("unknown".to_string())),
                    ),
                    ("amount".to_string(), json!(amount_to_charge)),
                ])),
            ));
        }

        info!(
            "💳 PROCESS_PAYMENT: Payment processed - payment_id={}, amount=${:.2}, method={}",
            payment_id, amount_to_charge, payment_method
        );

        let mut metadata = HashMap::new();
        metadata.insert("operation".to_string(), json!("process_payment"));
        metadata.insert(
            "http_headers".to_string(),
            json!({
                "X-Payment-Gateway": "stripe",
                "X-Gateway-Request-ID": gateway_response.transaction_id,
                "X-Idempotency-Key": payment_id
            }),
        );
        metadata.insert(
            "execution_hints".to_string(),
            json!({
                "gateway_response_time_ms": gateway_response.processing_time_ms,
                "gateway_fee_amount": gateway_response.gateway_fee,
                "requires_3ds_authentication": false
            }),
        );
        metadata.insert(
            "input_refs".to_string(),
            json!({
                "amount": "sequence.validate_order.result.order_total",
                "reservation_id": "sequence.reserve_inventory.result.reservation_id",
                "payment_info": "task.context.payment"
            }),
        );

        Ok(success_result(
            step_uuid,
            json!({
                "payment_processed": true,
                "payment_id": payment_id,
                "transaction_id": gateway_response.transaction_id,
                "amount_charged": gateway_response.amount,
                "payment_method_used": payment_method,
                "gateway_response": {
                    "status": gateway_response.status,
                    "processing_time_ms": gateway_response.processing_time_ms,
                    "gateway_fee": gateway_response.gateway_fee,
                    "reference": gateway_response.reference
                },
                "processed_at": Utc::now().to_rfc3339(),
                "payment_status": "completed"
            }),
            start_time.elapsed().as_millis() as i64,
            Some(metadata),
        ))
    }

    fn name(&self) -> &'static str {
        "process_payment"
    }

    fn new(config: StepHandlerConfig) -> Self {
        Self { config }
    }
}

/// Ship Order: Create shipping labels and generate tracking numbers
#[derive(Debug)]
pub struct ShipOrderHandler {
    #[allow(dead_code)] // api compatibility
    config: StepHandlerConfig,
}

#[async_trait]
impl RustStepHandler for ShipOrderHandler {
    async fn call(&self, step_data: &TaskSequenceStep) -> Result<StepExecutionResult> {
        let start_time = std::time::Instant::now();
        let step_uuid = step_data.workflow_step.workflow_step_uuid;

        // Get results from all previous steps
        let validate_order_results =
            step_data.get_dependency_result_column_value::<serde_json::Value>("validate_order")?;
        let reserve_inventory_results = step_data
            .get_dependency_result_column_value::<serde_json::Value>("reserve_inventory")?;
        let process_payment_results =
            step_data.get_dependency_result_column_value::<serde_json::Value>("process_payment")?;

        // Extract shipping info from task context
        let shipping_info = match step_data.get_context_field::<serde_json::Value>("shipping") {
            Ok(info) => info,
            Err(_) => {
                return Ok(error_result(
                    step_uuid,
                    "Shipping information is required".to_string(),
                    Some("MISSING_SHIPPING_INFO".to_string()),
                    Some("ValidationError".to_string()),
                    false,
                    start_time.elapsed().as_millis() as i64,
                    None,
                ));
            }
        };

        let _shipping_address = match shipping_info.get("address") {
            Some(addr) => addr,
            None => {
                return Ok(error_result(
                    step_uuid,
                    "Shipping address is required".to_string(),
                    Some("MISSING_SHIPPING_ADDRESS".to_string()),
                    Some("ValidationError".to_string()),
                    false,
                    start_time.elapsed().as_millis() as i64,
                    None,
                ));
            }
        };

        let shipping_method = shipping_info
            .get("method")
            .and_then(|v| v.as_str())
            .unwrap_or("standard");
        let empty_items = vec![];
        let validated_items = validate_order_results
            .get("validated_items")
            .and_then(|v| v.as_array())
            .unwrap_or(&empty_items);
        let _customer_id = validate_order_results
            .get("customer_id")
            .and_then(serde_json::Value::as_i64)
            .unwrap_or(0);
        let _reservation_id = reserve_inventory_results
            .get("reservation_id")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let _payment_id = process_payment_results
            .get("payment_id")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        // Generate shipment ID
        let shipment_id = format!(
            "SHIP-{}-{:X}",
            Utc::now().timestamp(),
            rand::random::<u32>() & 0xFFFF
        );

        // Simulate shipping carrier API call
        let carrier_response =
            simulate_shipping_carrier_call(&shipment_id, validated_items, shipping_method);

        // Calculate estimated delivery
        let estimated_delivery = calculate_delivery_estimate(shipping_method);

        info!(
            "📦 SHIP_ORDER: Shipment created - shipment_id={}, tracking={}, carrier={}",
            shipment_id, carrier_response.tracking_number, carrier_response.carrier
        );

        let mut metadata = HashMap::new();
        metadata.insert("operation".to_string(), json!("ship_order"));
        metadata.insert(
            "http_headers".to_string(),
            json!({
                "X-Carrier-Name": carrier_response.carrier,
                "X-Tracking-Number": carrier_response.tracking_number,
                "X-Carrier-Request-ID": format!("req-{:x}", rand::random::<u32>() & 0xFFFFFF)
            }),
        );
        metadata.insert(
            "execution_hints".to_string(),
            json!({
                "carrier_api_response_time_ms": carrier_response.api_response_time,
                "label_generation_time_ms": carrier_response.label_generation_time,
                "international_shipment": false
            }),
        );
        metadata.insert(
            "input_refs".to_string(),
            json!({
                "items": "sequence.validate_order.result.validated_items",
                "reservation_id": "sequence.reserve_inventory.result.reservation_id",
                "payment_id": "sequence.process_payment.result.payment_id",
                "shipping_info": "task.context.shipping"
            }),
        );

        Ok(success_result(
            step_uuid,
            json!({
                "shipment_id": shipment_id,
                "tracking_number": carrier_response.tracking_number,
                "shipping_status": "label_created",
                "estimated_delivery": estimated_delivery,
                "shipping_cost": carrier_response.cost,
                "carrier": carrier_response.carrier,
                "service_type": carrier_response.service,
                "label_url": carrier_response.label_url,
                "processed_at": Utc::now().to_rfc3339()
            }),
            start_time.elapsed().as_millis() as i64,
            Some(metadata),
        ))
    }

    fn name(&self) -> &'static str {
        "ship_order"
    }

    fn new(config: StepHandlerConfig) -> Self {
        Self { config }
    }
}

// Helper structures and functions for simulation

#[derive(Debug)]
struct ProductData {
    name: String,
    stock: i32,
    category: String,
}

#[derive(Debug)]
struct InventoryReservation {
    reserved_quantity: i64,
    location: String,
    reference: String,
}

#[derive(Debug)]
struct PaymentGatewayResponse {
    status: String,
    transaction_id: String,
    amount: f64,
    gateway_fee: f64,
    reference: String,
    processing_time_ms: i32,
    error_code: Option<String>,
    error_message: Option<String>,
}

#[derive(Debug)]
struct ShippingCarrierResponse {
    tracking_number: String,
    label_url: String,
    cost: f64,
    carrier: String,
    service: String,
    api_response_time: i32,
    label_generation_time: i32,
}

fn simulate_product_lookup(sku: &str) -> Result<ProductData, String> {
    match sku {
        "WIDGET-001" => Ok(ProductData {
            name: "Premium Widget A".to_string(),
            stock: 100,
            category: "widgets".to_string(),
        }),
        "WIDGET-002" => Ok(ProductData {
            name: "Deluxe Widget B".to_string(),
            stock: 50,
            category: "widgets".to_string(),
        }),
        "GADGET-002" => Ok(ProductData {
            name: "Standard Gadget C".to_string(),
            stock: 200,
            category: "gadgets".to_string(),
        }),
        _ => Err(format!("Product {} not found", sku)),
    }
}

fn simulate_inventory_reservation(
    sku: &str,
    quantity: i64,
    reservation_id: &str,
) -> Result<InventoryReservation, String> {
    let (available_stock, location) = match sku {
        "WIDGET-001" => (100, "WH-EAST-A1"),
        "WIDGET-002" => (50, "WH-WEST-B2"),
        "GADGET-002" => (200, "WH-CENTRAL-C3"),
        _ => return Err(format!("Product {} not found in inventory system", sku)),
    };

    if available_stock < quantity as i32 {
        return Err(format!(
            "Available: {}, Requested: {}",
            available_stock, quantity
        ));
    }

    Ok(InventoryReservation {
        reserved_quantity: quantity,
        location: location.to_string(),
        reference: format!("{}-{}", reservation_id, sku),
    })
}

fn simulate_payment_gateway_call(
    amount: f64,
    _method: &str,
    payment_id: &str,
) -> PaymentGatewayResponse {
    let processing_time = i32::from(rand::random::<u16>() % 200 + 50); // 50-250ms

    PaymentGatewayResponse {
        status: "succeeded".to_string(),
        transaction_id: format!("TXN-{:X}", rand::random::<u32>()),
        amount,
        gateway_fee: (amount * 0.029 * 100.0).round() / 100.0, // 2.9% fee rounded to cents
        reference: format!("{}-{}", payment_id, Utc::now().timestamp()),
        processing_time_ms: processing_time,
        error_code: None,
        error_message: None,
    }
}

fn simulate_shipping_carrier_call(
    shipment_id: &str,
    items: &[Value],
    method: &str,
) -> ShippingCarrierResponse {
    let (carrier, service) = match method {
        "express" => ("FedEx", "FedEx Express"),
        "overnight" => ("FedEx", "FedEx Overnight"),
        _ => ("UPS", "UPS Ground"),
    };

    // Calculate shipping cost
    let total_weight = items.len() as f64 * 0.5; // 0.5 lbs per item
    let base_cost = match method {
        "overnight" => 25.0,
        "express" => 15.0,
        _ => 8.99,
    };
    let weight_cost = if total_weight > 1.0 {
        (total_weight - 1.0) * 2.50
    } else {
        0.0
    };
    let shipping_cost = (base_cost + weight_cost * 100.0).round() / 100.0;

    let tracking_number = match carrier {
        "FedEx" => format!("1Z{:X}", rand::random::<u32>()),
        "UPS" => format!("UPS{:X}", rand::random::<u32>() & 0xFFFFFF),
        _ => format!("TRK{:X}", rand::random::<u32>() & 0xFFFFFFF),
    };

    ShippingCarrierResponse {
        tracking_number,
        label_url: format!(
            "https://labels.{}.com/{}.pdf",
            carrier.to_lowercase(),
            shipment_id
        ),
        cost: shipping_cost,
        carrier: carrier.to_string(),
        service: service.to_string(),
        api_response_time: i32::from(rand::random::<u16>() % 300 + 100), // 100-400ms
        label_generation_time: i32::from(rand::random::<u16>() % 100 + 50), // 50-150ms
    }
}

fn calculate_delivery_estimate(method: &str) -> String {
    let business_days = match method {
        "overnight" => 1,
        "express" => 2,
        _ => 5,
    };

    let mut delivery_date = Utc::now().date_naive();
    let mut days_added = 0;

    while days_added < business_days {
        delivery_date += chrono::Duration::days(1);
        // Skip weekends (Saturday=6, Sunday=0 in chrono)
        if delivery_date.weekday().num_days_from_sunday() != 0
            && delivery_date.weekday().num_days_from_sunday() != 6
        {
            days_added += 1;
        }
    }

    delivery_date.format("%Y-%m-%d").to_string()
}
