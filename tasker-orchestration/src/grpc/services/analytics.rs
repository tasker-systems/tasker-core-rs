//! Analytics service gRPC implementation.
//!
//! Provides performance metrics and bottleneck analysis operations via gRPC.

use crate::grpc::interceptors::AuthInterceptor;
use crate::grpc::state::GrpcState;
use tasker_shared::proto::v1::{
    self as proto, analytics_service_server::AnalyticsService as AnalyticsServiceTrait,
};
use tasker_shared::types::Permission;
use tasker_shared::types::SecurityContext;
use tonic::{Request, Response, Status};
use tracing::{debug, info};

/// gRPC Analytics service implementation.
#[derive(Debug)]
pub struct AnalyticsServiceImpl {
    state: GrpcState,
    auth_interceptor: AuthInterceptor,
}

impl AnalyticsServiceImpl {
    /// Create a new analytics service.
    pub fn new(state: GrpcState) -> Self {
        let auth_interceptor = AuthInterceptor::new(state.services.security_service.clone());
        Self {
            state,
            auth_interceptor,
        }
    }

    /// Authenticate the request and check permissions.
    async fn authenticate_and_authorize<T>(
        &self,
        request: &Request<T>,
        required_permission: Permission,
    ) -> Result<SecurityContext, Status> {
        let ctx = self.auth_interceptor.authenticate(request).await?;

        // Check permission
        if !ctx.has_permission(&required_permission) {
            return Err(Status::permission_denied(
                "Insufficient permissions for this operation",
            ));
        }

        Ok(ctx)
    }
}

#[tonic::async_trait]
impl AnalyticsServiceTrait for AnalyticsServiceImpl {
    /// Get performance metrics over a time period.
    async fn get_performance_metrics(
        &self,
        request: Request<proto::GetPerformanceMetricsRequest>,
    ) -> Result<Response<proto::GetPerformanceMetricsResponse>, Status> {
        // Authenticate and authorize
        let _ctx = self
            .authenticate_and_authorize(&request, Permission::AnalyticsRead)
            .await?;

        let req = request.into_inner();
        let hours = req.hours.unwrap_or(24) as u32;

        debug!(hours = hours, "gRPC get performance metrics");

        // Get metrics via service layer
        let result = self
            .state
            .services
            .analytics_service
            .get_performance_metrics(hours)
            .await;

        match result {
            Ok(metrics) => {
                info!(
                    hours = hours,
                    total_tasks = metrics.total_tasks,
                    "Performance metrics retrieved via gRPC"
                );

                // Convert domain PerformanceMetrics to proto response
                Ok(Response::new(proto::GetPerformanceMetricsResponse {
                    total_tasks: metrics.total_tasks,
                    active_tasks: metrics.active_tasks,
                    completed_tasks: metrics.completed_tasks,
                    failed_tasks: metrics.failed_tasks,
                    completion_rate: metrics.completion_rate,
                    error_rate: metrics.error_rate,
                    average_task_duration_seconds: metrics.average_task_duration_seconds,
                    average_step_duration_seconds: metrics.average_step_duration_seconds,
                    tasks_per_hour: metrics.tasks_per_hour,
                    steps_per_hour: metrics.steps_per_hour,
                    system_health_score: metrics.system_health_score,
                    analysis_period_start: metrics.analysis_period_start,
                    calculated_at: metrics.calculated_at,
                }))
            }
            Err(e) => Err(tasker_error_to_status(&e)),
        }
    }

    /// Get bottleneck analysis (slow steps, tasks, resource utilization).
    async fn get_bottleneck_analysis(
        &self,
        request: Request<proto::GetBottleneckAnalysisRequest>,
    ) -> Result<Response<proto::GetBottleneckAnalysisResponse>, Status> {
        // Authenticate and authorize
        let _ctx = self
            .authenticate_and_authorize(&request, Permission::AnalyticsRead)
            .await?;

        let req = request.into_inner();
        let limit = req.limit.unwrap_or(10);
        let min_executions = req.min_executions.unwrap_or(5);

        debug!(
            limit = limit,
            min_executions = min_executions,
            "gRPC get bottleneck analysis"
        );

        // Get analysis via service layer
        let result = self
            .state
            .services
            .analytics_service
            .get_bottleneck_analysis(limit, min_executions)
            .await;

        match result {
            Ok(analysis) => {
                info!(
                    limit = limit,
                    min_executions = min_executions,
                    slow_steps_count = analysis.slow_steps.len(),
                    slow_tasks_count = analysis.slow_tasks.len(),
                    "Bottleneck analysis retrieved via gRPC"
                );

                // Convert domain BottleneckAnalysis to proto response
                Ok(Response::new(proto::GetBottleneckAnalysisResponse {
                    slow_steps: analysis
                        .slow_steps
                        .into_iter()
                        .map(|step| proto::SlowStepInfo {
                            namespace_name: step.namespace_name,
                            task_name: step.task_name,
                            version: step.version,
                            step_name: step.step_name,
                            average_duration_seconds: step.average_duration_seconds,
                            max_duration_seconds: step.max_duration_seconds,
                            execution_count: step.execution_count,
                            error_count: step.error_count,
                            error_rate: step.error_rate,
                            last_executed_at: step.last_executed_at,
                        })
                        .collect(),
                    slow_tasks: analysis
                        .slow_tasks
                        .into_iter()
                        .map(|task| proto::SlowTaskInfo {
                            namespace_name: task.namespace_name,
                            task_name: task.task_name,
                            version: task.version,
                            average_duration_seconds: task.average_duration_seconds,
                            max_duration_seconds: task.max_duration_seconds,
                            execution_count: task.execution_count,
                            average_step_count: task.average_step_count,
                            error_count: task.error_count,
                            error_rate: task.error_rate,
                            last_executed_at: task.last_executed_at,
                        })
                        .collect(),
                    resource_utilization: Some(proto::ResourceUtilization {
                        database_pool_utilization: analysis
                            .resource_utilization
                            .database_pool_utilization,
                        system_health: Some(convert_system_health_counts(
                            &analysis.resource_utilization.system_health,
                        )),
                    }),
                    recommendations: analysis.recommendations,
                }))
            }
            Err(e) => Err(tasker_error_to_status(&e)),
        }
    }
}

/// Convert a TaskerError to a gRPC Status.
///
/// Maps analytics service errors (which return TaskerError) to appropriate gRPC status codes.
/// Internal errors use generic messages to avoid leaking implementation details.
fn tasker_error_to_status(error: &tasker_shared::errors::TaskerError) -> Status {
    use tasker_shared::errors::TaskerError;
    match error {
        // Client-facing errors - safe to expose details
        TaskerError::ValidationError(_)
        | TaskerError::InvalidInput(_)
        | TaskerError::InvalidParameter(_) => Status::invalid_argument(error.to_string()),
        TaskerError::CircuitBreakerOpen(_) => {
            Status::unavailable("Service temporarily unavailable")
        }
        TaskerError::Timeout(_) => Status::deadline_exceeded("Request timed out"),
        // Internal errors - use generic messages, details are logged server-side
        TaskerError::DatabaseError(_)
        | TaskerError::Internal(_)
        | TaskerError::StateTransitionError(_)
        | TaskerError::OrchestrationError(_)
        | TaskerError::EventError(_)
        | TaskerError::ConfigurationError(_)
        | TaskerError::InvalidConfiguration(_)
        | TaskerError::FFIError(_)
        | TaskerError::MessagingError(_)
        | TaskerError::CacheError(_)
        | TaskerError::WorkerError(_)
        | TaskerError::InvalidState(_)
        | TaskerError::TaskInitializationError { .. }
        | TaskerError::StateMachineError(_)
        | TaskerError::StateMachineActionError(_)
        | TaskerError::StateMachineGuardError(_)
        | TaskerError::StateMachinePersistenceError(_) => {
            tracing::error!(error = %error, "Analytics service error");
            Status::internal("Internal error processing analytics request")
        }
    }
}

/// Convert domain SystemHealthCounts to proto SystemHealthCounts.
///
/// Maps all 24 fields directly from the domain model:
/// - 12 task state counts + total_tasks
/// - 10 step state counts + total_steps
fn convert_system_health_counts(
    health: &tasker_shared::database::sql_functions::SystemHealthCounts,
) -> proto::SystemHealthCounts {
    proto::SystemHealthCounts {
        // Task counts by state (12 states + total)
        pending_tasks: health.pending_tasks,
        initializing_tasks: health.initializing_tasks,
        enqueuing_steps_tasks: health.enqueuing_steps_tasks,
        steps_in_process_tasks: health.steps_in_process_tasks,
        evaluating_results_tasks: health.evaluating_results_tasks,
        waiting_for_dependencies_tasks: health.waiting_for_dependencies_tasks,
        waiting_for_retry_tasks: health.waiting_for_retry_tasks,
        blocked_by_failures_tasks: health.blocked_by_failures_tasks,
        complete_tasks: health.complete_tasks,
        error_tasks: health.error_tasks,
        cancelled_tasks: health.cancelled_tasks,
        resolved_manually_tasks: health.resolved_manually_tasks,
        total_tasks: health.total_tasks,

        // Step counts by state (10 states + total)
        pending_steps: health.pending_steps,
        enqueued_steps: health.enqueued_steps,
        in_progress_steps: health.in_progress_steps,
        enqueued_for_orchestration_steps: health.enqueued_for_orchestration_steps,
        enqueued_as_error_for_orchestration_steps: health.enqueued_as_error_for_orchestration_steps,
        waiting_for_retry_steps: health.waiting_for_retry_steps,
        complete_steps: health.complete_steps,
        error_steps: health.error_steps,
        cancelled_steps: health.cancelled_steps,
        resolved_manually_steps: health.resolved_manually_steps,
        total_steps: health.total_steps,
    }
}
