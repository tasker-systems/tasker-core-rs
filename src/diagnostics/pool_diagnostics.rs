//! # Database Pool Diagnostics
//!
//! Comprehensive diagnostic system for tracking database pool usage patterns,
//! identifying connection leaks, and monitoring pool health.

use sqlx::PgPool;
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};
use tokio::sync::Mutex;
use tracing::{debug, error, info, warn};

/// Pool diagnostic metrics
#[derive(Debug, Clone)]
pub struct PoolDiagnostics {
    pub pool_size: u32,
    pub num_idle: usize,
    pub num_active: usize,
    pub acquire_count: usize,
    pub release_count: usize,
    pub connection_age_seconds: Vec<u64>,
    pub active_connections_duration: Vec<Duration>,
    pub last_acquire_time: Option<Instant>,
    pub last_release_time: Option<Instant>,
    pub pool_utilization_percentage: f64,
    pub connection_churn_rate: f64,
}

/// Pool usage tracker for identifying connection patterns
#[derive(Debug)]
pub struct PoolUsageTracker {
    pool: PgPool,
    acquire_counter: Arc<AtomicUsize>,
    release_counter: Arc<AtomicUsize>,
    active_connections: Arc<Mutex<HashMap<String, Instant>>>,
    connection_history: Arc<Mutex<Vec<ConnectionEvent>>>,
}

/// Connection usage event for tracking
#[derive(Debug, Clone)]
pub struct ConnectionEvent {
    pub event_type: ConnectionEventType,
    pub caller: String,
    pub timestamp: Instant,
    pub pool_size: u32,
    pub active_connections: usize,
    pub idle_connections: usize,
}

/// Types of connection events
#[derive(Debug, Clone)]
pub enum ConnectionEventType {
    Acquire,
    Release,
    Timeout,
    PoolCreated,
    PoolClosed,
}

impl PoolUsageTracker {
    /// Create a new pool usage tracker
    pub fn new(pool: PgPool) -> Self {
        let tracker = Self {
            pool,
            acquire_counter: Arc::new(AtomicUsize::new(0)),
            release_counter: Arc::new(AtomicUsize::new(0)),
            active_connections: Arc::new(Mutex::new(HashMap::new())),
            connection_history: Arc::new(Mutex::new(Vec::new())),
        };

        // Log initial pool creation
        tracker.log_event(ConnectionEventType::PoolCreated, "pool_creation".to_string());

        tracker
    }

    /// Track connection acquisition
    pub async fn track_acquire(&self, caller: &str) -> Result<(), String> {
        let caller_key = format!("{}#{}", caller, self.acquire_counter.fetch_add(1, Ordering::SeqCst));
        
        info!(
            "🔍 POOL ACQUIRE: {} requesting connection (pool: size={}, idle={}, active={})",
            caller_key,
            self.pool.size(),
            self.pool.num_idle(),
            self.pool.num_active()
        );

        // Record acquisition
        let mut active = self.active_connections.lock().await;
        active.insert(caller_key.clone(), Instant::now());

        self.log_event(ConnectionEventType::Acquire, caller_key);

        Ok(())
    }

    /// Track connection release
    pub async fn track_release(&self, caller: &str) -> Result<(), String> {
        self.release_counter.fetch_add(1, Ordering::SeqCst);

        let mut active = self.active_connections.lock().await;
        let caller_key = active.keys()
            .find(|k| k.starts_with(&format!("{}#", caller)))
            .cloned();

        if let Some(key) = caller_key {
            let start_time = active.remove(&key).unwrap_or(Instant::now());
            let duration = start_time.elapsed();

            info!(
                "🔍 POOL RELEASE: {} released connection after {:?} (pool: size={}, idle={}, active={})",
                key,
                duration,
                self.pool.size(),
                self.pool.num_idle(),
                self.pool.num_active()
            );

            self.log_event(ConnectionEventType::Release, key);
        } else {
            warn!(
                "🔍 POOL RELEASE: {} released connection but no matching acquire found",
                caller
            );
        }

        Ok(())
    }

    /// Track connection timeout
    pub async fn track_timeout(&self, caller: &str) -> Result<(), String> {
        error!(
            "🔍 POOL TIMEOUT: {} timed out acquiring connection (pool: size={}, idle={}, active={})",
            caller,
            self.pool.size(),
            self.pool.num_idle(),
            self.pool.num_active()
        );

        self.log_event(ConnectionEventType::Timeout, caller.to_string());

        Ok(())
    }

    /// Get current pool diagnostics
    pub async fn get_diagnostics(&self) -> PoolDiagnostics {
        let active = self.active_connections.lock().await;
        let connection_history = self.connection_history.lock().await;

        let pool_size = self.pool.size();
        let num_idle = self.pool.num_idle();
        let num_active = self.pool.num_active();

        // Calculate connection ages for active connections
        let now = Instant::now();
        let active_connections_duration: Vec<Duration> = active
            .values()
            .map(|start_time| now.duration_since(*start_time))
            .collect();

        // Calculate utilization percentage
        let pool_utilization_percentage = if pool_size > 0 {
            (num_active as f64 / pool_size as f64) * 100.0
        } else {
            0.0
        };

        // Calculate churn rate (acquires per minute)
        let acquire_count = self.acquire_counter.load(Ordering::SeqCst);
        let release_count = self.release_counter.load(Ordering::SeqCst);

        // Get last acquire and release times
        let last_acquire_time = connection_history
            .iter()
            .rev()
            .find(|e| matches!(e.event_type, ConnectionEventType::Acquire))
            .map(|e| e.timestamp);

        let last_release_time = connection_history
            .iter()
            .rev()
            .find(|e| matches!(e.event_type, ConnectionEventType::Release))
            .map(|e| e.timestamp);

        PoolDiagnostics {
            pool_size,
            num_idle,
            num_active,
            acquire_count,
            release_count,
            connection_age_seconds: active_connections_duration
                .iter()
                .map(|d| d.as_secs())
                .collect(),
            active_connections_duration,
            last_acquire_time,
            last_release_time,
            pool_utilization_percentage,
            connection_churn_rate: acquire_count as f64 / 60.0, // Rough approximation
        }
    }

    /// Check for potential connection leaks
    pub async fn check_for_leaks(&self) -> Vec<String> {
        let mut leaks = Vec::new();
        let active = self.active_connections.lock().await;
        let now = Instant::now();

        for (caller, start_time) in active.iter() {
            let duration = now.duration_since(*start_time);
            if duration > Duration::from_secs(300) { // 5 minutes
                leaks.push(format!(
                    "Potential leak: {} held connection for {:?}",
                    caller, duration
                ));
            }
        }

        leaks
    }

    /// Get pool usage summary
    pub async fn get_usage_summary(&self) -> String {
        let diagnostics = self.get_diagnostics().await;
        let leaks = self.check_for_leaks().await;

        format!(
            "📊 POOL USAGE SUMMARY\n\
            Pool Size: {}\n\
            Active Connections: {} ({:.1}% utilization)\n\
            Idle Connections: {}\n\
            Total Acquires: {}\n\
            Total Releases: {}\n\
            Pending Acquires: {}\n\
            Longest Active Connection: {:?}\n\
            Potential Leaks: {}\n\
            Leak Details: {:?}",
            diagnostics.pool_size,
            diagnostics.num_active,
            diagnostics.pool_utilization_percentage,
            diagnostics.num_idle,
            diagnostics.acquire_count,
            diagnostics.release_count,
            diagnostics.acquire_count - diagnostics.release_count,
            diagnostics.active_connections_duration
                .iter()
                .max()
                .unwrap_or(&Duration::from_secs(0)),
            leaks.len(),
            leaks
        )
    }

    /// Log connection event
    fn log_event(&self, event_type: ConnectionEventType, caller: String) {
        let event = ConnectionEvent {
            event_type,
            caller,
            timestamp: Instant::now(),
            pool_size: self.pool.size(),
            active_connections: self.pool.num_active(),
            idle_connections: self.pool.num_idle(),
        };

        // Log to history (async operation, so we spawn it)
        let history = self.connection_history.clone();
        tokio::spawn(async move {
            let mut history_guard = history.lock().await;
            history_guard.push(event);
            
            // Keep only last 1000 events to prevent memory leak
            if history_guard.len() > 1000 {
                history_guard.drain(0..500);
            }
        });
    }
}

/// Macro for tracking pool operations
#[macro_export]
macro_rules! track_pool_operation {
    ($tracker:expr, $operation:expr, $caller:expr) => {
        if let Some(tracker) = $tracker {
            match $operation {
                "acquire" => tracker.track_acquire($caller).await.unwrap_or_else(|e| {
                    eprintln!("Failed to track acquire: {}", e);
                }),
                "release" => tracker.track_release($caller).await.unwrap_or_else(|e| {
                    eprintln!("Failed to track release: {}", e);
                }),
                "timeout" => tracker.track_timeout($caller).await.unwrap_or_else(|e| {
                    eprintln!("Failed to track timeout: {}", e);
                }),
                _ => {}
            }
        }
    };
}

/// Global pool diagnostics instance
use std::sync::OnceLock;
static GLOBAL_POOL_DIAGNOSTICS: OnceLock<Arc<PoolUsageTracker>> = OnceLock::new();

/// Initialize global pool diagnostics
pub fn initialize_pool_diagnostics(pool: PgPool) -> Arc<PoolUsageTracker> {
    let tracker = Arc::new(PoolUsageTracker::new(pool));
    GLOBAL_POOL_DIAGNOSTICS.set(tracker.clone()).unwrap_or_else(|_| {
        warn!("Pool diagnostics already initialized");
    });
    tracker
}

/// Get global pool diagnostics
pub fn get_pool_diagnostics() -> Option<Arc<PoolUsageTracker>> {
    GLOBAL_POOL_DIAGNOSTICS.get().cloned()
}

/// Print comprehensive pool diagnostics
pub async fn print_pool_diagnostics() {
    if let Some(tracker) = get_pool_diagnostics() {
        let summary = tracker.get_usage_summary().await;
        println!("{}", summary);
    } else {
        println!("📊 POOL DIAGNOSTICS: Not initialized");
    }
}