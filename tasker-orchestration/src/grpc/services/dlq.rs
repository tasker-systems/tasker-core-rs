//! DLQ service gRPC implementation.
//!
//! Provides DLQ investigation tracking operations via gRPC.

use crate::grpc::conversions::{datetime_to_timestamp, json_to_struct, parse_uuid};
use crate::grpc::interceptors::AuthInterceptor;
use crate::grpc::state::GrpcState;
use tasker_shared::models::orchestration::dlq::{
    DlqEntry, DlqInvestigationQueueEntry, DlqInvestigationUpdate, DlqListParams, DlqStats,
    StalenessMonitoring,
};
use tasker_shared::proto::v1::{self as proto, dlq_service_server::DlqService as DlqServiceTrait};
use tasker_shared::types::Permission;
use tonic::{Request, Response, Status};
use tracing::{debug, error, info};

/// gRPC DLQ service implementation.
#[derive(Debug)]
pub struct DlqServiceImpl {
    state: GrpcState,
    auth_interceptor: AuthInterceptor,
}

impl DlqServiceImpl {
    /// Create a new DLQ service.
    pub fn new(state: GrpcState) -> Self {
        let auth_interceptor = AuthInterceptor::new(state.security_service.clone());
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
    ) -> Result<tasker_shared::types::SecurityContext, Status> {
        let ctx = self.auth_interceptor.authenticate(request).await?;

        // Check permission
        if !ctx.has_permission(&required_permission) {
            return Err(Status::permission_denied(format!(
                "Permission denied: requires {:?}",
                required_permission
            )));
        }

        Ok(ctx)
    }
}

#[tonic::async_trait]
impl DlqServiceTrait for DlqServiceImpl {
    /// List DLQ entries with optional filtering.
    async fn list_entries(
        &self,
        request: Request<proto::ListDlqEntriesRequest>,
    ) -> Result<Response<proto::ListDlqEntriesResponse>, Status> {
        // Authenticate and authorize
        let _ctx = self
            .authenticate_and_authorize(&request, Permission::DlqRead)
            .await?;

        let req = request.into_inner();
        debug!(
            resolution_status = ?req.resolution_status,
            limit = req.limit,
            offset = req.offset,
            "gRPC list DLQ entries"
        );

        // Build query parameters
        let resolution_status = req
            .resolution_status
            .and_then(|s| proto::DlqResolutionStatus::try_from(s).ok())
            .and_then(dlq_resolution_status_from_proto);

        let params = DlqListParams {
            resolution_status,
            limit: req.limit.unwrap_or(50) as i64,
            offset: req.offset.unwrap_or(0) as i64,
        };

        // List DLQ entries via model layer
        let entries = DlqEntry::list(&self.state.read_pool, params)
            .await
            .map_err(|e| {
                error!("Failed to list DLQ entries: {}", e);
                Status::internal(format!("Failed to list DLQ entries: {}", e))
            })?;

        info!(count = entries.len(), "Successfully listed DLQ entries");

        // Convert to proto
        let proto_entries = entries.iter().map(dlq_entry_to_proto).collect();

        Ok(Response::new(proto::ListDlqEntriesResponse {
            entries: proto_entries,
        }))
    }

    /// Get DLQ entry for a specific task (most recent).
    async fn get_entry_by_task(
        &self,
        request: Request<proto::GetDlqEntryByTaskRequest>,
    ) -> Result<Response<proto::GetDlqEntryByTaskResponse>, Status> {
        // Authenticate and authorize
        let _ctx = self
            .authenticate_and_authorize(&request, Permission::DlqRead)
            .await?;

        let req = request.into_inner();
        let task_uuid = parse_uuid(&req.task_uuid)?;

        debug!(task_uuid = %task_uuid, "gRPC get DLQ entry by task");

        // Get DLQ entry via model layer
        let entry = DlqEntry::find_by_task(&self.state.read_pool, task_uuid)
            .await
            .map_err(|e| {
                error!("Failed to get DLQ entry for task {}: {}", task_uuid, e);
                Status::internal(format!("Failed to get DLQ entry: {}", e))
            })?;

        match entry {
            Some(entry) => {
                info!(
                    dlq_entry_uuid = %entry.dlq_entry_uuid,
                    task_uuid = %task_uuid,
                    "Successfully retrieved DLQ entry"
                );
                Ok(Response::new(proto::GetDlqEntryByTaskResponse {
                    entry: Some(dlq_entry_to_proto(&entry)),
                }))
            }
            None => Err(Status::not_found(format!(
                "DLQ entry not found for task {}",
                task_uuid
            ))),
        }
    }

    /// Update DLQ investigation status and notes.
    async fn update_investigation(
        &self,
        request: Request<proto::UpdateDlqInvestigationRequest>,
    ) -> Result<Response<proto::UpdateDlqInvestigationResponse>, Status> {
        // Authenticate and authorize
        let _ctx = self
            .authenticate_and_authorize(&request, Permission::DlqUpdate)
            .await?;

        let req = request.into_inner();
        let dlq_entry_uuid = parse_uuid(&req.dlq_entry_uuid)?;

        debug!(
            dlq_entry_uuid = %dlq_entry_uuid,
            resolution_status = ?req.resolution_status,
            "gRPC update DLQ investigation"
        );

        // Build update request
        let resolution_status = req
            .resolution_status
            .and_then(|s| proto::DlqResolutionStatus::try_from(s).ok())
            .and_then(dlq_resolution_status_from_proto);

        let metadata = req
            .metadata
            .map(crate::grpc::conversions::struct_to_json);

        let update = DlqInvestigationUpdate {
            resolution_status,
            resolution_notes: req.resolution_notes,
            resolved_by: req.resolved_by,
            metadata,
        };

        // Update via model layer
        let updated = DlqEntry::update_investigation(
            &self.state.write_pool,
            dlq_entry_uuid,
            update,
        )
        .await
        .map_err(|e| {
            error!(
                "Failed to update DLQ investigation {}: {}",
                dlq_entry_uuid, e
            );
            Status::internal(format!("Failed to update DLQ investigation: {}", e))
        })?;

        if !updated {
            return Err(Status::not_found(format!(
                "DLQ entry not found: {}",
                dlq_entry_uuid
            )));
        }

        info!(
            dlq_entry_uuid = %dlq_entry_uuid,
            "Successfully updated DLQ investigation"
        );

        Ok(Response::new(proto::UpdateDlqInvestigationResponse {
            success: true,
            message: "Investigation status updated successfully".to_string(),
            dlq_entry_uuid: dlq_entry_uuid.to_string(),
        }))
    }

    /// Get DLQ statistics by reason.
    async fn get_stats(
        &self,
        request: Request<proto::GetDlqStatsRequest>,
    ) -> Result<Response<proto::GetDlqStatsResponse>, Status> {
        // Authenticate and authorize
        let _ctx = self
            .authenticate_and_authorize(&request, Permission::DlqStats)
            .await?;

        debug!("gRPC get DLQ stats");

        // Get stats via model layer
        let stats = DlqEntry::get_stats(&self.state.read_pool)
            .await
            .map_err(|e| {
                error!("Failed to get DLQ stats: {}", e);
                Status::internal(format!("Failed to get DLQ stats: {}", e))
            })?;

        info!(
            stats_count = stats.len(),
            "Successfully retrieved DLQ stats"
        );

        // Convert to proto
        let proto_stats = stats.iter().map(dlq_stats_to_proto).collect();

        Ok(Response::new(proto::GetDlqStatsResponse {
            stats: proto_stats,
        }))
    }

    /// Get prioritized investigation queue for operator triage.
    async fn get_investigation_queue(
        &self,
        request: Request<proto::GetDlqInvestigationQueueRequest>,
    ) -> Result<Response<proto::GetDlqInvestigationQueueResponse>, Status> {
        // Authenticate and authorize
        let _ctx = self
            .authenticate_and_authorize(&request, Permission::DlqRead)
            .await?;

        let req = request.into_inner();
        let limit = req.limit.map(|l| l as i64);

        debug!(limit = ?limit, "gRPC get investigation queue");

        // Get investigation queue via model layer
        let queue = DlqEntry::list_investigation_queue(&self.state.read_pool, limit)
            .await
            .map_err(|e| {
                error!("Failed to get investigation queue: {}", e);
                Status::internal(format!("Failed to get investigation queue: {}", e))
            })?;

        info!(
            queue_size = queue.len(),
            "Successfully retrieved investigation queue"
        );

        // Convert to proto
        let proto_entries = queue
            .iter()
            .map(dlq_investigation_queue_entry_to_proto)
            .collect();

        Ok(Response::new(proto::GetDlqInvestigationQueueResponse {
            entries: proto_entries,
        }))
    }

    /// Get task staleness monitoring (proactive health check).
    async fn get_staleness_monitoring(
        &self,
        request: Request<proto::GetStalenessMonitoringRequest>,
    ) -> Result<Response<proto::GetStalenessMonitoringResponse>, Status> {
        // Authenticate and authorize
        let _ctx = self
            .authenticate_and_authorize(&request, Permission::DlqRead)
            .await?;

        let req = request.into_inner();
        let limit = req.limit.map(|l| l as i64);

        debug!(limit = ?limit, "gRPC get staleness monitoring");

        // Get staleness monitoring via model layer
        let monitoring =
            DlqEntry::get_staleness_monitoring(&self.state.read_pool, limit)
                .await
                .map_err(|e| {
                    error!("Failed to get staleness monitoring: {}", e);
                    Status::internal(format!("Failed to get staleness monitoring: {}", e))
                })?;

        info!(
            monitoring_count = monitoring.len(),
            "Successfully retrieved staleness monitoring"
        );

        // Convert to proto
        let proto_entries = monitoring
            .iter()
            .map(staleness_monitoring_entry_to_proto)
            .collect();

        Ok(Response::new(proto::GetStalenessMonitoringResponse {
            entries: proto_entries,
        }))
    }
}

// ============================================================================
// Conversion Helpers
// ============================================================================

/// Convert domain DlqEntry to proto DlqEntry.
fn dlq_entry_to_proto(entry: &DlqEntry) -> proto::DlqEntry {
    proto::DlqEntry {
        dlq_entry_uuid: entry.dlq_entry_uuid.to_string(),
        task_uuid: entry.task_uuid.to_string(),
        original_state: entry.original_state.clone(),
        dlq_reason: dlq_reason_to_proto(entry.dlq_reason) as i32,
        dlq_timestamp: Some(datetime_to_timestamp(entry.dlq_timestamp.and_utc())),
        resolution_status: dlq_resolution_status_to_proto(entry.resolution_status) as i32,
        resolution_timestamp: entry
            .resolution_timestamp
            .map(|dt| datetime_to_timestamp(dt.and_utc())),
        resolution_notes: entry.resolution_notes.clone(),
        resolved_by: entry.resolved_by.clone(),
        task_snapshot: json_to_struct(entry.task_snapshot.clone()),
        metadata: entry.metadata.clone().and_then(json_to_struct),
        created_at: Some(datetime_to_timestamp(entry.created_at.and_utc())),
        updated_at: Some(datetime_to_timestamp(entry.updated_at.and_utc())),
    }
}

/// Convert domain DlqResolutionStatus to proto DlqResolutionStatus.
fn dlq_resolution_status_to_proto(
    status: tasker_shared::models::orchestration::dlq::DlqResolutionStatus,
) -> proto::DlqResolutionStatus {
    use tasker_shared::models::orchestration::dlq::DlqResolutionStatus as DomainStatus;
    match status {
        DomainStatus::Pending => proto::DlqResolutionStatus::Pending,
        DomainStatus::ManuallyResolved => proto::DlqResolutionStatus::ManuallyResolved,
        DomainStatus::PermanentlyFailed => proto::DlqResolutionStatus::PermanentlyFailed,
        DomainStatus::Cancelled => proto::DlqResolutionStatus::Cancelled,
    }
}

/// Convert proto DlqResolutionStatus to domain DlqResolutionStatus.
fn dlq_resolution_status_from_proto(
    status: proto::DlqResolutionStatus,
) -> Option<tasker_shared::models::orchestration::dlq::DlqResolutionStatus> {
    use tasker_shared::models::orchestration::dlq::DlqResolutionStatus as DomainStatus;
    match status {
        proto::DlqResolutionStatus::Unspecified => None,
        proto::DlqResolutionStatus::Pending => Some(DomainStatus::Pending),
        proto::DlqResolutionStatus::ManuallyResolved => Some(DomainStatus::ManuallyResolved),
        proto::DlqResolutionStatus::PermanentlyFailed => Some(DomainStatus::PermanentlyFailed),
        proto::DlqResolutionStatus::Cancelled => Some(DomainStatus::Cancelled),
    }
}

/// Convert domain DlqReason to proto DlqReason.
fn dlq_reason_to_proto(
    reason: tasker_shared::models::orchestration::dlq::DlqReason,
) -> proto::DlqReason {
    use tasker_shared::models::orchestration::dlq::DlqReason as DomainReason;
    match reason {
        DomainReason::StalenessTimeout => proto::DlqReason::StalenessTimeout,
        DomainReason::MaxRetriesExceeded => proto::DlqReason::MaxRetriesExceeded,
        DomainReason::DependencyCycleDetected => proto::DlqReason::DependencyCycleDetected,
        DomainReason::WorkerUnavailable => proto::DlqReason::WorkerUnavailable,
        DomainReason::ManualDlq => proto::DlqReason::ManualDlq,
    }
}

/// Convert domain DlqStats to proto DlqStats.
fn dlq_stats_to_proto(stats: &DlqStats) -> proto::DlqStats {
    proto::DlqStats {
        dlq_reason: dlq_reason_to_proto(stats.dlq_reason) as i32,
        total_entries: stats.total_entries,
        pending: stats.pending,
        manually_resolved: stats.manually_resolved,
        permanent_failures: stats.permanent_failures,
        cancelled: stats.cancelled,
        oldest_entry: stats
            .oldest_entry
            .map(|dt| datetime_to_timestamp(dt.and_utc())),
        newest_entry: stats
            .newest_entry
            .map(|dt| datetime_to_timestamp(dt.and_utc())),
        avg_resolution_time_minutes: stats.avg_resolution_time_minutes,
    }
}

/// Convert domain DlqInvestigationQueueEntry to proto DlqInvestigationQueueEntry.
fn dlq_investigation_queue_entry_to_proto(
    entry: &DlqInvestigationQueueEntry,
) -> proto::DlqInvestigationQueueEntry {
    proto::DlqInvestigationQueueEntry {
        dlq_entry_uuid: entry.dlq_entry_uuid.to_string(),
        task_uuid: entry.task_uuid.to_string(),
        original_state: entry.original_state.clone(),
        dlq_reason: dlq_reason_to_proto(entry.dlq_reason) as i32,
        dlq_timestamp: Some(datetime_to_timestamp(entry.dlq_timestamp.and_utc())),
        minutes_in_dlq: entry.minutes_in_dlq,
        namespace_name: entry.namespace_name.clone(),
        task_name: entry.task_name.clone(),
        current_state: entry.current_state.clone(),
        time_in_state_minutes: entry.time_in_state_minutes,
        priority_score: entry.priority_score,
    }
}

/// Convert domain StalenessMonitoring to proto StalenessMonitoringEntry.
fn staleness_monitoring_entry_to_proto(
    entry: &StalenessMonitoring,
) -> proto::StalenessMonitoringEntry {
    proto::StalenessMonitoringEntry {
        task_uuid: entry.task_uuid.to_string(),
        namespace_name: entry.namespace_name.clone(),
        task_name: entry.task_name.clone(),
        current_state: entry.current_state.clone(),
        time_in_state_minutes: entry.time_in_state_minutes,
        task_age_minutes: entry.task_age_minutes,
        staleness_threshold_minutes: entry.staleness_threshold_minutes,
        health_status: staleness_health_status_to_proto(entry.health_status) as i32,
        priority: entry.priority,
    }
}

/// Convert domain StalenessHealthStatus to proto StalenessHealthStatus.
fn staleness_health_status_to_proto(
    status: tasker_shared::models::orchestration::dlq::StalenessHealthStatus,
) -> proto::StalenessHealthStatus {
    use tasker_shared::models::orchestration::dlq::StalenessHealthStatus as DomainStatus;
    match status {
        DomainStatus::Healthy => proto::StalenessHealthStatus::Healthy,
        DomainStatus::Warning => proto::StalenessHealthStatus::Warning,
        DomainStatus::Stale => proto::StalenessHealthStatus::Stale,
    }
}
