# frozen_string_literal: true

require 'concurrent'

class SharedTestLoop
  def initialize
    # Use concurrent-ruby thread-safe array to track both workers and their actual threads
    @test_workers = Concurrent::Array.new
    @worker_threads = Concurrent::Array.new
    @logger = TaskerCore::Logging::Logger.instance
  end

  def create_workers(namespace:, num_workers: 2, poll_interval: 0.1)
    num_workers.times do |i|
      @test_workers << TaskerCore::Messaging.create_queue_worker(
        namespace,
        poll_interval: poll_interval # Fast polling for test
      )
    end
  end

  def cleanup_workers
    puts "🧹 Starting worker cleanup - #{@test_workers.length} workers, #{@worker_threads.length} captured threads"

    # 1. Signal all workers to stop gracefully (but don't wait)
    @test_workers.each(&:stop)

    # 2. Kill all captured thread handles directly
    @worker_threads.each_with_index do |thread, index|
      if thread&.alive?
        puts "💀 Killing captured thread #{index + 1} (object_id: #{thread.object_id}, status: #{thread.status})"
        thread.kill
        # Brief wait for cleanup
        sleep 0.1
      else
        puts "✅ Thread #{index + 1} already stopped (object_id: #{thread&.object_id || 'nil'})"
      end
    end

    # 3. Clean up worker state for good measure
    @test_workers.each do |worker|
      worker.instance_variable_set(:@running, false)
      worker.instance_variable_set(:@worker_thread, nil)
    end
  end

  def start
    @test_workers.each do |worker|
      unless worker.start
        raise TaskerCore::Errors::OrchestrationError, "Could not start worker #{worker.inspect}"
      end

      # Capture the actual thread handle from the worker
      worker_thread = worker.instance_variable_get(:@worker_thread)
      if worker_thread
        @worker_threads << worker_thread
        puts "📝 Captured thread handle for namespace #{worker.namespace}: #{worker_thread.object_id}"
      else
        puts "⚠️  WARNING: No thread handle found for worker #{worker.namespace}"
      end
    end
  end

  def stop
    cleanup_workers
    @test_workers.clear
    @worker_threads.clear
  end

  def run_loop(task_id:, timeout:)
    Timeout.timeout(timeout) do
      loop do
        tec = TaskerCore::Database::Functions::FunctionBasedTaskExecutionContext.find(task_id)
        if tec.completion_percentage.to_i == 100
          # Task completed - immediately stop workers
          @logger.info "✅ Task completed, stopping workers immediately..."
          cleanup_workers
          break
        else
          @logger.warn("⚠️ SHARED_TEST_LOOP: Task not completed yet, completion percentage: #{tec.completion_percentage}%")
        end

        sleep 1
      end
    end

    final_execution = TaskerCore::Database::Functions::FunctionBasedTaskExecutionContext.find(task_id)

    raise TaskerCore::Errors::OrchestrationError, "Task did not complete" unless final_execution.completion_percentage == 100.0

    TaskerCore::Database::Models::Task.with_all_associated.find(task_id)
  end

  def create_task(task_request:)
    base_handler = TaskerCore::Internal::OrchestrationManager.instance.base_task_handler
    raise TaskerCore::Errors::OrchestrationError, "Base task handler is nil" unless base_handler

    task_result = TaskerCore.initialize_task_embedded(task_request.to_ffi_hash)
    raise TaskerCore::Errors::OrchestrationError, "Task creation failed" unless task_result['success']

    task_result['task_id']
  end

  def run(task_request:, namespace:, num_workers: 2, timeout: 10, worker_poll_interval: 0.1)
    task_id = create_task(task_request: task_request)
    create_workers(namespace: namespace, num_workers: num_workers, poll_interval: worker_poll_interval)
    start
    task = nil
    workers_stopped = false
    begin
      task = run_loop(task_id: task_id, timeout: timeout)
      workers_stopped = true # Workers were stopped in run_loop when task completed
    rescue Timeout::Error => e
      puts "⚠️  WARNING: Task execution timed out after #{timeout} seconds"
      raise e
    ensure
      # Only stop workers if they weren't already stopped due to task completion
      unless workers_stopped
        stop
      end
      # Give a moment for workers to fully clean up
      sleep 0.2
      # Clear both arrays regardless
      @test_workers.clear
      @worker_threads.clear
    end
    task
  end
end
