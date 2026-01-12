/**
 * Data Pipeline Analytics Step Handlers for Blog Post 02.
 *
 * Implements the complete analytics pipeline workflow:
 * 1. ExtractSalesData: Pull sales records (parallel)
 * 2. ExtractInventoryData: Pull inventory records (parallel)
 * 3. ExtractCustomerData: Pull customer records (parallel)
 * 4. TransformSales: Transform sales data for analytics
 * 5. TransformInventory: Transform inventory data for analytics
 * 6. TransformCustomers: Transform customer data for analytics
 * 7. AggregateMetrics: Combine all transformed data (DAG convergence)
 * 8. GenerateInsights: Create actionable business intelligence
 *
 * TAS-137 Best Practices Demonstrated:
 * - Root DAG nodes: No task context or dependency access needed
 * - getDependencyResult() for upstream step results (auto-unwraps)
 * - DAG convergence pattern with multiple dependencies
 * - ErrorType.PERMANENT_ERROR for non-recoverable failures
 */

import { StepHandler } from '../../../../../../src/handler/base.js';
import { ErrorType } from '../../../../../../src/types/error-type.js';
import type { StepContext } from '../../../../../../src/types/step-context.js';
import type { StepHandlerResult } from '../../../../../../src/types/step-handler-result.js';

// =============================================================================
// Types
// =============================================================================

interface SalesRecord {
  order_id: string;
  date: string;
  product_id: string;
  quantity: number;
  amount: number;
}

interface InventoryRecord {
  product_id: string;
  sku: string;
  warehouse: string;
  quantity_on_hand: number;
  reorder_point: number;
}

interface CustomerRecord {
  customer_id: string;
  name: string;
  tier: string;
  lifetime_value: number;
  join_date: string;
}

interface DailySales {
  total_amount: number;
  order_count: number;
  avg_order_value: number;
}

interface ProductSales {
  total_quantity: number;
  total_revenue: number;
  order_count: number;
}

interface WarehouseSummary {
  total_quantity: number;
  product_count: number;
  reorder_alerts: number;
}

interface ProductInventory {
  total_quantity: number;
  warehouse_count: number;
  needs_reorder: boolean;
}

interface TierAnalysis {
  customer_count: number;
  total_lifetime_value: number;
  avg_lifetime_value: number;
}

interface ValueSegments {
  high_value: number;
  medium_value: number;
  low_value: number;
}

interface Insight {
  category: string;
  finding: string;
  metric: number;
  recommendation: string;
}

interface HealthScore {
  score: number;
  max_score: number;
  rating: string;
}

// =============================================================================
// Sample Data (Simulated External Systems)
// =============================================================================

const SAMPLE_SALES_DATA: SalesRecord[] = [
  { order_id: 'ORD-001', date: '2025-11-01', product_id: 'PROD-A', quantity: 5, amount: 499.95 },
  { order_id: 'ORD-002', date: '2025-11-05', product_id: 'PROD-B', quantity: 3, amount: 299.97 },
  { order_id: 'ORD-003', date: '2025-11-10', product_id: 'PROD-A', quantity: 2, amount: 199.98 },
  { order_id: 'ORD-004', date: '2025-11-15', product_id: 'PROD-C', quantity: 10, amount: 1499.9 },
  { order_id: 'ORD-005', date: '2025-11-18', product_id: 'PROD-B', quantity: 7, amount: 699.93 },
];

const SAMPLE_INVENTORY_DATA: InventoryRecord[] = [
  {
    product_id: 'PROD-A',
    sku: 'SKU-A-001',
    warehouse: 'WH-01',
    quantity_on_hand: 150,
    reorder_point: 50,
  },
  {
    product_id: 'PROD-B',
    sku: 'SKU-B-002',
    warehouse: 'WH-01',
    quantity_on_hand: 75,
    reorder_point: 25,
  },
  {
    product_id: 'PROD-C',
    sku: 'SKU-C-003',
    warehouse: 'WH-02',
    quantity_on_hand: 200,
    reorder_point: 100,
  },
  {
    product_id: 'PROD-A',
    sku: 'SKU-A-001',
    warehouse: 'WH-02',
    quantity_on_hand: 100,
    reorder_point: 50,
  },
  {
    product_id: 'PROD-B',
    sku: 'SKU-B-002',
    warehouse: 'WH-03',
    quantity_on_hand: 50,
    reorder_point: 25,
  },
];

const SAMPLE_CUSTOMER_DATA: CustomerRecord[] = [
  {
    customer_id: 'CUST-001',
    name: 'Alice Johnson',
    tier: 'gold',
    lifetime_value: 5000.0,
    join_date: '2024-01-15',
  },
  {
    customer_id: 'CUST-002',
    name: 'Bob Smith',
    tier: 'silver',
    lifetime_value: 2500.0,
    join_date: '2024-03-20',
  },
  {
    customer_id: 'CUST-003',
    name: 'Carol White',
    tier: 'premium',
    lifetime_value: 15000.0,
    join_date: '2023-11-10',
  },
  {
    customer_id: 'CUST-004',
    name: 'David Brown',
    tier: 'standard',
    lifetime_value: 500.0,
    join_date: '2025-01-05',
  },
  {
    customer_id: 'CUST-005',
    name: 'Eve Davis',
    tier: 'gold',
    lifetime_value: 7500.0,
    join_date: '2024-06-12',
  },
];

// =============================================================================
// Extract Handlers (Parallel - No Dependencies)
// =============================================================================

/**
 * Extract sales data from source system (simulated).
 *
 * This handler runs in parallel with other extract handlers.
 * No dependencies - root node in the DAG.
 *
 * Output:
 *   records: Raw sales records
 *   extracted_at: Extraction timestamp
 *   total_amount: Sum of all sales amounts
 *   date_range: Start and end dates of data
 */
export class ExtractSalesDataHandler extends StepHandler {
  static handlerName = 'DataPipeline.StepHandlers.ExtractSalesDataHandler';
  static handlerVersion = '1.0.0';

  async call(_context: StepContext): Promise<StepHandlerResult> {
    const dates = SAMPLE_SALES_DATA.map((r) => r.date);
    const totalAmount = SAMPLE_SALES_DATA.reduce((sum, r) => sum + r.amount, 0);

    const salesData = {
      records: SAMPLE_SALES_DATA,
      extracted_at: new Date().toISOString(),
      source: 'SalesDatabase',
      total_amount: totalAmount,
      date_range: {
        start_date: dates.reduce((a, b) => (a < b ? a : b)),
        end_date: dates.reduce((a, b) => (a > b ? a : b)),
      },
    };

    return this.success(salesData, {
      operation: 'extract_sales',
      source: 'SalesDatabase',
      records_extracted: SAMPLE_SALES_DATA.length,
      extraction_time: new Date().toISOString(),
    });
  }
}

/**
 * Extract inventory data from warehouse system (simulated).
 *
 * This handler runs in parallel with other extract handlers.
 * No dependencies - root node in the DAG.
 *
 * Output:
 *   records: Raw inventory records
 *   extracted_at: Extraction timestamp
 *   total_quantity: Sum of all quantities on hand
 *   warehouses: Unique warehouse IDs
 */
export class ExtractInventoryDataHandler extends StepHandler {
  static handlerName = 'DataPipeline.StepHandlers.ExtractInventoryDataHandler';
  static handlerVersion = '1.0.0';

  async call(_context: StepContext): Promise<StepHandlerResult> {
    const warehouses = [...new Set(SAMPLE_INVENTORY_DATA.map((r) => r.warehouse))];
    const totalQuantity = SAMPLE_INVENTORY_DATA.reduce((sum, r) => sum + r.quantity_on_hand, 0);
    const productsTracked = new Set(SAMPLE_INVENTORY_DATA.map((r) => r.product_id)).size;

    const inventoryData = {
      records: SAMPLE_INVENTORY_DATA,
      extracted_at: new Date().toISOString(),
      source: 'InventorySystem',
      total_quantity: totalQuantity,
      warehouses,
      products_tracked: productsTracked,
    };

    return this.success(inventoryData, {
      operation: 'extract_inventory',
      source: 'InventorySystem',
      records_extracted: SAMPLE_INVENTORY_DATA.length,
      warehouses,
      extraction_time: new Date().toISOString(),
    });
  }
}

/**
 * Extract customer data from CRM system (simulated).
 *
 * This handler runs in parallel with other extract handlers.
 * No dependencies - root node in the DAG.
 *
 * Output:
 *   records: Raw customer records
 *   extracted_at: Extraction timestamp
 *   total_customers: Total customer count
 *   tier_breakdown: Count per tier
 */
export class ExtractCustomerDataHandler extends StepHandler {
  static handlerName = 'DataPipeline.StepHandlers.ExtractCustomerDataHandler';
  static handlerVersion = '1.0.0';

  async call(_context: StepContext): Promise<StepHandlerResult> {
    const tierBreakdown: Record<string, number> = {};
    for (const customer of SAMPLE_CUSTOMER_DATA) {
      tierBreakdown[customer.tier] = (tierBreakdown[customer.tier] || 0) + 1;
    }

    const totalLtv = SAMPLE_CUSTOMER_DATA.reduce((sum, r) => sum + r.lifetime_value, 0);

    const customerData = {
      records: SAMPLE_CUSTOMER_DATA,
      extracted_at: new Date().toISOString(),
      source: 'CRMSystem',
      total_customers: SAMPLE_CUSTOMER_DATA.length,
      total_lifetime_value: totalLtv,
      tier_breakdown: tierBreakdown,
      avg_lifetime_value: totalLtv / SAMPLE_CUSTOMER_DATA.length,
    };

    return this.success(customerData, {
      operation: 'extract_customers',
      source: 'CRMSystem',
      records_extracted: SAMPLE_CUSTOMER_DATA.length,
      customer_tiers: Object.keys(tierBreakdown),
      extraction_time: new Date().toISOString(),
    });
  }
}

// =============================================================================
// Transform Handlers (Sequential - Depend on Extracts)
// =============================================================================

/**
 * Transform sales data for analytics.
 *
 * Dependencies: extract_sales_data
 *
 * Input (from dependency):
 *   extract_sales_data.records: Raw sales records
 *
 * Output:
 *   record_count: Number of records processed
 *   daily_sales: Sales aggregated by date
 *   product_sales: Sales aggregated by product
 *   total_revenue: Total revenue
 */
export class TransformSalesHandler extends StepHandler {
  static handlerName = 'DataPipeline.StepHandlers.TransformSalesHandler';
  static handlerVersion = '1.0.0';

  async call(context: StepContext): Promise<StepHandlerResult> {
    // TAS-137: Use getDependencyResult() for upstream step results
    const extractResults = context.getDependencyResult('extract_sales_data') as Record<
      string,
      unknown
    > | null;

    if (!extractResults) {
      return this.failure('Sales extraction results not found', ErrorType.PERMANENT_ERROR, false);
    }

    const rawRecords = (extractResults.records || []) as SalesRecord[];

    // Group by date for daily sales
    const dailyGroups = new Map<string, SalesRecord[]>();
    for (const record of rawRecords) {
      const existing = dailyGroups.get(record.date) || [];
      existing.push(record);
      dailyGroups.set(record.date, existing);
    }

    const dailySales: Record<string, DailySales> = {};
    for (const [date, dayRecords] of dailyGroups) {
      const total = dayRecords.reduce((sum, r) => sum + r.amount, 0);
      dailySales[date] = {
        total_amount: total,
        order_count: dayRecords.length,
        avg_order_value: total / dayRecords.length,
      };
    }

    // Group by product for product sales
    const productGroups = new Map<string, SalesRecord[]>();
    for (const record of rawRecords) {
      const existing = productGroups.get(record.product_id) || [];
      existing.push(record);
      productGroups.set(record.product_id, existing);
    }

    const productSales: Record<string, ProductSales> = {};
    for (const [productId, productRecords] of productGroups) {
      productSales[productId] = {
        total_quantity: productRecords.reduce((sum, r) => sum + r.quantity, 0),
        total_revenue: productRecords.reduce((sum, r) => sum + r.amount, 0),
        order_count: productRecords.length,
      };
    }

    const totalRevenue = rawRecords.reduce((sum, r) => sum + r.amount, 0);

    return this.success(
      {
        record_count: rawRecords.length,
        daily_sales: dailySales,
        product_sales: productSales,
        total_revenue: totalRevenue,
        transformation_type: 'sales_analytics',
        source: 'extract_sales_data',
      },
      {
        operation: 'transform_sales',
        source_step: 'extract_sales_data',
        transformation_applied: true,
        record_count: rawRecords.length,
        transformed_at: new Date().toISOString(),
      }
    );
  }
}

/**
 * Transform inventory data for analytics.
 *
 * Dependencies: extract_inventory_data
 *
 * Input (from dependency):
 *   extract_inventory_data.records: Raw inventory records
 *
 * Output:
 *   record_count: Number of records processed
 *   warehouse_summary: Inventory by warehouse
 *   product_inventory: Inventory by product
 *   reorder_alerts: Products needing reorder
 */
export class TransformInventoryHandler extends StepHandler {
  static handlerName = 'DataPipeline.StepHandlers.TransformInventoryHandler';
  static handlerVersion = '1.0.0';

  async call(context: StepContext): Promise<StepHandlerResult> {
    // TAS-137: Use getDependencyResult() for upstream step results
    const extractResults = context.getDependencyResult('extract_inventory_data') as Record<
      string,
      unknown
    > | null;

    if (!extractResults) {
      return this.failure(
        'Inventory extraction results not found',
        ErrorType.PERMANENT_ERROR,
        false
      );
    }

    const rawRecords = (extractResults.records || []) as InventoryRecord[];

    // Group by warehouse
    const warehouseGroups = new Map<string, InventoryRecord[]>();
    for (const record of rawRecords) {
      const existing = warehouseGroups.get(record.warehouse) || [];
      existing.push(record);
      warehouseGroups.set(record.warehouse, existing);
    }

    const warehouseSummary: Record<string, WarehouseSummary> = {};
    for (const [warehouse, whRecords] of warehouseGroups) {
      const reorderCount = whRecords.filter(
        (r) => r.quantity_on_hand <= r.reorder_point
      ).length;
      warehouseSummary[warehouse] = {
        total_quantity: whRecords.reduce((sum, r) => sum + r.quantity_on_hand, 0),
        product_count: new Set(whRecords.map((r) => r.product_id)).size,
        reorder_alerts: reorderCount,
      };
    }

    // Group by product
    const productGroups = new Map<string, InventoryRecord[]>();
    for (const record of rawRecords) {
      const existing = productGroups.get(record.product_id) || [];
      existing.push(record);
      productGroups.set(record.product_id, existing);
    }

    const productInventory: Record<string, ProductInventory> = {};
    for (const [productId, productRecords] of productGroups) {
      const totalQty = productRecords.reduce((sum, r) => sum + r.quantity_on_hand, 0);
      const totalReorder = productRecords.reduce((sum, r) => sum + r.reorder_point, 0);
      productInventory[productId] = {
        total_quantity: totalQty,
        warehouse_count: new Set(productRecords.map((r) => r.warehouse)).size,
        needs_reorder: totalQty < totalReorder,
      };
    }

    const totalOnHand = rawRecords.reduce((sum, r) => sum + r.quantity_on_hand, 0);
    const reorderAlerts = Object.values(productInventory).filter((data) => data.needs_reorder)
      .length;

    return this.success(
      {
        record_count: rawRecords.length,
        warehouse_summary: warehouseSummary,
        product_inventory: productInventory,
        total_quantity_on_hand: totalOnHand,
        reorder_alerts: reorderAlerts,
        transformation_type: 'inventory_analytics',
        source: 'extract_inventory_data',
      },
      {
        operation: 'transform_inventory',
        source_step: 'extract_inventory_data',
        transformation_applied: true,
        record_count: rawRecords.length,
        transformed_at: new Date().toISOString(),
      }
    );
  }
}

/**
 * Transform customer data for analytics.
 *
 * Dependencies: extract_customer_data
 *
 * Input (from dependency):
 *   extract_customer_data.records: Raw customer records
 *
 * Output:
 *   record_count: Number of records processed
 *   tier_analysis: Analysis by customer tier
 *   value_segments: Customer value segmentation
 *   total_lifetime_value: Total LTV across all customers
 */
export class TransformCustomersHandler extends StepHandler {
  static handlerName = 'DataPipeline.StepHandlers.TransformCustomersHandler';
  static handlerVersion = '1.0.0';

  async call(context: StepContext): Promise<StepHandlerResult> {
    // TAS-137: Use getDependencyResult() for upstream step results
    const extractResults = context.getDependencyResult('extract_customer_data') as Record<
      string,
      unknown
    > | null;

    if (!extractResults) {
      return this.failure(
        'Customer extraction results not found',
        ErrorType.PERMANENT_ERROR,
        false
      );
    }

    const rawRecords = (extractResults.records || []) as CustomerRecord[];

    // Group by tier
    const tierGroups = new Map<string, CustomerRecord[]>();
    for (const record of rawRecords) {
      const existing = tierGroups.get(record.tier) || [];
      existing.push(record);
      tierGroups.set(record.tier, existing);
    }

    const tierAnalysis: Record<string, TierAnalysis> = {};
    for (const [tier, tierRecords] of tierGroups) {
      const totalLtv = tierRecords.reduce((sum, r) => sum + r.lifetime_value, 0);
      tierAnalysis[tier] = {
        customer_count: tierRecords.length,
        total_lifetime_value: totalLtv,
        avg_lifetime_value: totalLtv / tierRecords.length,
      };
    }

    // Value segmentation
    const valueSegments: ValueSegments = {
      high_value: rawRecords.filter((r) => r.lifetime_value >= 10000).length,
      medium_value: rawRecords.filter(
        (r) => r.lifetime_value >= 1000 && r.lifetime_value < 10000
      ).length,
      low_value: rawRecords.filter((r) => r.lifetime_value < 1000).length,
    };

    const totalLtv = rawRecords.reduce((sum, r) => sum + r.lifetime_value, 0);

    return this.success(
      {
        record_count: rawRecords.length,
        tier_analysis: tierAnalysis,
        value_segments: valueSegments,
        total_lifetime_value: totalLtv,
        avg_customer_value: rawRecords.length > 0 ? totalLtv / rawRecords.length : 0,
        transformation_type: 'customer_analytics',
        source: 'extract_customer_data',
      },
      {
        operation: 'transform_customers',
        source_step: 'extract_customer_data',
        transformation_applied: true,
        record_count: rawRecords.length,
        transformed_at: new Date().toISOString(),
      }
    );
  }
}

// =============================================================================
// Aggregate Handler (DAG Convergence)
// =============================================================================

/**
 * Aggregate metrics from all transformed data sources.
 *
 * This handler demonstrates DAG convergence - it depends on all 3 transform
 * steps and combines their results.
 *
 * Dependencies:
 *   - transform_sales
 *   - transform_inventory
 *   - transform_customers
 *
 * Output:
 *   total_revenue: Total revenue from sales
 *   total_inventory_quantity: Total inventory on hand
 *   total_customers: Total customer count
 *   revenue_per_customer: Average revenue per customer
 *   inventory_turnover_indicator: Revenue / inventory ratio
 */
export class AggregateMetricsHandler extends StepHandler {
  static handlerName = 'DataPipeline.StepHandlers.AggregateMetricsHandler';
  static handlerVersion = '1.0.0';

  async call(context: StepContext): Promise<StepHandlerResult> {
    // TAS-137: Get results from all 3 transform steps (DAG convergence)
    const salesData = context.getDependencyResult('transform_sales') as Record<
      string,
      unknown
    > | null;
    const inventoryData = context.getDependencyResult('transform_inventory') as Record<
      string,
      unknown
    > | null;
    const customerData = context.getDependencyResult('transform_customers') as Record<
      string,
      unknown
    > | null;

    // Validate all sources present
    const missing: string[] = [];
    if (!salesData) missing.push('transform_sales');
    if (!inventoryData) missing.push('transform_inventory');
    if (!customerData) missing.push('transform_customers');

    if (missing.length > 0) {
      return this.failure(
        `Missing transform results: ${missing.join(', ')}`,
        ErrorType.PERMANENT_ERROR,
        false
      );
    }

    // Extract metrics from each source
    const totalRevenue = (salesData!.total_revenue as number) || 0;
    const salesRecordCount = (salesData!.record_count as number) || 0;

    const totalInventory = (inventoryData!.total_quantity_on_hand as number) || 0;
    const reorderAlerts = (inventoryData!.reorder_alerts as number) || 0;

    const totalCustomers = (customerData!.record_count as number) || 0;
    const totalLtv = (customerData!.total_lifetime_value as number) || 0;

    // Calculate cross-source metrics
    const revenuePerCustomer = totalCustomers > 0 ? totalRevenue / totalCustomers : 0;
    const inventoryTurnover = totalInventory > 0 ? totalRevenue / totalInventory : 0;

    return this.success(
      {
        total_revenue: totalRevenue,
        total_inventory_quantity: totalInventory,
        total_customers: totalCustomers,
        total_customer_lifetime_value: totalLtv,
        sales_transactions: salesRecordCount,
        inventory_reorder_alerts: reorderAlerts,
        revenue_per_customer: Math.round(revenuePerCustomer * 100) / 100,
        inventory_turnover_indicator: Math.round(inventoryTurnover * 10000) / 10000,
        aggregation_complete: true,
        sources_included: 3,
      },
      {
        operation: 'aggregate_metrics',
        sources: ['transform_sales', 'transform_inventory', 'transform_customers'],
        sources_aggregated: 3,
        aggregated_at: new Date().toISOString(),
      }
    );
  }
}

// =============================================================================
// Generate Insights Handler (Final Step)
// =============================================================================

/**
 * Generate business insights from aggregated metrics.
 *
 * This handler is the final step in the DAG workflow.
 *
 * Dependencies:
 *   - aggregate_metrics
 *
 * Output:
 *   insights: Business insights with recommendations
 *   health_score: Overall business health score
 *   total_metrics_analyzed: Number of metrics analyzed
 *   pipeline_complete: Pipeline completion flag
 */
export class GenerateInsightsHandler extends StepHandler {
  static handlerName = 'DataPipeline.StepHandlers.GenerateInsightsHandler';
  static handlerVersion = '1.0.0';

  async call(context: StepContext): Promise<StepHandlerResult> {
    // TAS-137: Get aggregated metrics from prior step
    const metrics = context.getDependencyResult('aggregate_metrics') as Record<
      string,
      unknown
    > | null;

    if (!metrics) {
      return this.failure('Aggregated metrics not found', ErrorType.PERMANENT_ERROR, false);
    }

    const insights: Insight[] = [];

    // Revenue insights
    const revenue = (metrics.total_revenue as number) || 0;
    const customers = (metrics.total_customers as number) || 0;
    const revenuePerCustomer = (metrics.revenue_per_customer as number) || 0;

    if (revenue > 0) {
      insights.push({
        category: 'Revenue',
        finding: `Total revenue of $${revenue} with ${customers} customers`,
        metric: revenuePerCustomer,
        recommendation:
          revenuePerCustomer < 500 ? 'Consider upselling strategies' : 'Customer spend is healthy',
      });
    }

    // Inventory insights
    const inventoryAlerts = (metrics.inventory_reorder_alerts as number) || 0;
    if (inventoryAlerts > 0) {
      insights.push({
        category: 'Inventory',
        finding: `${inventoryAlerts} products need reordering`,
        metric: inventoryAlerts,
        recommendation: 'Review reorder points and place purchase orders',
      });
    } else {
      insights.push({
        category: 'Inventory',
        finding: 'All products above reorder points',
        metric: 0,
        recommendation: 'Inventory levels are healthy',
      });
    }

    // Customer insights
    const totalLtv = (metrics.total_customer_lifetime_value as number) || 0;
    const avgLtv = customers > 0 ? totalLtv / customers : 0;

    insights.push({
      category: 'Customer Value',
      finding: `Average customer lifetime value: $${avgLtv.toFixed(2)}`,
      metric: avgLtv,
      recommendation: avgLtv > 3000 ? 'Focus on retention programs' : 'Increase customer engagement',
    });

    // Business health score
    const healthScore = this.calculateHealthScore(revenuePerCustomer, inventoryAlerts, avgLtv);

    return this.success(
      {
        insights,
        health_score: healthScore,
        total_metrics_analyzed: Object.keys(metrics).length,
        pipeline_complete: true,
        generated_at: new Date().toISOString(),
      },
      {
        operation: 'generate_insights',
        source_step: 'aggregate_metrics',
        insights_generated: insights.length,
        generated_at: new Date().toISOString(),
      }
    );
  }

  private calculateHealthScore(
    revenuePerCustomer: number,
    inventoryAlerts: number,
    avgLtv: number
  ): HealthScore {
    let score = 0;
    if (revenuePerCustomer > 500) score += 40; // Revenue health
    if (inventoryAlerts === 0) score += 30; // Inventory health
    if (avgLtv > 3000) score += 30; // Customer health

    let rating: string;
    if (score >= 80) {
      rating = 'Excellent';
    } else if (score >= 60) {
      rating = 'Good';
    } else if (score >= 40) {
      rating = 'Fair';
    } else {
      rating = 'Needs Improvement';
    }

    return {
      score,
      max_score: 100,
      rating,
    };
  }
}
