#!/usr/bin/env ruby

require_relative 'lib/tasker_core'
require 'json'
require 'pg'

# Connect to database directly
conn = PG.connect(
  dbname: 'tasker_rust_test',
  user: 'tasker',
  password: 'tasker',
  host: 'localhost'
)

# Find a recent task with order fulfillment steps
result = conn.exec_params("
  SELECT t.task_id, t.complete, t.created_at,
    (SELECT tt.to_state FROM tasker_task_transitions tt 
     WHERE tt.task_id = t.task_id 
     ORDER BY tt.sort_key DESC 
     LIMIT 1) as current_state
  FROM tasker_tasks t
  WHERE t.task_id IN (
    SELECT DISTINCT task_id 
    FROM tasker_workflow_steps ws
    JOIN tasker_named_steps ns ON ns.named_step_id = ws.named_step_id
    WHERE ns.name = 'validate_order'
  )
  ORDER BY t.created_at DESC
  LIMIT 1
")

if result.ntuples == 0
  puts "No OrderFulfillmentTaskHandler tasks found"
  exit 1
end

task = result[0]
task_id = task['task_id']
puts "Found task: #{task_id} - State: #{task['current_state']} - Complete: #{task['complete']} - Created: #{task['created_at']}"

# Get all workflow steps for this task
steps_result = conn.exec_params("
  SELECT 
    ws.workflow_step_id,
    ws.named_step_id,
    ns.name as step_name,
    (SELECT wst.to_state FROM tasker_workflow_step_transitions wst 
     WHERE wst.workflow_step_id = ws.workflow_step_id 
     ORDER BY wst.sort_key DESC 
     LIMIT 1) as current_state,
    ws.processed,
    ws.in_process,
    ws.attempts,
    ws.results,
    ws.created_at,
    ws.processed_at
  FROM tasker_workflow_steps ws
  JOIN tasker_named_steps ns ON ns.named_step_id = ws.named_step_id
  WHERE ws.task_id = $1
  ORDER BY ws.workflow_step_id
", [task_id])

puts "\nWorkflow steps:"
steps_result.each do |step|
  puts "\n  Step: #{step['step_name']} (ID: #{step['workflow_step_id']})"
  puts "    State: #{step['current_state']}"
  puts "    Processed: #{step['processed']}"
  puts "    In Process: #{step['in_process']}"
  puts "    Attempts: #{step['attempts']}"
  puts "    Results: #{step['results'] ? JSON.parse(step['results']) : 'null'}"
  puts "    Created: #{step['created_at']}"
  puts "    Processed At: #{step['processed_at'] || 'never'}"
end

# Check dependencies (using correct table: tasker_workflow_step_edges)
puts "\n\nDependencies:"
deps_result = conn.exec_params("
  SELECT 
    ns1.name as step_name,
    ns2.name as depends_on,
    e.name as edge_type
  FROM tasker_workflow_step_edges e
  JOIN tasker_workflow_steps ws1 ON ws1.workflow_step_id = e.to_step_id
  JOIN tasker_workflow_steps ws2 ON ws2.workflow_step_id = e.from_step_id
  JOIN tasker_named_steps ns1 ON ns1.named_step_id = ws1.named_step_id
  JOIN tasker_named_steps ns2 ON ns2.named_step_id = ws2.named_step_id
  WHERE ws1.task_id = $1
  ORDER BY e.to_step_id
", [task_id])

if deps_result.ntuples == 0
  puts "  No dependencies found in tasker_workflow_step_edges table!"
else
  deps_result.each do |dep|
    puts "  #{dep['step_name']} depends on #{dep['depends_on']} (#{dep['edge_type']})"
  end
end

# Check step readiness using the same SQL function as orchestration system
puts "\n\nStep Readiness Status (using get_step_readiness_status):"
readiness_result = conn.exec_params("
  SELECT 
    workflow_step_id,
    name,
    current_state,
    dependencies_satisfied,
    retry_eligible,
    ready_for_execution,
    total_parents,
    completed_parents,
    attempts
  FROM get_step_readiness_status($1)
  ORDER BY workflow_step_id
", [task_id])

readiness_result.each do |step|
  puts "\n  Step: #{step['name']} (ID: #{step['workflow_step_id']})"
  puts "    Current State: #{step['current_state']}"
  puts "    Dependencies Satisfied: #{step['dependencies_satisfied']}"
  puts "    Ready for Execution: #{step['ready_for_execution']}"
  puts "    Total Parents: #{step['total_parents']}"
  puts "    Completed Parents: #{step['completed_parents']}"
  puts "    Attempts: #{step['attempts']}"
end

conn.close