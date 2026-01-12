//! # E-commerce Order Processing Handlers
//!
//! Native Rust implementation of the e-commerce order processing workflow pattern.
//! This module demonstrates real-world e-commerce checkout with external service integration.
//!
//! ## Workflow Pattern (5 steps)
//!
//! 1. **ValidateCart**: Validate cart items, check availability, calculate totals
//! 2. **ProcessPayment**: Process customer payment using mock payment service
//! 3. **UpdateInventory**: Reserve inventory for order items
//! 4. **CreateOrder**: Create order record with all details
//! 5. **SendConfirmation**: Send order confirmation email to customer
//!
//! ## TAS-91: Blog Post 01 - E-commerce Integration (Rust Implementation)
//!
//! This module matches the Python and TypeScript implementations for cross-language
//! consistency demonstration in Blog Post 01.
//!
//! ## TAS-137 Best Practices Demonstrated
//!
//! - `get_input::<T>()` for task context field access (cross-language standard)
//! - `get_dependency_result_column_value::<T>()` for upstream step results
//! - `get_dependency_field()` for nested field access
//! - Proper error classification (retryable vs permanent)

use super::{error_result, success_result, RustStepHandler, StepHandlerConfig};
use anyhow::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::HashMap;
use tasker_shared::messaging::StepExecutionResult;
use tasker_shared::types::TaskSequenceStep;
use tracing::{error, info};
use uuid::Uuid;

// ============================================================================
// Data Types
// ============================================================================

/// Cart item from task input
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CartItem {
    pub product_id: i64,
    pub quantity: i64,
}

/// Customer information from task input
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CustomerInfo {
    pub email: String,
    pub name: String,
    pub phone: Option<String>,
}

/// Payment information from task input
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PaymentInfo {
    pub method: String,
    pub token: String,
    pub amount: f64,
}

/// Product catalog entry (simulated)
#[derive(Debug, Clone)]
struct Product {
    id: i64,
    name: String,
    price: f64,
    stock: i64,
}

/// Get simulated product catalog
fn get_product_catalog() -> HashMap<i64, Product> {
    let mut catalog = HashMap::new();
    catalog.insert(
        1,
        Product {
            id: 1,
            name: "Widget A".to_string(),
            price: 29.99,
            stock: 100,
        },
    );
    catalog.insert(
        2,
        Product {
            id: 2,
            name: "Widget B".to_string(),
            price: 49.99,
            stock: 50,
        },
    );
    catalog.insert(
        3,
        Product {
            id: 3,
            name: "Widget C".to_string(),
            price: 99.99,
            stock: 25,
        },
    );
    catalog
}

// ============================================================================
// Step 1: Validate Cart Handler
// ============================================================================

/// Validates cart items, checks availability, and calculates totals.
///
/// TAS-137 Best Practices:
/// - Uses `get_input::<T>()` for task context access
/// - Returns structured validation results
#[derive(Debug)]
pub struct ValidateCartHandler {
    #[expect(
        dead_code,
        reason = "Required for API compatibility with handler config pattern"
    )]
    config: StepHandlerConfig,
}

#[async_trait]
impl RustStepHandler for ValidateCartHandler {
    async fn call(&self, step_data: &TaskSequenceStep) -> Result<StepExecutionResult> {
        let start_time = std::time::Instant::now();
        let step_uuid = step_data.workflow_step.workflow_step_uuid;

        // TAS-137: Use get_input() for task context access
        let cart_items: Vec<CartItem> = match step_data.get_input("cart_items") {
            Ok(items) => items,
            Err(e) => {
                error!("Missing cart_items in task context: {}", e);
                return Ok(error_result(
                    step_uuid,
                    "Cart items are required".to_string(),
                    Some("MISSING_CART_ITEMS".to_string()),
                    Some("ValidationError".to_string()),
                    false, // Not retryable - data validation error
                    start_time.elapsed().as_millis() as i64,
                    None,
                ));
            }
        };

        if cart_items.is_empty() {
            return Ok(error_result(
                step_uuid,
                "Cart cannot be empty".to_string(),
                Some("EMPTY_CART".to_string()),
                Some("ValidationError".to_string()),
                false,
                start_time.elapsed().as_millis() as i64,
                None,
            ));
        }

        let catalog = get_product_catalog();
        let mut validated_items = Vec::new();
        let mut subtotal = 0.0;
        let mut item_count = 0i64;

        for cart_item in &cart_items {
            let product = match catalog.get(&cart_item.product_id) {
                Some(p) => p,
                None => {
                    return Ok(error_result(
                        step_uuid,
                        format!("Product {} not found", cart_item.product_id),
                        Some("PRODUCT_NOT_FOUND".to_string()),
                        Some("ValidationError".to_string()),
                        false,
                        start_time.elapsed().as_millis() as i64,
                        Some(HashMap::from([(
                            "product_id".to_string(),
                            json!(cart_item.product_id),
                        )])),
                    ));
                }
            };

            if cart_item.quantity > product.stock {
                return Ok(error_result(
                    step_uuid,
                    format!(
                        "Insufficient stock for product {}: requested {}, available {}",
                        product.name, cart_item.quantity, product.stock
                    ),
                    Some("INSUFFICIENT_STOCK".to_string()),
                    Some("ValidationError".to_string()),
                    false,
                    start_time.elapsed().as_millis() as i64,
                    Some(HashMap::from([
                        ("product_id".to_string(), json!(cart_item.product_id)),
                        ("requested".to_string(), json!(cart_item.quantity)),
                        ("available".to_string(), json!(product.stock)),
                    ])),
                ));
            }

            let line_total = product.price * (cart_item.quantity as f64);
            subtotal += line_total;
            item_count += cart_item.quantity;

            validated_items.push(json!({
                "product_id": product.id,
                "product_name": product.name,
                "quantity": cart_item.quantity,
                "unit_price": product.price,
                "line_total": line_total
            }));
        }

        // Calculate totals
        let tax = (subtotal * 0.08 * 100.0).round() / 100.0; // 8% tax
        let shipping = 5.99;
        let total = ((subtotal + tax + shipping) * 100.0).round() / 100.0;

        info!(
            "Cart validated: {} items, subtotal=${:.2}, tax=${:.2}, shipping=${:.2}, total=${:.2}",
            item_count, subtotal, tax, shipping, total
        );

        Ok(success_result(
            step_uuid,
            json!({
                "validated_items": validated_items,
                "subtotal": subtotal,
                "tax": tax,
                "shipping": shipping,
                "total": total,
                "item_count": item_count
            }),
            start_time.elapsed().as_millis() as i64,
            Some(HashMap::from([(
                "validation_type".to_string(),
                json!("cart"),
            )])),
        ))
    }

    fn name(&self) -> &str {
        "ecommerce_validate_cart"
    }

    fn new(config: StepHandlerConfig) -> Self {
        Self { config }
    }
}

// ============================================================================
// Step 2: Process Payment Handler
// ============================================================================

/// Processes customer payment using mock payment service.
///
/// TAS-137 Best Practices:
/// - Uses `get_input::<T>()` for payment info access
/// - Uses `get_dependency_field()` for cart total access
/// - Classifies errors as retryable (network) vs permanent (declined)
#[derive(Debug)]
pub struct ProcessPaymentHandler {
    #[expect(
        dead_code,
        reason = "Required for API compatibility with handler config pattern"
    )]
    config: StepHandlerConfig,
}

#[async_trait]
impl RustStepHandler for ProcessPaymentHandler {
    async fn call(&self, step_data: &TaskSequenceStep) -> Result<StepExecutionResult> {
        let start_time = std::time::Instant::now();
        let step_uuid = step_data.workflow_step.workflow_step_uuid;

        // Get payment info from task context
        let payment_info: PaymentInfo = match step_data.get_input("payment_info") {
            Ok(info) => info,
            Err(e) => {
                error!("Missing payment_info in task context: {}", e);
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

        // Get cart total from validate_cart step
        let cart_total: f64 = match step_data.get_dependency_field("validate_cart", &["total"]) {
            Ok(total) => total,
            Err(e) => {
                error!("Missing total from validate_cart: {}", e);
                return Ok(error_result(
                    step_uuid,
                    "Cart validation required before payment".to_string(),
                    Some("MISSING_DEPENDENCY".to_string()),
                    Some("DependencyError".to_string()),
                    true, // Retryable
                    start_time.elapsed().as_millis() as i64,
                    None,
                ));
            }
        };

        // Simulate payment processing based on token
        let error_responses: HashMap<&str, (&str, &str, bool)> = HashMap::from([
            (
                "tok_test_declined",
                ("CARD_DECLINED", "Card was declined", false),
            ),
            (
                "tok_test_insufficient_funds",
                ("INSUFFICIENT_FUNDS", "Insufficient funds", false),
            ),
            (
                "tok_test_network_error",
                ("NETWORK_ERROR", "Payment gateway unreachable", true),
            ),
        ]);

        if let Some((code, message, retryable)) = error_responses.get(payment_info.token.as_str()) {
            return Ok(error_result(
                step_uuid,
                message.to_string(),
                Some(code.to_string()),
                Some("PaymentError".to_string()),
                *retryable,
                start_time.elapsed().as_millis() as i64,
                Some(HashMap::from([
                    ("payment_method".to_string(), json!(payment_info.method)),
                    ("amount".to_string(), json!(cart_total)),
                ])),
            ));
        }

        // Successful payment
        let transaction_id = format!("txn_{}", &Uuid::new_v4().to_string().replace('-', "")[..12]);

        info!(
            "Payment processed: ${:.2} via {} (txn: {})",
            cart_total, payment_info.method, transaction_id
        );

        Ok(success_result(
            step_uuid,
            json!({
                "transaction_id": transaction_id,
                "amount_charged": cart_total,
                "payment_method": payment_info.method,
                "status": "completed",
                "timestamp": chrono::Utc::now().to_rfc3339()
            }),
            start_time.elapsed().as_millis() as i64,
            Some(HashMap::from([(
                "payment_gateway".to_string(),
                json!("mock_gateway"),
            )])),
        ))
    }

    fn name(&self) -> &str {
        "ecommerce_process_payment"
    }

    fn new(config: StepHandlerConfig) -> Self {
        Self { config }
    }
}

// ============================================================================
// Step 3: Update Inventory Handler
// ============================================================================

/// Reserves inventory for order items.
///
/// TAS-137 Best Practices:
/// - Uses `get_dependency_field()` for validated items from cart step
#[derive(Debug)]
pub struct UpdateInventoryHandler {
    #[expect(
        dead_code,
        reason = "Required for API compatibility with handler config pattern"
    )]
    config: StepHandlerConfig,
}

#[async_trait]
impl RustStepHandler for UpdateInventoryHandler {
    async fn call(&self, step_data: &TaskSequenceStep) -> Result<StepExecutionResult> {
        let start_time = std::time::Instant::now();
        let step_uuid = step_data.workflow_step.workflow_step_uuid;

        // Get validated items from validate_cart step
        let validated_items: Vec<serde_json::Value> =
            match step_data.get_dependency_field("validate_cart", &["validated_items"]) {
                Ok(items) => items,
                Err(e) => {
                    error!("Missing validated_items from validate_cart: {}", e);
                    return Ok(error_result(
                        step_uuid,
                        "Cart validation required before inventory update".to_string(),
                        Some("MISSING_DEPENDENCY".to_string()),
                        Some("DependencyError".to_string()),
                        true,
                        start_time.elapsed().as_millis() as i64,
                        None,
                    ));
                }
            };

        // Process inventory reservations
        let mut reservations = Vec::new();
        let mut total_reserved = 0i64;

        for item in &validated_items {
            let product_id = item["product_id"].as_i64().unwrap_or(0);
            let quantity = item["quantity"].as_i64().unwrap_or(0);
            let reservation_id =
                format!("res_{}", &Uuid::new_v4().to_string().replace('-', "")[..8]);

            reservations.push(json!({
                "reservation_id": reservation_id,
                "product_id": product_id,
                "quantity_reserved": quantity,
                "status": "reserved"
            }));
            total_reserved += quantity;
        }

        info!(
            "Inventory reserved: {} items across {} products",
            total_reserved,
            reservations.len()
        );

        Ok(success_result(
            step_uuid,
            json!({
                "reservations": reservations,
                "total_items_reserved": total_reserved,
                "reservation_timestamp": chrono::Utc::now().to_rfc3339()
            }),
            start_time.elapsed().as_millis() as i64,
            Some(HashMap::from([(
                "inventory_system".to_string(),
                json!("mock_inventory"),
            )])),
        ))
    }

    fn name(&self) -> &str {
        "ecommerce_update_inventory"
    }

    fn new(config: StepHandlerConfig) -> Self {
        Self { config }
    }
}

// ============================================================================
// Step 4: Create Order Handler
// ============================================================================

/// Creates order record with customer, payment, and inventory details.
///
/// TAS-137 Best Practices:
/// - Uses `get_input::<T>()` for customer info
/// - Uses `get_dependency_field()` for upstream step results
/// - Aggregates data from multiple dependency steps
#[derive(Debug)]
pub struct CreateOrderHandler {
    #[expect(
        dead_code,
        reason = "Required for API compatibility with handler config pattern"
    )]
    config: StepHandlerConfig,
}

#[async_trait]
impl RustStepHandler for CreateOrderHandler {
    async fn call(&self, step_data: &TaskSequenceStep) -> Result<StepExecutionResult> {
        let start_time = std::time::Instant::now();
        let step_uuid = step_data.workflow_step.workflow_step_uuid;

        // Get customer info from task context
        let customer_info: CustomerInfo = match step_data.get_input("customer_info") {
            Ok(info) => info,
            Err(e) => {
                error!("Missing customer_info in task context: {}", e);
                return Ok(error_result(
                    step_uuid,
                    "Customer information is required".to_string(),
                    Some("MISSING_CUSTOMER_INFO".to_string()),
                    Some("ValidationError".to_string()),
                    false,
                    start_time.elapsed().as_millis() as i64,
                    None,
                ));
            }
        };

        // Get validated items from validate_cart
        let validated_items: Vec<serde_json::Value> =
            match step_data.get_dependency_field("validate_cart", &["validated_items"]) {
                Ok(items) => items,
                Err(e) => {
                    error!("Missing validated_items from validate_cart: {}", e);
                    return Ok(error_result(
                        step_uuid,
                        "Cart validation required".to_string(),
                        Some("MISSING_DEPENDENCY".to_string()),
                        Some("DependencyError".to_string()),
                        true,
                        start_time.elapsed().as_millis() as i64,
                        None,
                    ));
                }
            };

        // Get payment transaction from process_payment
        let transaction_id: String =
            match step_data.get_dependency_field("process_payment", &["transaction_id"]) {
                Ok(id) => id,
                Err(e) => {
                    error!("Missing transaction_id from process_payment: {}", e);
                    return Ok(error_result(
                        step_uuid,
                        "Payment processing required".to_string(),
                        Some("MISSING_DEPENDENCY".to_string()),
                        Some("DependencyError".to_string()),
                        true,
                        start_time.elapsed().as_millis() as i64,
                        None,
                    ));
                }
            };

        // Get inventory reservations
        let reservations: Vec<serde_json::Value> =
            match step_data.get_dependency_field("update_inventory", &["reservations"]) {
                Ok(res) => res,
                Err(e) => {
                    error!("Missing reservations from update_inventory: {}", e);
                    return Ok(error_result(
                        step_uuid,
                        "Inventory update required".to_string(),
                        Some("MISSING_DEPENDENCY".to_string()),
                        Some("DependencyError".to_string()),
                        true,
                        start_time.elapsed().as_millis() as i64,
                        None,
                    ));
                }
            };

        // Get order total
        let order_total: f64 = step_data
            .get_dependency_field("validate_cart", &["total"])
            .unwrap_or(0.0);

        // Create order record
        let order_id = format!(
            "ORD-{}",
            Uuid::new_v4().to_string().replace('-', "")[..8].to_uppercase()
        );

        info!("Order created: {} for {}", order_id, customer_info.email);

        Ok(success_result(
            step_uuid,
            json!({
                "order_id": order_id,
                "customer": {
                    "email": customer_info.email,
                    "name": customer_info.name,
                    "phone": customer_info.phone
                },
                "items": validated_items,
                "payment": {
                    "transaction_id": transaction_id,
                    "amount": order_total
                },
                "inventory_reservations": reservations,
                "status": "confirmed",
                "created_at": chrono::Utc::now().to_rfc3339()
            }),
            start_time.elapsed().as_millis() as i64,
            Some(HashMap::from([(
                "order_system".to_string(),
                json!("mock_orders"),
            )])),
        ))
    }

    fn name(&self) -> &str {
        "ecommerce_create_order"
    }

    fn new(config: StepHandlerConfig) -> Self {
        Self { config }
    }
}

// ============================================================================
// Step 5: Send Confirmation Handler
// ============================================================================

/// Sends order confirmation email to customer.
///
/// TAS-137 Best Practices:
/// - Uses `get_input::<T>()` for customer info (email address)
/// - Uses `get_dependency_field()` for order details
#[derive(Debug)]
pub struct SendConfirmationHandler {
    #[expect(
        dead_code,
        reason = "Required for API compatibility with handler config pattern"
    )]
    config: StepHandlerConfig,
}

#[async_trait]
impl RustStepHandler for SendConfirmationHandler {
    async fn call(&self, step_data: &TaskSequenceStep) -> Result<StepExecutionResult> {
        let start_time = std::time::Instant::now();
        let step_uuid = step_data.workflow_step.workflow_step_uuid;

        // Get customer info from task context
        let customer_info: CustomerInfo = match step_data.get_input("customer_info") {
            Ok(info) => info,
            Err(e) => {
                error!("Missing customer_info in task context: {}", e);
                return Ok(error_result(
                    step_uuid,
                    "Customer information is required for confirmation".to_string(),
                    Some("MISSING_CUSTOMER_INFO".to_string()),
                    Some("ValidationError".to_string()),
                    false,
                    start_time.elapsed().as_millis() as i64,
                    None,
                ));
            }
        };

        // Get order details from create_order
        let order_id: String = match step_data.get_dependency_field("create_order", &["order_id"]) {
            Ok(id) => id,
            Err(e) => {
                error!("Missing order_id from create_order: {}", e);
                return Ok(error_result(
                    step_uuid,
                    "Order creation required before confirmation".to_string(),
                    Some("MISSING_DEPENDENCY".to_string()),
                    Some("DependencyError".to_string()),
                    true,
                    start_time.elapsed().as_millis() as i64,
                    None,
                ));
            }
        };

        // Get order total
        let order_total: f64 = step_data
            .get_dependency_field("validate_cart", &["total"])
            .unwrap_or(0.0);

        // Get item count
        let item_count: i64 = step_data
            .get_dependency_field("validate_cart", &["item_count"])
            .unwrap_or_default();

        // Simulate sending email
        let email_id = format!(
            "email_{}",
            &Uuid::new_v4().to_string().replace('-', "")[..12]
        );

        info!(
            "Confirmation email sent: {} to {} for order {}",
            email_id, customer_info.email, order_id
        );

        Ok(success_result(
            step_uuid,
            json!({
                "email_id": email_id,
                "recipient": customer_info.email,
                "order_id": order_id,
                "subject": format!("Order Confirmation - {}", order_id),
                "summary": {
                    "item_count": item_count,
                    "order_total": order_total
                },
                "status": "sent",
                "sent_at": chrono::Utc::now().to_rfc3339()
            }),
            start_time.elapsed().as_millis() as i64,
            Some(HashMap::from([(
                "email_provider".to_string(),
                json!("mock_email"),
            )])),
        ))
    }

    fn name(&self) -> &str {
        "ecommerce_send_confirmation"
    }

    fn new(config: StepHandlerConfig) -> Self {
        Self { config }
    }
}
