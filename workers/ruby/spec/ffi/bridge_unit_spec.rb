# frozen_string_literal: true

require 'spec_helper'

RSpec.describe 'TaskerCore::FFI Bridge' do
  describe 'module existence and method availability' do
    it 'exposes expected FFI methods' do
      expect(TaskerCore::FFI).to respond_to(:bootstrap_worker)
      expect(TaskerCore::FFI).to respond_to(:stop_worker)
      expect(TaskerCore::FFI).to respond_to(:worker_status)
      expect(TaskerCore::FFI).to respond_to(:transition_to_graceful_shutdown)
    end
  end

  describe 'bootstrap_worker' do
    context 'when called for first time' do
      it 'returns a hash with status, worker_id, and handle_id' do
        # Assuming this doesn't actually start anything in unit test context
        result = TaskerCore::FFI.bootstrap_worker

        expect(result).to be_a(Hash)
        expect(result.keys).to include('status', 'worker_id', 'handle_id')
        expect(%w[started already_running]).to include(result['status'])
        expect(result['worker_id']).to match(/^ruby-worker-[a-f0-9-]+$/)
        expect(result['handle_id']).to match(/^[a-f0-9-]+$/)
      end
    end

    context 'when called multiple times' do
      it 'handles repeated bootstrap calls gracefully' do
        # First call should work
        expect { TaskerCore::FFI.bootstrap_worker }.not_to raise_error

        # Second call should also work (return already_running)
        expect { TaskerCore::FFI.bootstrap_worker }.not_to raise_error

        # Clean up
        begin
          TaskerCore::FFI.stop_worker
        rescue StandardError
          nil
        end
      end
    end

    context 'when system already bootstrapped' do
      before do
        # Bootstrap once
        TaskerCore::FFI.bootstrap_worker
      end

      after do
        # Clean up

        TaskerCore::FFI.stop_worker
      rescue StandardError
        nil
      end

      it 'returns already_running status' do
        result = TaskerCore::FFI.bootstrap_worker

        expect(result['status']).to eq('already_running')
        expect(result['handle_id']).to match(/^[a-f0-9-]+$/)
        # NOTE: already_running response may not include worker_id
      end
    end
  end

  describe 'worker_status' do
    context 'when no worker is running' do
      before do
        # Ensure clean state

        TaskerCore::FFI.stop_worker
      rescue StandardError
        nil
      end

      it 'returns status indicating not running' do
        status = TaskerCore::FFI.worker_status

        expect(status).to be_a(Hash)
        expect(status['running']).to be false
      end
    end

    context 'when worker is running' do
      before do
        TaskerCore::FFI.bootstrap_worker
      end

      after do
        TaskerCore::FFI.stop_worker
      rescue StandardError
        nil
      end

      it 'returns status indicating running state' do
        status = TaskerCore::FFI.worker_status

        expect(status).to be_a(Hash)
        expect(status['running']).to be true
        expect(status).to have_key('worker_core_status')
      end
    end
  end

  describe 'transition_to_graceful_shutdown' do
    context 'when worker is running' do
      before do
        TaskerCore::FFI.bootstrap_worker
      end

      after do
        TaskerCore::FFI.stop_worker
      rescue StandardError
        nil
      end

      it 'does not raise error' do
        expect { TaskerCore::FFI.transition_to_graceful_shutdown }.not_to raise_error
      end
    end

    context 'when no worker is running' do
      before do
        TaskerCore::FFI.stop_worker
      rescue StandardError
        nil
      end

      it 'handles gracefully when no worker to shutdown' do
        # May raise an error when no worker is running, which is acceptable behavior
        expect { TaskerCore::FFI.transition_to_graceful_shutdown }.to raise_error(RuntimeError)
      end
    end
  end

  describe 'stop_worker' do
    context 'when worker is running' do
      before do
        TaskerCore::FFI.bootstrap_worker
      end

      it 'stops the worker successfully' do
        expect { TaskerCore::FFI.stop_worker }.not_to raise_error

        # Verify worker is no longer running
        status = TaskerCore::FFI.worker_status
        expect(status['running']).to be false
      end
    end

    context 'when no worker is running' do
      before do
        TaskerCore::FFI.stop_worker
      rescue StandardError
        nil
      end

      it 'handles stop gracefully when nothing running' do
        expect { TaskerCore::FFI.stop_worker }.not_to raise_error
      end
    end
  end

  describe 'error handling' do
    it 'raises error for invalid method calls' do
      # bootstrap_worker doesn't accept arguments, so this should raise ArgumentError
      expect do
        TaskerCore::FFI.bootstrap_worker({ invalid_config: 'bad_value' })
      end.to raise_error(ArgumentError)
    end
  end

  describe 'thread safety' do
    it 'handles concurrent bootstrap calls safely' do
      threads = []
      results = []

      # Try to bootstrap from multiple threads simultaneously
      5.times do
        threads << Thread.new do
          result = TaskerCore::FFI.bootstrap_worker
          results << result
        rescue StandardError => e
          results << { error: e.message }
        end
      end

      threads.each(&:join)

      # Should have at least one success, others should be "already_running"
      successful_starts = results.count { |r| r['status'] == 'started' }
      already_running = results.count { |r| r['status'] == 'already_running' }

      expect(successful_starts).to be >= 1
      expect(successful_starts + already_running).to eq(5)

      # Clean up
      begin
        TaskerCore::FFI.stop_worker
      rescue StandardError
        nil
      end
    end
  end

  describe 'lifecycle integration' do
    it 'supports complete bootstrap -> status -> shutdown cycle' do
      # Bootstrap
      bootstrap_result = TaskerCore::FFI.bootstrap_worker

      expect(%w[started already_running]).to include(bootstrap_result['status'])

      # Check status
      status = TaskerCore::FFI.worker_status
      expect(status['running']).to be true

      # Graceful shutdown transition
      expect { TaskerCore::FFI.transition_to_graceful_shutdown }.not_to raise_error

      # Stop
      expect { TaskerCore::FFI.stop_worker }.not_to raise_error

      # Verify stopped
      final_status = TaskerCore::FFI.worker_status
      expect(final_status['running']).to be false
    end
  end

  describe 'return value consistency' do
    it 'always returns Hash with string keys (not symbols)' do
      result = TaskerCore::FFI.bootstrap_worker

      # All keys should be strings, not symbols
      expect(result.keys).to all(be_a(String))

      # Common expected keys should be strings
      expect(result).to have_key('status')
      expect(result).to have_key('handle_id')
      # NOTE: worker_id may not be present in all responses

      begin
        TaskerCore::FFI.stop_worker
      rescue StandardError
        nil
      end
    end

    it 'worker_status always returns Hash with string keys' do
      TaskerCore::FFI.bootstrap_worker
      status = TaskerCore::FFI.worker_status

      expect(status.keys).to all(be_a(String))
      expect(status).to have_key('running')

      begin
        TaskerCore::FFI.stop_worker
      rescue StandardError
        nil
      end
    end
  end
end
