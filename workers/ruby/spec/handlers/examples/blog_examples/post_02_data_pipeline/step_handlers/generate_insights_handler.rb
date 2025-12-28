# frozen_string_literal: true

module DataPipeline
  module StepHandlers
    # Generate business insights from aggregated metrics
    #
    # This handler demonstrates:
    # - Final step in DAG workflow
    # - Business intelligence generation
    # - Actionable insights from aggregated data
    class GenerateInsightsHandler < TaskerCore::StepHandler::Base
      def call(context)
        logger.info "💡 GenerateInsightsHandler: Generating business insights - task_uuid=#{context.task_uuid}"

        # Get aggregated metrics from prior step
        metrics = context.get_dependency_result('aggregate_metrics')
        unless metrics
          raise TaskerCore::Errors::PermanentError.new(
            'Aggregated metrics not found',
            error_code: 'MISSING_AGGREGATE_RESULTS'
          )
        end

        # Generate insights
        insights = generate_business_insights(metrics)

        logger.info "✅ GenerateInsightsHandler: Generated #{insights[:insights].count} business insights"
        insights[:insights].each_with_index do |insight, idx|
          logger.info "   #{idx + 1}. #{insight[:category]}: #{insight[:finding]}"
        end

        TaskerCore::Types::StepHandlerCallResult.success(
          result: insights,
          metadata: {
            operation: 'generate_insights',
            source_step: 'aggregate_metrics',
            insights_generated: insights[:insights].count,
            generated_at: Time.now.utc.iso8601,
            handler_class: self.class.name
          }
        )
      rescue StandardError => e
        logger.error "❌ GenerateInsightsHandler: Insight generation failed - #{e.class.name}: #{e.message}"
        raise
      end

      private

      def generate_business_insights(metrics)
        insights = []

        # Revenue insights
        revenue = metrics['total_revenue'] || metrics[:total_revenue] || 0
        customers = metrics['total_customers'] || metrics[:total_customers] || 0
        revenue_per_customer = metrics['revenue_per_customer'] || metrics[:revenue_per_customer] || 0

        if revenue.positive?
          insights << {
            category: 'Revenue',
            finding: "Total revenue of $#{revenue} with #{customers} customers",
            metric: revenue_per_customer,
            recommendation: revenue_per_customer < 500 ? 'Consider upselling strategies' : 'Customer spend is healthy'
          }
        end

        # Inventory insights
        inventory_alerts = metrics['inventory_reorder_alerts'] || metrics[:inventory_reorder_alerts] || 0
        insights << if inventory_alerts.positive?
                      {
                        category: 'Inventory',
                        finding: "#{inventory_alerts} products need reordering",
                        metric: inventory_alerts,
                        recommendation: 'Review reorder points and place purchase orders'
                      }
                    else
                      {
                        category: 'Inventory',
                        finding: 'All products above reorder points',
                        metric: 0,
                        recommendation: 'Inventory levels are healthy'
                      }
                    end

        # Customer insights
        total_ltv = metrics['total_customer_lifetime_value'] || metrics[:total_customer_lifetime_value] || 0
        avg_ltv = customers.positive? ? total_ltv / customers.to_f : 0

        insights << {
          category: 'Customer Value',
          finding: "Average customer lifetime value: $#{avg_ltv.round(2)}",
          metric: avg_ltv,
          recommendation: avg_ltv > 3000 ? 'Focus on retention programs' : 'Increase customer engagement'
        }

        # Business health score
        health_score = calculate_health_score(revenue_per_customer, inventory_alerts, avg_ltv)

        {
          insights: insights,
          health_score: health_score,
          total_metrics_analyzed: metrics.keys.count,
          pipeline_complete: true,
          generated_at: Time.now.utc.iso8601
        }
      end

      def calculate_health_score(revenue_per_customer, inventory_alerts, avg_ltv)
        score = 0
        score += 40 if revenue_per_customer > 500 # Revenue health
        score += 30 if inventory_alerts.zero? # Inventory health
        score += 30 if avg_ltv > 3000 # Customer health

        {
          score: score,
          max_score: 100,
          rating: rating_from_score(score)
        }
      end

      def rating_from_score(score)
        case score
        when 80..100 then 'Excellent'
        when 60..79 then 'Good'
        when 40..59 then 'Fair'
        else 'Needs Improvement'
        end
      end
    end
  end
end
