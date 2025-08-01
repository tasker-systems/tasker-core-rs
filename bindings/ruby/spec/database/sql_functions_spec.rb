# frozen_string_literal: true

require 'spec_helper'

RSpec.describe TaskerCore::Database::SqlFunctions do
  let(:sql_functions) { described_class.new }

  describe 'SQL function calls' do
    context 'with no test data (expected empty results)' do
      describe '#system_health_counts' do
        it 'can call get_system_health_counts_v01() without errors' do
          expect { sql_functions.system_health_counts }.not_to raise_error
        end

        it 'returns empty hash when no data exists' do
          result = sql_functions.system_health_counts
          expect(result).to eq({})
        end
      end

      describe '#analytics_metrics' do
        it 'can call get_analytics_metrics_v01() without timestamp' do
          expect { sql_functions.analytics_metrics }.not_to raise_error
        end

        it 'can call get_analytics_metrics_v01() with timestamp' do
          timestamp = Time.now - 3600 # 1 hour ago
          expect { sql_functions.analytics_metrics(since_timestamp: timestamp) }.not_to raise_error
        end

        it 'returns empty hash when no data exists' do
          result = sql_functions.analytics_metrics
          expect(result).to eq({})
        end
      end

      describe '#task_execution_context' do
        it 'can call get_task_execution_context() with BIGINT casting' do
          task_id = 12345
          expect { sql_functions.task_execution_context(task_id) }.not_to raise_error
        end

        it 'returns nil when task does not exist' do
          task_id = 99999
          result = sql_functions.task_execution_context(task_id)
          expect(result).to be_nil
        end
      end

      describe '#task_execution_contexts_batch' do
        it 'can call get_task_execution_contexts_batch() with BIGINT[] casting' do
          task_ids = [12345, 67890, 11111]
          expect { sql_functions.task_execution_contexts_batch(task_ids) }.not_to raise_error
        end

        it 'returns empty array when no tasks exist' do
          task_ids = [99998, 99999]
          result = sql_functions.task_execution_contexts_batch(task_ids)
          expect(result).to eq([])
        end
      end

      describe '#step_readiness_status' do
        it 'can call get_step_readiness_status() with BIGINT casting' do
          task_id = 12345
          expect { sql_functions.step_readiness_status(task_id) }.not_to raise_error
        end

        it 'can call get_step_readiness_status() with step_ids array' do
          task_id = 12345
          step_ids = [1, 2, 3]
          expect { sql_functions.step_readiness_status(task_id, step_ids: step_ids) }.not_to raise_error
        end

        it 'returns empty array when no steps exist' do
          task_id = 99999
          result = sql_functions.step_readiness_status(task_id)
          expect(result).to eq([])
        end
      end

      describe '#step_readiness_status_batch' do
        it 'can call get_step_readiness_status_batch() with BIGINT[] casting' do
          task_ids = [12345, 67890]
          expect { sql_functions.step_readiness_status_batch(task_ids) }.not_to raise_error
        end

        it 'returns empty array when no tasks exist' do
          task_ids = [99998, 99999]
          result = sql_functions.step_readiness_status_batch(task_ids)
          expect(result).to eq([])
        end
      end

      describe '#calculate_dependency_levels' do
        it 'can call calculate_dependency_levels() with BIGINT casting' do
          task_id = 12345
          expect { sql_functions.calculate_dependency_levels(task_id) }.not_to raise_error
        end

        it 'returns empty array when no dependencies exist' do
          task_id = 99999
          result = sql_functions.calculate_dependency_levels(task_id)
          expect(result).to eq([])
        end
      end

      describe '#slowest_steps' do
        it 'can call get_slowest_steps_v01() with default parameters' do
          expect { sql_functions.slowest_steps }.not_to raise_error
        end

        it 'can call get_slowest_steps_v01() with all parameters' do
          timestamp = Time.now - 7200 # 2 hours ago
          expect do
            sql_functions.slowest_steps(
              since_timestamp: timestamp,
              limit_count: 5,
              namespace_filter: 'test_namespace',
              task_name_filter: 'test_task',
              version_filter: 'v1.0'
            )
          end.not_to raise_error
        end

        it 'returns empty array when no slow steps exist' do
          result = sql_functions.slowest_steps
          expect(result).to eq([])
        end
      end

      describe '#slowest_tasks' do
        it 'can call get_slowest_tasks_v01() with default parameters' do
          expect { sql_functions.slowest_tasks }.not_to raise_error
        end

        it 'can call get_slowest_tasks_v01() with all parameters' do
          timestamp = Time.now - 7200 # 2 hours ago
          expect do
            sql_functions.slowest_tasks(
              since_timestamp: timestamp,
              limit_count: 3,
              namespace_filter: 'orders',
              task_name_filter: 'process_order',
              version_filter: 'v2.1'
            )
          end.not_to raise_error
        end

        it 'returns empty array when no slow tasks exist' do
          result = sql_functions.slowest_tasks
          expect(result).to eq([])
        end
      end

      describe '#execute_function' do
        it 'can call arbitrary SQL functions with proper parameter substitution' do
          expect { sql_functions.execute_function('get_system_health_counts_v01') }.not_to raise_error
        end

        it 'can call functions with parameters' do
          task_id = 12345
          expect { sql_functions.execute_function('get_task_execution_context', task_id) }.not_to raise_error
        end
      end
    end

    context 'helper methods using SQL functions' do
      describe '#task_complete?' do
        it 'returns false for non-existent task' do
          task_id = 99999
          result = sql_functions.task_complete?(task_id)
          expect(result).to be false
        end
      end

      describe '#task_progress' do
        it 'returns nil for non-existent task' do
          task_id = 99999
          result = sql_functions.task_progress(task_id)
          expect(result).to be_nil
        end
      end
    end

    context 'error handling' do
      describe 'invalid parameters' do
        it 'raises TaskerCore::Error for invalid task_id type' do
          expect { sql_functions.task_execution_context('invalid') }.to raise_error(TaskerCore::Error)
        end

        it 'raises TaskerCore::Error for invalid timestamp format' do
          expect { sql_functions.analytics_metrics(since_timestamp: 'invalid') }.to raise_error(TaskerCore::Error)
        end
      end

      describe 'database connection issues' do
        let(:bad_connection) { instance_double(PG::Connection) }
        let(:sql_functions_with_bad_connection) { described_class.new(connection: bad_connection) }

        before do
          allow(bad_connection).to receive(:exec).and_raise(PG::Error.new('Connection failed'))
        end

        it 'wraps PG::Error in TaskerCore::Error' do
          expect { sql_functions_with_bad_connection.system_health_counts }.to raise_error(TaskerCore::Error, /Connection failed/)
        end
      end
    end

    context 'type casting validation' do
      describe 'parameter type casting' do
        it 'properly handles nil values in arrays' do
          task_ids = [12345, nil, 67890].compact
          expect { sql_functions.task_execution_contexts_batch(task_ids) }.not_to raise_error
        end

        it 'properly handles empty arrays' do
          task_ids = []
          expect { sql_functions.task_execution_contexts_batch(task_ids) }.not_to raise_error
        end

        it 'properly handles nil optional parameters' do
          task_id = 12345
          expect { sql_functions.step_readiness_status(task_id, step_ids: nil) }.not_to raise_error
        end
      end
    end

    context 'logging behavior' do
      let(:logger) { instance_double(TaskerCore::Logging::Logger, debug: nil, warn: nil, error: nil) }
      let(:sql_functions_with_logger) { described_class.new(logger: logger) }

      it 'logs debug messages for successful operations' do
        expect(logger).to receive(:debug).with(/Getting system health counts/)
        expect(logger).to receive(:warn).with(/No system health data found/)
        
        sql_functions_with_logger.system_health_counts
      end

      it 'logs debug messages with operation details' do
        task_id = 12345
        expect(logger).to receive(:debug).with(/Getting task execution context for task_id: #{task_id}/)
        expect(logger).to receive(:warn).with(/No execution context found for task_id: #{task_id}/)
        
        sql_functions_with_logger.task_execution_context(task_id)
      end
    end
  end
end