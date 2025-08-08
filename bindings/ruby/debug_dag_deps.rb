require_relative 'lib/tasker_core'

puts '🔍 Testing DAG relationship view with order fulfillment...'

# Set up database connection
TaskerCore::Database::Connection.instance

# Start embedded orchestration to register task templates
TaskerCore.start_embedded_orchestration!(['fulfillment'])

# Create a test task
task_request = TaskerCore::Types::TaskTypes::TaskRequest.new(
  namespace: 'fulfillment',
  name: 'order_fulfillment', 
  version: '1.0.0',
  context: { 
    order_id: 12345,
    order_items: [
      { product_id: 101, quantity: 2, price: 29.99 },
      { product_id: 102, quantity: 1, price: 49.99 }
    ],
    payment_info: { method: 'credit_card', amount: 109.97 },
    customer_info: { customer_id: 67890, email: 'test@example.com' },
    shipping_info: { address: '123 Test St', city: 'Test City', zip: '12345' }
  },
  initiator: 'debug_test',
  source_system: 'debug',
  reason: 'Debug DAG dependencies',
  priority: 5,
  claim_timeout_seconds: 300
)

task_result = TaskerCore.initialize_task_embedded(task_request.to_ffi_hash)
task_id = task_result['task_id']
puts "Task created: #{task_id}"

# Get direct database connection to check the DAG view
pgmq_client = TaskerCore::Messaging::PgmqClient.new
conn = pgmq_client.connection

# Find the ship_order step
ship_order_step = conn.exec("
  SELECT ws.workflow_step_id, ws.task_id, ns.name as step_name
  FROM tasker_workflow_steps ws
  JOIN tasker_named_steps ns ON ws.named_step_id = ns.named_step_id
  WHERE ws.task_id = #{task_id} AND ns.name = 'ship_order'
").first

if ship_order_step
  step_id = ship_order_step['workflow_step_id']
  puts "Found ship_order step: #{step_id}"
  
  # Check what the DAG relationship view shows for this step
  dag_result = conn.exec("
    SELECT workflow_step_id, parent_step_ids, parent_count
    FROM tasker_step_dag_relationships  
    WHERE workflow_step_id = #{step_id}
  ").first
  
  if dag_result
    puts "DAG relationship for ship_order:"
    puts "  parent_step_ids: #{dag_result['parent_step_ids']}"
    puts "  parent_count: #{dag_result['parent_count']}"
    
    # Parse the parent step IDs and look up their names
    require 'json'
    parent_ids = JSON.parse(dag_result['parent_step_ids'])
    puts "  Parsed parent IDs: #{parent_ids}"
    
    if parent_ids.any?
      parent_names_result = conn.exec("
        SELECT ws.workflow_step_id, ns.name as step_name
        FROM tasker_workflow_steps ws
        JOIN tasker_named_steps ns ON ws.named_step_id = ns.named_step_id
        WHERE ws.workflow_step_id = ANY('{#{parent_ids.join(',')}}')
        ORDER BY ws.workflow_step_id
      ")
      
      puts "  Parent step names:"
      parent_names_result.each do |row|
        puts "    - #{row['step_name']} (ID: #{row['workflow_step_id']})"
      end
    else
      puts "  No parent steps found"
    end
  else
    puts "No DAG relationship found for step #{step_id}"
  end
  
  # Also check the direct edges for comparison
  direct_edges_result = conn.exec("
    SELECT ws.workflow_step_id, ns.name as step_name
    FROM tasker_workflow_steps ws
    JOIN tasker_named_steps ns ON ws.named_step_id = ns.named_step_id
    JOIN tasker_workflow_step_edges wse ON wse.from_step_id = ws.workflow_step_id
    WHERE wse.to_step_id = #{step_id}
    ORDER BY ws.workflow_step_id
  ")
  
  puts "\nDirect edges (immediate parents):"
  direct_edges_result.each do |row|
    puts "  - #{row['step_name']} (ID: #{row['workflow_step_id']})"
  end
else
  puts "ship_order step not found"
end