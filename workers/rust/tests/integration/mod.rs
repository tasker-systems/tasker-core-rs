// Integration Tests Module - Native Rust Worker Implementation
//
// Comprehensive integration testing suite for all workflow patterns using native Rust step handlers.
// This module organizes and provides access to all integration tests that mirror the Ruby integration
// test patterns but use the native Rust implementation throughout the entire execution pipeline.
//
// Test Coverage:
// - Linear Workflow: Sequential mathematical operations (input^8)
// - Diamond Workflow: Parallel execution with convergence (input^16)  
// - Tree Workflow: Complex hierarchical structure (input^32)
// - Mixed DAG Workflow: Most complex pattern with mixed dependencies (input^64)
// - Order Fulfillment: Business workflow with external service simulation
//
// Each test suite includes:
// - Complete workflow execution tests
// - Error handling and validation tests
// - Framework integration tests  
// - Step handler registration tests
// - Performance and concurrency tests
// - Dependency resolution tests

// Test helper modules
pub mod test_helpers;

// Integration test suites for each workflow pattern
pub mod linear_workflow_integration;
pub mod diamond_workflow_integration;
pub mod tree_workflow_integration;
pub mod mixed_dag_workflow_integration;
pub mod order_fulfillment_integration;

// Performance benchmarking (will be implemented next)
pub mod performance_benchmarks;