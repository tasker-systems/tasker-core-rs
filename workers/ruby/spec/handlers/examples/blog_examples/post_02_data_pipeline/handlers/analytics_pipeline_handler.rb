# frozen_string_literal: true

module DataPipeline
  module Handlers
    # TaskHandler for Blog Post 02: Data Pipeline Resilience
    #
    # Narrative: 3 AM ETL alerts → reliable analytics pipeline
    #
    # This handler coordinates an 8-step DAG workflow demonstrating:
    # - Parallel data extraction from multiple sources (sales, inventory, customer)
    # - Sequential transformation of each data source
    # - Aggregation across all transformed data
    # - Final insights generation
    #
    # DAG Structure:
    #   Extract (Parallel):    sales → transform_sales ─┐
    #                          inventory → transform_inventory ─→ aggregate → insights
    #                          customer → transform_customers ─┘
    #
    # Blog Post: Post 02 - Data Pipeline Resilience
    # Pattern: DAG workflow with parallel execution and aggregation
    class AnalyticsPipelineHandler < TaskerCore::TaskHandler::Base
      def initialize(context = {})
        super
        @logger = context[:logger] || Logger.new($stdout)
        @timeout_seconds = context[:timeout_seconds] || 120
      end

      def call(_context)
        @logger.info "📊 AnalyticsPipelineHandler: Starting analytics pipeline for task #{task.task_uuid}"
        @logger.info '   Pipeline: Extract → Transform → Aggregate → Insights'
        @logger.info '   Total steps: 8 (3 parallel extracts, 3 transforms, 1 aggregate, 1 insights)'

        # Task handler doesn't process data - just coordinates workflow
        # All business logic is in step handlers

        TaskerCore::Types::StepHandlerCallResult.success(
          result: {
            pipeline_started: true,
            task_uuid: task.task_uuid,
            total_steps: 8,
            parallel_extract_steps: 3
          },
          metadata: {
            handler_class: self.class.name,
            pipeline_type: 'analytics_etl',
            started_at: Time.now.utc.iso8601
          }
        )
      rescue StandardError => e
        @logger.error "❌ AnalyticsPipelineHandler: Pipeline initialization failed - #{e.class.name}: #{e.message}"
        raise
      end
    end
  end
end
