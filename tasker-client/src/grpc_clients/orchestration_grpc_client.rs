//! # Orchestration gRPC Client
//!
//! gRPC client for communicating with the tasker-orchestration gRPC API.
//! Provides methods for all available endpoints including tasks, workflow steps, templates,
//! analytics, health, DLQ, and configuration.
//!
//! This client mirrors the REST API client interface, returning the same domain types.

use tonic::transport::Channel;
use tracing::{debug, info};
use uuid::Uuid;

use tasker_shared::{
    models::core::task_request::TaskRequest,
    proto::{
        client::{
            AnalyticsServiceClient, ConfigServiceClient, DlqServiceClient, HealthServiceClient,
            StepServiceClient, TaskServiceClient, TemplateServiceClient,
        },
        v1 as proto,
    },
    types::api::{
        orchestration::{
            BottleneckAnalysis, DetailedHealthResponse, HealthResponse, MetricsQuery,
            OrchestrationConfigResponse, PerformanceMetrics, ReadinessResponse, StepAuditResponse,
            StepManualAction, StepResponse, TaskListResponse, TaskResponse,
        },
        templates::{TemplateDetail, TemplateListResponse},
    },
};

use super::common::{AuthInterceptor, GrpcAuthConfig, GrpcClientConfig};
use super::conversions;
use crate::error::ClientError;

// Re-export DLQ types used by this client
pub use tasker_shared::models::orchestration::{
    DlqEntry, DlqInvestigationQueueEntry, DlqInvestigationUpdate, DlqListParams, DlqStats,
    StalenessMonitoring,
};

/// gRPC client for communicating with the orchestration system.
///
/// This client provides methods to interact with all orchestration API endpoints
/// using gRPC as the transport protocol. It mirrors the REST client interface,
/// returning the same domain types.
///
/// # Examples
///
/// ```rust,ignore
/// use tasker_client::grpc_clients::OrchestrationGrpcClient;
/// use tasker_shared::models::core::task_request::TaskRequest;
///
/// #[tokio::main]
/// async fn main() -> Result<(), Box<dyn std::error::Error>> {
///     // Connect with default config
///     let client = OrchestrationGrpcClient::connect("http://localhost:9090").await?;
///
///     // Create a task
///     let task = client.create_task(TaskRequest {
///         name: "example".to_string(),
///         namespace: "default".to_string(),
///         version: "1.0.0".to_string(),
///         context: serde_json::json!({}),
///     }).await?;
///
///     println!("Created task: {}", task.task_uuid);
///     Ok(())
/// }
/// ```
#[derive(Debug, Clone)]
pub struct OrchestrationGrpcClient {
    task_client: TaskServiceClient<
        tonic::service::interceptor::InterceptedService<Channel, AuthInterceptor>,
    >,
    step_client: StepServiceClient<
        tonic::service::interceptor::InterceptedService<Channel, AuthInterceptor>,
    >,
    template_client: TemplateServiceClient<
        tonic::service::interceptor::InterceptedService<Channel, AuthInterceptor>,
    >,
    analytics_client: AnalyticsServiceClient<
        tonic::service::interceptor::InterceptedService<Channel, AuthInterceptor>,
    >,
    health_client: HealthServiceClient<
        tonic::service::interceptor::InterceptedService<Channel, AuthInterceptor>,
    >,
    dlq_client:
        DlqServiceClient<tonic::service::interceptor::InterceptedService<Channel, AuthInterceptor>>,
    config_client: ConfigServiceClient<
        tonic::service::interceptor::InterceptedService<Channel, AuthInterceptor>,
    >,
    endpoint: String,
}

impl OrchestrationGrpcClient {
    /// Connect to a gRPC endpoint with default configuration.
    ///
    /// # Arguments
    ///
    /// * `endpoint` - The gRPC endpoint URL (e.g., "http://localhost:9090")
    pub async fn connect(endpoint: impl Into<String>) -> Result<Self, ClientError> {
        let config = GrpcClientConfig::new(endpoint);
        Self::with_config(config).await
    }

    /// Connect to a gRPC endpoint with authentication.
    ///
    /// # Arguments
    ///
    /// * `endpoint` - The gRPC endpoint URL
    /// * `auth` - Authentication configuration
    pub async fn connect_with_auth(
        endpoint: impl Into<String>,
        auth: GrpcAuthConfig,
    ) -> Result<Self, ClientError> {
        let config = GrpcClientConfig::new(endpoint).with_auth(auth);
        Self::with_config(config).await
    }

    /// Connect with full configuration.
    pub async fn with_config(config: GrpcClientConfig) -> Result<Self, ClientError> {
        let endpoint = config.endpoint.clone();
        let channel = config.connect().await?;
        let interceptor = AuthInterceptor::new(config.auth);

        info!(endpoint = %endpoint, "Connected to orchestration gRPC endpoint");

        Ok(Self {
            task_client: TaskServiceClient::with_interceptor(channel.clone(), interceptor.clone()),
            step_client: StepServiceClient::with_interceptor(channel.clone(), interceptor.clone()),
            template_client: TemplateServiceClient::with_interceptor(
                channel.clone(),
                interceptor.clone(),
            ),
            analytics_client: AnalyticsServiceClient::with_interceptor(
                channel.clone(),
                interceptor.clone(),
            ),
            health_client: HealthServiceClient::with_interceptor(
                channel.clone(),
                interceptor.clone(),
            ),
            dlq_client: DlqServiceClient::with_interceptor(channel.clone(), interceptor.clone()),
            config_client: ConfigServiceClient::with_interceptor(channel, interceptor),
            endpoint,
        })
    }

    /// Get the configured endpoint URL.
    #[must_use]
    pub fn endpoint(&self) -> &str {
        &self.endpoint
    }

    // ===================================================================================
    // TASK API METHODS
    // ===================================================================================

    /// Create a new task.
    pub async fn create_task(&self, request: TaskRequest) -> Result<TaskResponse, ClientError> {
        debug!(
            namespace = %request.namespace,
            name = %request.name,
            "Creating task via gRPC"
        );

        let proto_request = proto::CreateTaskRequest {
            name: request.name,
            namespace: request.namespace,
            version: request.version,
            context: json_to_proto_struct(&request.context),
            initiator: None,
            source_system: None,
            reason: None,
            priority: None,
            tags: vec![],
            correlation_id: None,
            parent_correlation_id: None,
        };

        let response = self
            .task_client
            .clone()
            .create_task(proto_request)
            .await?
            .into_inner();

        conversions::proto_create_task_response_to_domain(response)
    }

    /// Get a task by UUID.
    pub async fn get_task(&self, task_uuid: Uuid) -> Result<TaskResponse, ClientError> {
        debug!(task_uuid = %task_uuid, "Getting task via gRPC");

        let response = self
            .task_client
            .clone()
            .get_task(proto::GetTaskRequest {
                task_uuid: task_uuid.to_string(),
                include_steps: false,
                include_context: true,
            })
            .await?
            .into_inner();

        conversions::proto_get_task_response_to_domain(response)
    }

    /// List tasks with pagination and filtering.
    pub async fn list_tasks(
        &self,
        limit: i32,
        offset: i32,
        namespace: Option<&str>,
        status: Option<&str>,
    ) -> Result<TaskListResponse, ClientError> {
        debug!("Listing tasks via gRPC");

        let states = status
            .and_then(parse_task_state)
            .map(|state| vec![state as i32])
            .unwrap_or_default();

        let response = self
            .task_client
            .clone()
            .list_tasks(proto::ListTasksRequest {
                pagination: Some(proto::PaginationRequest {
                    limit: Some(limit),
                    offset: Some(offset),
                }),
                namespace: namespace.map(String::from),
                name: None,
                states,
                created_at: None,
                initiator: None,
                tags: vec![],
                sort: None,
            })
            .await?
            .into_inner();

        conversions::proto_list_tasks_response_to_domain(response)
    }

    /// Cancel a task.
    pub async fn cancel_task(&self, task_uuid: Uuid) -> Result<(), ClientError> {
        debug!(task_uuid = %task_uuid, "Canceling task via gRPC");

        self.task_client
            .clone()
            .cancel_task(proto::CancelTaskRequest {
                task_uuid: task_uuid.to_string(),
                reason: None,
            })
            .await?;

        info!(task_uuid = %task_uuid, "Successfully canceled task");
        Ok(())
    }

    /// Get task context.
    pub async fn get_task_context(
        &self,
        task_uuid: Uuid,
    ) -> Result<serde_json::Value, ClientError> {
        debug!(task_uuid = %task_uuid, "Getting task context via gRPC");

        let response = self
            .task_client
            .clone()
            .get_task_context(proto::GetTaskContextRequest {
                task_uuid: task_uuid.to_string(),
            })
            .await?
            .into_inner();

        Ok(response
            .context
            .and_then(|ctx| ctx.merged)
            .map(conversions::proto_struct_to_json)
            .unwrap_or(serde_json::Value::Null))
    }

    // ===================================================================================
    // STEP API METHODS
    // ===================================================================================

    /// List workflow steps for a task.
    pub async fn list_task_steps(&self, task_uuid: Uuid) -> Result<Vec<StepResponse>, ClientError> {
        debug!(task_uuid = %task_uuid, "Listing task steps via gRPC");

        let response = self
            .step_client
            .clone()
            .list_steps(proto::ListStepsRequest {
                task_uuid: task_uuid.to_string(),
            })
            .await?
            .into_inner();

        response
            .steps
            .into_iter()
            .map(conversions::proto_step_to_domain)
            .collect()
    }

    /// Get a specific workflow step.
    pub async fn get_step(
        &self,
        task_uuid: Uuid,
        step_uuid: Uuid,
    ) -> Result<StepResponse, ClientError> {
        debug!(task_uuid = %task_uuid, step_uuid = %step_uuid, "Getting step via gRPC");

        let response = self
            .step_client
            .clone()
            .get_step(proto::GetStepRequest {
                task_uuid: task_uuid.to_string(),
                step_uuid: step_uuid.to_string(),
            })
            .await?
            .into_inner();

        conversions::proto_get_step_response_to_domain(response)
    }

    /// Manually resolve a workflow step.
    pub async fn resolve_step_manually(
        &self,
        task_uuid: Uuid,
        step_uuid: Uuid,
        action: StepManualAction,
    ) -> Result<StepResponse, ClientError> {
        debug!(
            task_uuid = %task_uuid,
            step_uuid = %step_uuid,
            "Resolving step manually via gRPC"
        );

        let (action_type, performed_by, reason, result) = match action {
            StepManualAction::ResetForRetry { reset_by, reason } => (
                proto::StepManualActionType::ResetForRetry,
                reset_by,
                reason,
                None,
            ),
            StepManualAction::ResolveManually {
                resolved_by,
                reason,
            } => (
                proto::StepManualActionType::ResolveManually,
                resolved_by,
                reason,
                None,
            ),
            StepManualAction::CompleteManually {
                completion_data,
                reason,
                completed_by,
            } => (
                proto::StepManualActionType::CompleteManually,
                completed_by,
                reason,
                json_to_proto_struct(&serde_json::to_value(&completion_data).unwrap_or_default()),
            ),
        };

        let response = self
            .step_client
            .clone()
            .resolve_step(proto::ResolveStepRequest {
                task_uuid: task_uuid.to_string(),
                step_uuid: step_uuid.to_string(),
                action_type: action_type as i32,
                performed_by,
                reason,
                result,
                metadata: None,
            })
            .await?
            .into_inner();

        let step = response.step.ok_or_else(|| {
            ClientError::Internal(format!("Step {} not found after resolution", step_uuid))
        })?;

        conversions::proto_step_to_domain(step)
    }

    /// Get audit history for a workflow step.
    pub async fn get_step_audit_history(
        &self,
        task_uuid: Uuid,
        step_uuid: Uuid,
    ) -> Result<Vec<StepAuditResponse>, ClientError> {
        debug!(
            task_uuid = %task_uuid,
            step_uuid = %step_uuid,
            "Getting step audit history via gRPC"
        );

        let response = self
            .step_client
            .clone()
            .get_step_audit(proto::GetStepAuditRequest {
                task_uuid: task_uuid.to_string(),
                step_uuid: step_uuid.to_string(),
            })
            .await?
            .into_inner();

        response
            .audit_records
            .into_iter()
            .map(conversions::proto_audit_to_domain)
            .collect()
    }

    // ===================================================================================
    // TEMPLATE API METHODS
    // ===================================================================================

    /// List all available templates.
    pub async fn list_templates(
        &self,
        namespace: Option<&str>,
    ) -> Result<TemplateListResponse, ClientError> {
        debug!(namespace = ?namespace, "Listing templates via gRPC");

        let response = self
            .template_client
            .clone()
            .list_templates(proto::ListTemplatesRequest {
                namespace: namespace.map(String::from),
            })
            .await?
            .into_inner();

        conversions::proto_template_list_to_domain(response)
    }

    /// Get details about a specific template.
    pub async fn get_template(
        &self,
        namespace: &str,
        name: &str,
        version: &str,
    ) -> Result<TemplateDetail, ClientError> {
        debug!(
            namespace = %namespace,
            name = %name,
            version = %version,
            "Getting template via gRPC"
        );

        let response = self
            .template_client
            .clone()
            .get_template(proto::GetTemplateRequest {
                namespace: namespace.to_string(),
                name: name.to_string(),
                version: version.to_string(),
            })
            .await?
            .into_inner();

        let template = response.template.ok_or_else(|| {
            ClientError::Internal(format!(
                "Template {}/{}/{} not found",
                namespace, name, version
            ))
        })?;

        conversions::proto_template_detail_to_domain(template)
    }

    // ===================================================================================
    // ANALYTICS API METHODS
    // ===================================================================================

    /// Get performance metrics.
    pub async fn get_performance_metrics(
        &self,
        query: Option<&MetricsQuery>,
    ) -> Result<PerformanceMetrics, ClientError> {
        debug!("Getting performance metrics via gRPC");

        let response = self
            .analytics_client
            .clone()
            .get_performance_metrics(proto::GetPerformanceMetricsRequest {
                hours: query.and_then(|q| q.hours.map(|h| h as i32)).or(Some(24)),
            })
            .await?
            .into_inner();

        conversions::proto_performance_metrics_to_domain(response)
    }

    /// Get bottleneck analysis.
    pub async fn get_bottlenecks(
        &self,
        limit: Option<i32>,
        min_executions: Option<i32>,
    ) -> Result<BottleneckAnalysis, ClientError> {
        debug!("Getting bottleneck analysis via gRPC");

        let response = self
            .analytics_client
            .clone()
            .get_bottleneck_analysis(proto::GetBottleneckAnalysisRequest {
                limit: limit.or(Some(10)),
                min_executions: min_executions.or(Some(5)),
            })
            .await?
            .into_inner();

        conversions::proto_bottleneck_to_domain(response)
    }

    // ===================================================================================
    // HEALTH API METHODS
    // ===================================================================================

    /// Check if the orchestration API is healthy.
    pub async fn health_check(&self) -> Result<(), ClientError> {
        self.get_basic_health().await?;
        Ok(())
    }

    /// Get basic health status.
    pub async fn get_basic_health(&self) -> Result<HealthResponse, ClientError> {
        debug!("Checking health via gRPC");

        let response = self
            .health_client
            .clone()
            .check_health(proto::HealthRequest {})
            .await?
            .into_inner();

        Ok(conversions::proto_health_response_to_domain(response))
    }

    /// Kubernetes liveness probe.
    pub async fn liveness_probe(&self) -> Result<HealthResponse, ClientError> {
        debug!("Checking liveness via gRPC");

        let response = self
            .health_client
            .clone()
            .check_liveness(proto::LivenessRequest {})
            .await?
            .into_inner();

        Ok(conversions::proto_health_response_to_domain(response))
    }

    /// Kubernetes readiness probe.
    pub async fn readiness_probe(&self) -> Result<ReadinessResponse, ClientError> {
        debug!("Checking readiness via gRPC");

        let response = self
            .health_client
            .clone()
            .check_readiness(proto::ReadinessRequest {})
            .await?
            .into_inner();

        conversions::proto_readiness_response_to_domain(response)
    }

    /// Get detailed health status.
    pub async fn get_detailed_health(&self) -> Result<DetailedHealthResponse, ClientError> {
        debug!("Getting detailed health via gRPC");

        let response = self
            .health_client
            .clone()
            .check_detailed_health(proto::DetailedHealthRequest {})
            .await?
            .into_inner();

        conversions::proto_detailed_health_response_to_domain(response)
    }

    // ===================================================================================
    // DLQ API METHODS
    // ===================================================================================

    /// List DLQ entries with optional filtering.
    pub async fn list_dlq_entries(
        &self,
        params: Option<&DlqListParams>,
    ) -> Result<Vec<DlqEntry>, ClientError> {
        debug!("Listing DLQ entries via gRPC");

        let response = self
            .dlq_client
            .clone()
            .list_entries(proto::ListDlqEntriesRequest {
                resolution_status: params
                    .and_then(|p| p.resolution_status.as_ref())
                    .map(|s| conversions::dlq_resolution_status_to_proto(s) as i32),
                limit: params.map(|p| p.limit as i32).or(Some(50)),
                offset: params.map(|p| p.offset as i32).or(Some(0)),
            })
            .await?
            .into_inner();

        response
            .entries
            .into_iter()
            .map(conversions::proto_dlq_entry_to_domain)
            .collect()
    }

    /// Get DLQ entry by task UUID.
    pub async fn get_dlq_entry(&self, task_uuid: Uuid) -> Result<DlqEntry, ClientError> {
        debug!(task_uuid = %task_uuid, "Getting DLQ entry via gRPC");

        let response = self
            .dlq_client
            .clone()
            .get_entry_by_task(proto::GetDlqEntryByTaskRequest {
                task_uuid: task_uuid.to_string(),
            })
            .await?
            .into_inner();

        let entry = response.entry.ok_or_else(|| {
            ClientError::Internal(format!("DLQ entry for task {} not found", task_uuid))
        })?;

        conversions::proto_dlq_entry_to_domain(entry)
    }

    /// Update DLQ investigation status.
    pub async fn update_dlq_investigation(
        &self,
        dlq_entry_uuid: Uuid,
        update: DlqInvestigationUpdate,
    ) -> Result<(), ClientError> {
        debug!(dlq_entry_uuid = %dlq_entry_uuid, "Updating DLQ investigation via gRPC");

        self.dlq_client
            .clone()
            .update_investigation(proto::UpdateDlqInvestigationRequest {
                dlq_entry_uuid: dlq_entry_uuid.to_string(),
                resolution_status: update
                    .resolution_status
                    .as_ref()
                    .map(|s| conversions::dlq_resolution_status_to_proto(s) as i32),
                resolution_notes: update.resolution_notes,
                resolved_by: update.resolved_by,
                metadata: update.metadata.and_then(|v| json_to_proto_struct(&v)),
            })
            .await?;

        info!(dlq_entry_uuid = %dlq_entry_uuid, "Successfully updated DLQ investigation");
        Ok(())
    }

    /// Get DLQ statistics.
    pub async fn get_dlq_stats(&self) -> Result<Vec<DlqStats>, ClientError> {
        debug!("Getting DLQ statistics via gRPC");

        let response = self
            .dlq_client
            .clone()
            .get_stats(proto::GetDlqStatsRequest {})
            .await?
            .into_inner();

        response
            .stats
            .into_iter()
            .map(conversions::proto_dlq_stats_to_domain)
            .collect()
    }

    /// Get DLQ investigation queue.
    pub async fn get_investigation_queue(
        &self,
        limit: Option<i64>,
    ) -> Result<Vec<DlqInvestigationQueueEntry>, ClientError> {
        debug!("Getting DLQ investigation queue via gRPC");

        let response = self
            .dlq_client
            .clone()
            .get_investigation_queue(proto::GetDlqInvestigationQueueRequest {
                limit: limit.map(|l| l as i32).or(Some(100)),
            })
            .await?
            .into_inner();

        response
            .entries
            .into_iter()
            .map(conversions::proto_dlq_queue_entry_to_domain)
            .collect()
    }

    /// Get staleness monitoring data.
    pub async fn get_staleness_monitoring(
        &self,
        limit: Option<i64>,
    ) -> Result<Vec<StalenessMonitoring>, ClientError> {
        debug!("Getting staleness monitoring via gRPC");

        let response = self
            .dlq_client
            .clone()
            .get_staleness_monitoring(proto::GetStalenessMonitoringRequest {
                limit: limit.map(|l| l as i32).or(Some(100)),
            })
            .await?
            .into_inner();

        response
            .entries
            .into_iter()
            .map(conversions::proto_staleness_to_domain)
            .collect()
    }

    // ===================================================================================
    // CONFIG API METHODS
    // ===================================================================================

    /// Get orchestration configuration (secrets redacted).
    pub async fn get_config(&self) -> Result<OrchestrationConfigResponse, ClientError> {
        debug!("Getting config via gRPC");

        let response = self
            .config_client
            .clone()
            .get_config(proto::GetConfigRequest {})
            .await?
            .into_inner();

        conversions::proto_config_to_domain(response)
    }
}

// ===================================================================================
// REQUEST HELPERS (Domain -> Proto)
// ===================================================================================

/// Convert JSON Value to proto Struct (for request building)
fn json_to_proto_struct(value: &serde_json::Value) -> Option<prost_types::Struct> {
    if value.is_null() {
        return None;
    }
    if let serde_json::Value::Object(map) = value {
        let fields = map
            .iter()
            .filter_map(|(k, v)| json_value_to_proto_value(v).map(|pv| (k.clone(), pv)))
            .collect();
        Some(prost_types::Struct { fields })
    } else {
        None
    }
}

fn json_value_to_proto_value(value: &serde_json::Value) -> Option<prost_types::Value> {
    use prost_types::value::Kind;

    let kind = match value {
        serde_json::Value::Null => Kind::NullValue(0),
        serde_json::Value::Bool(b) => Kind::BoolValue(*b),
        serde_json::Value::Number(n) => Kind::NumberValue(n.as_f64().unwrap_or(0.0)),
        serde_json::Value::String(s) => Kind::StringValue(s.clone()),
        serde_json::Value::Array(arr) => {
            let values: Vec<prost_types::Value> =
                arr.iter().filter_map(json_value_to_proto_value).collect();
            Kind::ListValue(prost_types::ListValue { values })
        }
        serde_json::Value::Object(map) => {
            let fields = map
                .iter()
                .filter_map(|(k, v)| json_value_to_proto_value(v).map(|pv| (k.clone(), pv)))
                .collect();
            Kind::StructValue(prost_types::Struct { fields })
        }
    };

    Some(prost_types::Value { kind: Some(kind) })
}

/// Parse task state string to proto enum (for request filtering)
fn parse_task_state(s: &str) -> Option<proto::TaskState> {
    match s.to_lowercase().as_str() {
        "pending" => Some(proto::TaskState::Pending),
        "initializing" => Some(proto::TaskState::Initializing),
        "enqueuingsteps" | "enqueuing_steps" => Some(proto::TaskState::EnqueuingSteps),
        "stepsinprocess" | "steps_in_process" => Some(proto::TaskState::StepsInProcess),
        "evaluatingresults" | "evaluating_results" => Some(proto::TaskState::EvaluatingResults),
        "waitingfordependencies" | "waiting_for_dependencies" => {
            Some(proto::TaskState::WaitingForDependencies)
        }
        "waitingforretry" | "waiting_for_retry" => Some(proto::TaskState::WaitingForRetry),
        "blockedbyfailures" | "blocked_by_failures" => Some(proto::TaskState::BlockedByFailures),
        "complete" => Some(proto::TaskState::Complete),
        "error" => Some(proto::TaskState::Error),
        "cancelled" => Some(proto::TaskState::Cancelled),
        "resolvedmanually" | "resolved_manually" => Some(proto::TaskState::ResolvedManually),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_grpc_client_config_default() {
        let config = GrpcClientConfig::default();
        assert_eq!(config.endpoint, "http://localhost:9090");
    }

    #[test]
    fn test_json_to_proto_struct_roundtrip() {
        // Note: protobuf represents all numbers as f64, so we use floats in the test
        let json = serde_json::json!({
            "key": "value",
            "number": 42.0,
            "nested": {
                "bool": true
            }
        });

        let proto_struct = json_to_proto_struct(&json).unwrap();
        let roundtrip = conversions::proto_struct_to_json(proto_struct);

        assert_eq!(json, roundtrip);
    }

    #[test]
    fn test_parse_task_state() {
        assert_eq!(parse_task_state("pending"), Some(proto::TaskState::Pending));
        assert_eq!(
            parse_task_state("complete"),
            Some(proto::TaskState::Complete)
        );
        assert_eq!(parse_task_state("PENDING"), Some(proto::TaskState::Pending));
        assert_eq!(parse_task_state("invalid"), None);
    }
}
