#!/usr/bin/env ruby

require 'pg'
require 'yaml'

puts "=== Direct Database Debug ===" 

# Direct database connection
conn = PG.connect(ENV['DATABASE_URL'] || 'postgresql://tasker:tasker@localhost/tasker_rust_test')

# Find the most recent order fulfillment task
result = conn.exec("
  SELECT t.task_id, t.created_at
  FROM tasker_tasks t
  JOIN tasker_named_tasks nt ON t.named_task_id = nt.named_task_id  
  JOIN tasker_task_namespaces tn ON nt.task_namespace_id = tn.task_namespace_id
  WHERE tn.name = 'fulfillment' AND nt.name = 'process_order'
  ORDER BY t.created_at DESC 
  LIMIT 1
")

if result.ntuples == 0
  puts "No order fulfillment tasks found in database"
  exit 1
end

task_id = result[0]['task_id']
puts "Most recent task: #{task_id} (created: #{result[0]['created_at']})"

# Check workflow steps for this task
puts "\n1. Workflow steps for task #{task_id}:"
steps_result = conn.exec_params("
  SELECT 
    ws.workflow_step_id,
    ns.name,
    ws.attempts,
    ws.retry_limit,
    ws.retryable,
    ws.processed,
    ws.in_process,
    COALESCE(current_state.to_state, 'pending') as current_state
  FROM tasker_workflow_steps ws
  JOIN tasker_named_steps ns ON ns.named_step_id = ws.named_step_id
  LEFT JOIN tasker_workflow_step_transitions current_state
    ON current_state.workflow_step_id = ws.workflow_step_id
    AND current_state.most_recent = true
  WHERE ws.task_id = $1
  ORDER BY ws.workflow_step_id
", [task_id])

steps_result.each do |row|
  puts "   - #{row['name']}: state=#{row['current_state']}, attempts=#{row['attempts']}, retry_limit=#{row['retry_limit']}, retryable=#{row['retryable']}, processed=#{row['processed']}, in_process=#{row['in_process']}"
end

# Check step dependencies
puts "\n2. Step dependencies:"
deps_result = conn.exec_params("
  SELECT 
    from_ns.name as from_step,
    to_ns.name as to_step
  FROM tasker_workflow_step_edges edge
  JOIN tasker_workflow_steps from_ws ON edge.from_step_id = from_ws.workflow_step_id
  JOIN tasker_workflow_steps to_ws ON edge.to_step_id = to_ws.workflow_step_id
  JOIN tasker_named_steps from_ns ON from_ws.named_step_id = from_ns.named_step_id
  JOIN tasker_named_steps to_ns ON to_ws.named_step_id = to_ns.named_step_id
  WHERE from_ws.task_id = $1 OR to_ws.task_id = $1
  ORDER BY from_ns.name, to_ns.name
", [task_id])

if deps_result.ntuples == 0
  puts "   No dependencies found!"
else
  deps_result.each do |row|
    puts "   #{row['from_step']} -> #{row['to_step']}"
  end
end

# Use the actual get_step_readiness_status SQL function  
puts "\n3. Step readiness status (using get_step_readiness_status function):"
readiness_result = conn.exec_params("
  SELECT 
    name,
    current_state,
    dependencies_satisfied,
    retry_eligible,
    ready_for_execution,
    attempts,
    retry_limit,
    total_parents,
    completed_parents
  FROM get_step_readiness_status($1)
", [task_id])

readiness_result.each do |row|
  puts "   - #{row['name']}:"
  puts "     ready_for_execution: #{row['ready_for_execution']}"
  puts "     current_state: #{row['current_state']}"
  puts "     dependencies_satisfied: #{row['dependencies_satisfied']}"  
  puts "     retry_eligible: #{row['retry_eligible']}"
  puts "     attempts: #{row['attempts']}, retry_limit: #{row['retry_limit']}"
  puts "     dependencies: #{row['completed_parents']}/#{row['total_parents']} completed"
end

# Check task execution context using the SQL function
puts "\n4. Task execution context:"
context_result = conn.exec_params("
  SELECT 
    task_id,
    task_state,
    ready_steps,
    has_ready_steps,
    total_steps,
    completed_steps,
    in_progress_steps,
    failed_steps
  FROM get_task_execution_context($1) 
", [task_id])

if context_result.ntuples > 0
  ctx = context_result[0]
  puts "   Task #{ctx['task_id']}:"
  puts "   - task_state: #{ctx['task_state']}"
  puts "   - ready_steps: #{ctx['ready_steps']}"
  puts "   - has_ready_steps: #{ctx['has_ready_steps']}"
  puts "   - total_steps: #{ctx['total_steps']}"
  puts "   - completed_steps: #{ctx['completed_steps']}"
  puts "   - in_progress_steps: #{ctx['in_progress_steps']}"
  puts "   - failed_steps: #{ctx['failed_steps']}"
else
  puts "   No task execution context found"
end

conn.close

puts "\n=== Debug Complete ==="