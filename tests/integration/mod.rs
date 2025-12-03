// Integration tests for framework lifecycle (TAS-42)
mod batch_processing_workflow;
mod conditional_approval_workflow;
mod diamond_decision_batch_workflow;
mod diamond_workflow;
mod dlq_lifecycle_test;
mod domain_event_workflow; // TAS-65: Domain event publishing workflow
mod linear_workflow;
mod manual_step_completion_test;
mod metrics;
mod mixed_dag_workflow;
mod order_fulfillment;
mod sql_functions;
mod tree_workflow;
