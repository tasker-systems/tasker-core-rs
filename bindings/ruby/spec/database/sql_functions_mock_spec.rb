# frozen_string_literal: true

require 'spec_helper'

RSpec.describe TaskerCore::Database::SqlFunctions do
  let(:mock_connection) { instance_double(PG::Connection) }
  let(:mock_result) { instance_double(PG::Result) }
  let(:sql_functions) { described_class.new(connection: mock_connection) }

  before do
    allow(mock_result).to receive_messages(ntuples: 0, map: [])
    allow(mock_connection).to receive(:exec).and_return(mock_result)
  end

  describe 'SQL function signatures and type casting' do
    describe '#system_health_counts' do
      it 'calls get_system_health_counts_v01() with correct signature' do
        expect(mock_connection).to receive(:exec).with('SELECT * FROM get_system_health_counts_v01()')

        sql_functions.system_health_counts
      end
    end

    describe '#analytics_metrics' do
      it 'calls get_analytics_metrics_v01() with TIMESTAMP WITH TIME ZONE casting' do
        timestamp = Time.parse('2024-01-01 12:00:00 UTC')

        expect(mock_connection).to receive(:exec).with(
          'SELECT * FROM get_analytics_metrics_v01($1::TIMESTAMP WITH TIME ZONE)',
          [timestamp.utc.iso8601]
        )

        sql_functions.analytics_metrics(since_timestamp: timestamp)
      end

      it 'handles nil timestamp correctly' do
        expect(mock_connection).to receive(:exec).with(
          'SELECT * FROM get_analytics_metrics_v01($1::TIMESTAMP WITH TIME ZONE)',
          [nil]
        )

        sql_functions.analytics_metrics(since_timestamp: nil)
      end
    end

    describe '#task_execution_context' do
      it 'calls get_task_execution_context() with BIGINT casting' do
        task_id = 12_345

        expect(mock_connection).to receive(:exec).with(
          'SELECT * FROM get_task_execution_context($1::BIGINT)',
          [task_id]
        )

        sql_functions.task_execution_context(task_id)
      end
    end

    describe '#task_execution_contexts_batch' do
      it 'calls get_task_execution_contexts_batch() with BIGINT[] casting' do
        task_ids = [12_345, 67_890, 11_111]

        expect(mock_connection).to receive(:exec).with(
          'SELECT * FROM get_task_execution_contexts_batch($1::BIGINT[])',
          [task_ids]
        )

        sql_functions.task_execution_contexts_batch(task_ids)
      end
    end

    describe '#step_readiness_status' do
      it 'calls get_step_readiness_status() with BIGINT castings' do
        task_id = 12_345
        step_ids = [1, 2, 3]

        expect(mock_connection).to receive(:exec).with(
          'SELECT * FROM get_step_readiness_status($1::BIGINT, $2::BIGINT[])',
          [task_id, step_ids]
        )

        sql_functions.step_readiness_status(task_id, step_ids: step_ids)
      end

      it 'handles nil step_ids correctly' do
        task_id = 12_345

        expect(mock_connection).to receive(:exec).with(
          'SELECT * FROM get_step_readiness_status($1::BIGINT, $2::BIGINT[])',
          [task_id, nil]
        )

        sql_functions.step_readiness_status(task_id, step_ids: nil)
      end
    end

    describe '#step_readiness_status_batch' do
      it 'calls get_step_readiness_status_batch() with BIGINT[] casting' do
        task_ids = [12_345, 67_890]

        expect(mock_connection).to receive(:exec).with(
          'SELECT * FROM get_step_readiness_status_batch($1::BIGINT[])',
          [task_ids]
        )

        sql_functions.step_readiness_status_batch(task_ids)
      end
    end

    describe '#calculate_dependency_levels' do
      it 'calls calculate_dependency_levels() with BIGINT casting' do
        task_id = 12_345

        expect(mock_connection).to receive(:exec).with(
          'SELECT * FROM calculate_dependency_levels($1::BIGINT)',
          [task_id]
        )

        sql_functions.calculate_dependency_levels(task_id)
      end
    end

    describe '#slowest_steps' do
      it 'calls get_slowest_steps_v01() with proper type casting for all parameters' do
        timestamp = Time.parse('2024-01-01 10:00:00 UTC')

        expect(mock_connection).to receive(:exec).with(
          'SELECT * FROM get_slowest_steps_v01($1::TIMESTAMP WITH TIME ZONE, $2::INTEGER, $3::TEXT, $4::TEXT, $5::TEXT)',
          [timestamp.utc.iso8601, 5, 'test_namespace', 'test_task', 'v1.0']
        )

        sql_functions.slowest_steps(
          since_timestamp: timestamp,
          limit_count: 5,
          namespace_filter: 'test_namespace',
          task_name_filter: 'test_task',
          version_filter: 'v1.0'
        )
      end

      it 'handles default parameters correctly' do
        expect(mock_connection).to receive(:exec).with(
          'SELECT * FROM get_slowest_steps_v01($1::TIMESTAMP WITH TIME ZONE, $2::INTEGER, $3::TEXT, $4::TEXT, $5::TEXT)',
          [nil, 10, nil, nil, nil]
        )

        sql_functions.slowest_steps
      end
    end

    describe '#slowest_tasks' do
      it 'calls get_slowest_tasks_v01() with proper type casting for all parameters' do
        timestamp = Time.parse('2024-01-01 08:00:00 UTC')

        expect(mock_connection).to receive(:exec).with(
          'SELECT * FROM get_slowest_tasks_v01($1::TIMESTAMP WITH TIME ZONE, $2::INTEGER, $3::TEXT, $4::TEXT, $5::TEXT)',
          [timestamp.utc.iso8601, 3, 'orders', 'process_order', 'v2.1']
        )

        sql_functions.slowest_tasks(
          since_timestamp: timestamp,
          limit_count: 3,
          namespace_filter: 'orders',
          task_name_filter: 'process_order',
          version_filter: 'v2.1'
        )
      end

      it 'handles default parameters correctly' do
        expect(mock_connection).to receive(:exec).with(
          'SELECT * FROM get_slowest_tasks_v01($1::TIMESTAMP WITH TIME ZONE, $2::INTEGER, $3::TEXT, $4::TEXT, $5::TEXT)',
          [nil, 10, nil, nil, nil]
        )

        sql_functions.slowest_tasks
      end
    end

    describe '#execute_function' do
      it 'builds correct SQL with parameter placeholders' do
        expect(mock_connection).to receive(:exec).with(
          'SELECT * FROM get_task_execution_context($1)',
          [12_345]
        )

        sql_functions.execute_function('get_task_execution_context', 12_345)
      end

      it 'handles functions with no parameters' do
        expect(mock_connection).to receive(:exec).with(
          'SELECT * FROM get_system_health_counts_v01()',
          []
        )

        sql_functions.execute_function('get_system_health_counts_v01')
      end

      it 'handles functions with multiple parameters' do
        expect(mock_connection).to receive(:exec).with(
          'SELECT * FROM custom_function($1, $2, $3)',
          ['param1', 123, true]
        )

        sql_functions.execute_function('custom_function', 'param1', 123, true)
      end
    end
  end

  describe 'result parsing validation' do
    let(:mock_row) do
      {
        'task_id' => '12345',
        'named_task_id' => '999',
        'status' => 'completed',
        'total_steps' => '10',
        'pending_steps' => '0',
        'in_progress_steps' => '0',
        'completed_steps' => '10',
        'failed_steps' => '0',
        'ready_steps' => '0',
        'execution_status' => 'success',
        'recommended_action' => 'none',
        'completion_percentage' => '100.0',
        'health_status' => 'healthy'
      }
    end

    before do
      allow(mock_result).to receive(:ntuples).and_return(1)
      allow(mock_result).to receive(:[]).with(0).and_return(mock_row)
    end

    it 'parses task execution context correctly' do
      result = sql_functions.task_execution_context(12_345)

      expect(result).to include(
        task_id: 12_345,
        named_task_id: 999,
        status: 'completed',
        total_steps: 10,
        completion_percentage: 100.0,
        health_status: 'healthy'
      )
    end
  end

  describe 'error handling' do
    before do
      allow(mock_connection).to receive(:exec).and_raise(PG::Error.new('Test database error'))
    end

    it 'wraps PG::Error in TaskerCore::Error' do
      expect { sql_functions.system_health_counts }.to raise_error(TaskerCore::Error, /Test database error/)
    end

    it 'includes function context in error message' do
      expect do
        sql_functions.task_execution_context(123)
      end.to raise_error(TaskerCore::Error, /Failed to get task execution context/)
    end
  end
end
