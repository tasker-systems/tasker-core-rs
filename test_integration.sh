#!/bin/bash

# Start TCP executor in background
echo "Starting TCP executor..."
cargo run --bin tcp_executor > tcp_executor.log 2>&1 &
TCP_PID=$!

# Give it time to start
sleep 3

# Check if it's running
if ps -p $TCP_PID > /dev/null; then
    echo "TCP executor started (PID: $TCP_PID)"
    
    # Run the Ruby test
    echo "Running Ruby health check test..."
    ruby test_tcp_debug.rb
    
    # Run the full integration example
    echo -e "\nRunning full integration example..."
    cd bindings/ruby && ruby examples/command_integration_example.rb
    
    # Kill the executor
    echo -e "\nStopping TCP executor..."
    kill $TCP_PID
else
    echo "Failed to start TCP executor"
    exit 1
fi