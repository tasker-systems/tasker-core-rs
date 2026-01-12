/**
 * E-commerce Order Processing Step Handlers for Blog Post 01.
 *
 * Implements the complete e-commerce checkout workflow:
 * 1. ValidateCart: Validate cart items, check availability, calculate totals
 * 2. ProcessPayment: Process customer payment using mock payment service
 * 3. UpdateInventory: Reserve inventory for order items
 * 4. CreateOrder: Create order record with all details
 * 5. SendConfirmation: Send order confirmation email to customer
 *
 * TAS-137 Best Practices Demonstrated:
 * - getInput<T>() for task context field access (cross-language standard)
 * - getDependencyResult() for upstream step results (auto-unwraps)
 * - getDependencyField() for nested field extraction from dependencies
 * - ErrorType.PERMANENT_ERROR for non-recoverable failures
 * - ErrorType.RETRYABLE_ERROR for transient issues
 */

import { StepHandler } from '../../../../../../src/handler/base.js';
import { ErrorType } from '../../../../../../src/types/error-type.js';
import type { StepContext } from '../../../../../../src/types/step-context.js';
import type { StepHandlerResult } from '../../../../../../src/types/step-handler-result.js';

// =============================================================================
// Types
// =============================================================================

interface CartItem {
  product_id: number;
  quantity: number;
}

interface CustomerInfo {
  email: string;
  name: string;
  phone?: string;
}

interface PaymentInfo {
  method: string;
  token: string;
  amount: number;
}

interface Product {
  id: number;
  name: string;
  price: number;
  stock: number;
}

interface ValidatedItem {
  product_id: number;
  name: string;
  price: number;
  quantity: number;
  line_total: number;
}

// =============================================================================
// Mock Data
// =============================================================================

const PRODUCTS: Record<number, Product> = {
  1: { id: 1, name: 'Widget A', price: 29.99, stock: 100 },
  2: { id: 2, name: 'Widget B', price: 49.99, stock: 50 },
  3: { id: 3, name: 'Widget C', price: 19.99, stock: 25 },
  4: { id: 4, name: 'Widget D', price: 99.99, stock: 0 },
  5: { id: 5, name: 'Widget E', price: 14.99, stock: 10 },
};

const TAX_RATE = 0.08;
const SHIPPING_COST = 5.99;

// =============================================================================
// Utility Functions
// =============================================================================

function generateId(prefix: string): string {
  const hex = Array.from({ length: 12 }, () => Math.floor(Math.random() * 16).toString(16)).join(
    ''
  );
  return `${prefix}_${hex}`;
}

function generateOrderNumber(): string {
  const today = new Date().toISOString().slice(0, 10).replace(/-/g, '');
  const suffix = Array.from({ length: 8 }, () => Math.floor(Math.random() * 16).toString(16))
    .join('')
    .toUpperCase();
  return `ORD-${today}-${suffix}`;
}

// =============================================================================
// Handlers
// =============================================================================

/**
 * Step 1: Validate cart items, check availability, calculate totals.
 *
 * Input (from task context):
 *   cart_items: Array of {product_id, quantity}
 *
 * Output:
 *   validated_items: Array of validated items with prices
 *   subtotal: Subtotal before tax/shipping
 *   tax: Tax amount
 *   shipping: Shipping cost
 *   total: Final total
 *   item_count: Total number of items
 *
 * TAS-137 Best Practices:
 * - Uses getInput<T>() for task context access
 */
export class ValidateCartHandler extends StepHandler {
  static handlerName = 'Ecommerce.StepHandlers.ValidateCartHandler';
  static handlerVersion = '1.0.0';

  async call(context: StepContext): Promise<StepHandlerResult> {
    // TAS-137: Use getInput<T>() for task context access
    const cartItems = context.getInput<CartItem[]>('cart_items');

    if (!cartItems || cartItems.length === 0) {
      return this.failure(
        'Cart items are required but were not provided',
        ErrorType.PERMANENT_ERROR,
        false
      );
    }

    const validatedItems: ValidatedItem[] = [];
    let subtotal = 0;

    for (const item of cartItems) {
      const product = PRODUCTS[item.product_id];

      if (!product) {
        return this.failure(
          `Product ${item.product_id} not found`,
          ErrorType.PERMANENT_ERROR,
          false
        );
      }

      if (product.stock < item.quantity) {
        return this.failure(
          `Insufficient stock for ${product.name}. Available: ${product.stock}, Requested: ${item.quantity}`,
          ErrorType.RETRYABLE_ERROR,
          true
        );
      }

      const lineTotal = product.price * item.quantity;
      subtotal += lineTotal;

      validatedItems.push({
        product_id: product.id,
        name: product.name,
        price: product.price,
        quantity: item.quantity,
        line_total: Math.round(lineTotal * 100) / 100,
      });
    }

    const tax = Math.round(subtotal * TAX_RATE * 100) / 100;
    const total = Math.round((subtotal + tax + SHIPPING_COST) * 100) / 100;
    const itemCount = validatedItems.reduce((sum, item) => sum + item.quantity, 0);

    return this.success(
      {
        validated_items: validatedItems,
        subtotal: Math.round(subtotal * 100) / 100,
        tax,
        shipping: SHIPPING_COST,
        total,
        item_count: itemCount,
      },
      {
        operation: 'validate_cart',
        products_checked: validatedItems.length,
        total_items: itemCount,
      }
    );
  }
}

/**
 * Step 2: Process customer payment using mock payment service.
 *
 * Input (from task context):
 *   payment_info: Payment method, token, and amount
 *
 * Input (from dependency):
 *   validate_cart.total: Total amount to charge
 *
 * Output:
 *   payment_id: Payment identifier
 *   transaction_id: Transaction reference
 *   status: Payment status
 *   amount_charged: Amount charged
 *
 * TAS-137 Best Practices:
 * - Uses getInput<T>() for task context access
 * - Uses getDependencyField() for nested field extraction
 */
export class ProcessPaymentHandler extends StepHandler {
  static handlerName = 'Ecommerce.StepHandlers.ProcessPaymentHandler';
  static handlerVersion = '1.0.0';

  async call(context: StepContext): Promise<StepHandlerResult> {
    // TAS-137: Use getInput<T>() for task context access
    const paymentInfo = context.getInput<PaymentInfo>('payment_info');

    // TAS-137: Use getDependencyField() for nested field extraction
    const cartTotal = context.getDependencyField('validate_cart', 'total') as number | null;

    if (!paymentInfo) {
      return this.failure(
        'Payment information is required but was not provided',
        ErrorType.PERMANENT_ERROR,
        false
      );
    }

    if (cartTotal === null || cartTotal === undefined) {
      return this.failure(
        'Cart total is required but was not found from validate_cart step',
        ErrorType.PERMANENT_ERROR,
        false
      );
    }

    // Simulate payment processing with test tokens
    const paymentResult = this.simulatePayment(paymentInfo.token, cartTotal);

    if (paymentResult.status === 'card_declined') {
      return this.failure(
        `Payment declined: ${paymentResult.error}`,
        ErrorType.PERMANENT_ERROR,
        false
      );
    }

    if (paymentResult.status === 'insufficient_funds') {
      return this.failure(
        `Payment failed: ${paymentResult.error}`,
        ErrorType.PERMANENT_ERROR,
        false
      );
    }

    if (paymentResult.status === 'timeout') {
      return this.failure(
        `Payment gateway timeout: ${paymentResult.error}`,
        ErrorType.RETRYABLE_ERROR,
        true
      );
    }

    return this.success(
      {
        payment_id: paymentResult.payment_id,
        transaction_id: paymentResult.transaction_id,
        status: paymentResult.status,
        amount_charged: paymentResult.amount_charged,
        currency: 'USD',
        payment_method: paymentInfo.method,
      },
      {
        operation: 'process_payment',
        gateway: 'MockPaymentGateway',
      }
    );
  }

  private simulatePayment(
    token: string,
    amount: number
  ): {
    status: string;
    error?: string;
    payment_id?: string;
    transaction_id?: string;
    amount_charged?: number;
  } {
    // Test tokens for different scenarios
    const errorResponses: Record<string, { status: string; error: string }> = {
      tok_test_declined: { status: 'card_declined', error: 'Card was declined by issuer' },
      tok_test_insufficient_funds: { status: 'insufficient_funds', error: 'Insufficient funds' },
      tok_test_network_error: { status: 'timeout', error: 'Payment gateway timeout' },
    };

    if (token in errorResponses) {
      return errorResponses[token];
    }

    // Success case
    return {
      status: 'succeeded',
      payment_id: generateId('pay'),
      transaction_id: generateId('txn'),
      amount_charged: amount,
    };
  }
}

/**
 * Step 3: Reserve inventory for order items.
 *
 * Input (from dependency):
 *   validate_cart.validated_items: Items to reserve
 *
 * Output:
 *   updated_products: Products with updated stock info
 *   total_items_reserved: Total quantity reserved
 *   inventory_log_id: Log identifier
 *
 * TAS-137 Best Practices:
 * - Uses getDependencyResult() for upstream step results
 */
export class UpdateInventoryHandler extends StepHandler {
  static handlerName = 'Ecommerce.StepHandlers.UpdateInventoryHandler';
  static handlerVersion = '1.0.0';

  async call(context: StepContext): Promise<StepHandlerResult> {
    // TAS-137: Use getDependencyResult() for upstream step results
    const cartValidation = context.getDependencyResult('validate_cart') as Record<
      string,
      unknown
    > | null;

    if (!cartValidation || !cartValidation.validated_items) {
      return this.failure(
        'Validated cart items are required but were not found from validate_cart step',
        ErrorType.PERMANENT_ERROR,
        false
      );
    }

    const validatedItems = cartValidation.validated_items as ValidatedItem[];
    const updatedProducts: Array<{
      product_id: number;
      name: string;
      previous_stock: number;
      new_stock: number;
      quantity_reserved: number;
      reservation_id: string;
    }> = [];

    let totalItemsReserved = 0;

    for (const item of validatedItems) {
      const product = PRODUCTS[item.product_id];

      if (!product) {
        return this.failure(
          `Product ${item.product_id} not found`,
          ErrorType.PERMANENT_ERROR,
          false
        );
      }

      if (product.stock < item.quantity) {
        return this.failure(
          `Stock not available for ${product.name}. Available: ${product.stock}, Needed: ${item.quantity}`,
          ErrorType.RETRYABLE_ERROR,
          true
        );
      }

      updatedProducts.push({
        product_id: product.id,
        name: product.name,
        previous_stock: product.stock,
        new_stock: product.stock - item.quantity,
        quantity_reserved: item.quantity,
        reservation_id: generateId('rsv'),
      });

      totalItemsReserved += item.quantity;
    }

    return this.success(
      {
        updated_products: updatedProducts,
        total_items_reserved: totalItemsReserved,
        inventory_log_id: generateId('log'),
        updated_at: new Date().toISOString(),
      },
      {
        operation: 'update_inventory',
        products_updated: updatedProducts.length,
      }
    );
  }
}

/**
 * Step 4: Create order record with all details from upstream steps.
 *
 * Input (from task context):
 *   customer_info: Customer information
 *
 * Input (from dependencies):
 *   validate_cart: Cart validation results
 *   process_payment: Payment results
 *   update_inventory: Inventory reservation results
 *
 * Output:
 *   order_id: Order identifier
 *   order_number: Human-readable order number
 *   status: Order status
 *   total_amount: Order total
 *
 * TAS-137 Best Practices:
 * - Uses getInput<T>() for task context access
 * - Uses getDependencyResult() for multiple upstream dependencies
 */
export class CreateOrderHandler extends StepHandler {
  static handlerName = 'Ecommerce.StepHandlers.CreateOrderHandler';
  static handlerVersion = '1.0.0';

  async call(context: StepContext): Promise<StepHandlerResult> {
    // TAS-137: Use getInput<T>() for task context access
    const customerInfo = context.getInput<CustomerInfo>('customer_info');

    // TAS-137: Use getDependencyResult() for upstream step results
    const cartValidation = context.getDependencyResult('validate_cart') as Record<
      string,
      unknown
    > | null;
    const paymentResult = context.getDependencyResult('process_payment') as Record<
      string,
      unknown
    > | null;
    const inventoryResult = context.getDependencyResult('update_inventory') as Record<
      string,
      unknown
    > | null;

    // Validate required inputs
    if (!customerInfo) {
      return this.failure(
        'Customer information is required but was not provided',
        ErrorType.PERMANENT_ERROR,
        false
      );
    }

    if (!cartValidation || !cartValidation.validated_items) {
      return this.failure(
        'Cart validation results are required but were not found from validate_cart step',
        ErrorType.PERMANENT_ERROR,
        false
      );
    }

    if (!paymentResult || !paymentResult.payment_id) {
      return this.failure(
        'Payment results are required but were not found from process_payment step',
        ErrorType.PERMANENT_ERROR,
        false
      );
    }

    if (!inventoryResult || !inventoryResult.updated_products) {
      return this.failure(
        'Inventory results are required but were not found from update_inventory step',
        ErrorType.PERMANENT_ERROR,
        false
      );
    }

    const orderId = Math.floor(Math.random() * 9000) + 1000;
    const orderNumber = generateOrderNumber();
    const now = new Date().toISOString();

    // Calculate estimated delivery (7 days from now)
    const deliveryDate = new Date();
    deliveryDate.setDate(deliveryDate.getDate() + 7);
    const estimatedDelivery = deliveryDate.toLocaleDateString('en-US', {
      year: 'numeric',
      month: 'long',
      day: 'numeric',
    });

    return this.success(
      {
        order_id: orderId,
        order_number: orderNumber,
        status: 'confirmed',
        total_amount: cartValidation.total,
        customer_email: customerInfo.email,
        created_at: now,
        estimated_delivery: estimatedDelivery,
      },
      {
        operation: 'create_order',
        order_id: orderId,
        order_number: orderNumber,
        item_count: cartValidation.item_count,
      }
    );
  }
}

/**
 * Step 5: Send order confirmation email to customer.
 *
 * Input (from task context):
 *   customer_info: Customer information
 *
 * Input (from dependencies):
 *   validate_cart: Cart details for email
 *   create_order: Order details for email
 *
 * Output:
 *   email_id: Email message identifier
 *   recipient: Email recipient
 *   status: Send status
 *
 * TAS-137 Best Practices:
 * - Uses getInput<T>() for task context access
 * - Uses getDependencyField() for specific fields from dependencies
 */
export class SendConfirmationHandler extends StepHandler {
  static handlerName = 'Ecommerce.StepHandlers.SendConfirmationHandler';
  static handlerVersion = '1.0.0';

  async call(context: StepContext): Promise<StepHandlerResult> {
    // TAS-137: Use getInput<T>() for task context access
    const customerInfo = context.getInput<CustomerInfo>('customer_info');

    // TAS-137: Use getDependencyField() for specific fields
    const orderNumber = context.getDependencyField('create_order', 'order_number') as string | null;
    const totalAmount = context.getDependencyField('create_order', 'total_amount') as number | null;
    const estimatedDelivery = context.getDependencyField('create_order', 'estimated_delivery') as
      | string
      | null;
    const validatedItems = context.getDependencyField('validate_cart', 'validated_items') as
      | ValidatedItem[]
      | null;

    // Validate required inputs
    if (!customerInfo || !customerInfo.email) {
      return this.failure(
        'Customer email is required but was not provided',
        ErrorType.PERMANENT_ERROR,
        false
      );
    }

    if (!orderNumber) {
      return this.failure(
        'Order number is required but was not found from create_order step',
        ErrorType.PERMANENT_ERROR,
        false
      );
    }

    // Simulate email sending
    const emailId = generateId('eml');
    const now = new Date().toISOString();

    return this.success(
      {
        email_id: emailId,
        recipient: customerInfo.email,
        subject: `Order Confirmation - ${orderNumber}`,
        status: 'sent',
        sent_at: now,
        template: 'order_confirmation',
        template_data: {
          customer_name: customerInfo.name,
          order_number: orderNumber,
          total_amount: totalAmount,
          estimated_delivery: estimatedDelivery,
          items: validatedItems,
        },
      },
      {
        operation: 'send_confirmation',
        email_service: 'MockEmailService',
        recipient: customerInfo.email,
      }
    );
  }
}
