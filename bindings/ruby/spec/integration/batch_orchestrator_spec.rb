# frozen_string_literal: true

require 'spec_helper'

RSpec.describe 'BatchStepExecutionOrchestrator Implementation', type: :integration do
  let(:orchestrator_class) { TaskerCore::Orchestration::BatchStepExecutionOrchestrator }
  let(:registry_class) { TaskerCore::Orchestration::EnhancedHandlerRegistry }
  let(:registry) { registry_class.new }

  describe 'Class Loading' do
    it 'loads BatchStepExecutionOrchestrator successfully' do
      expect(orchestrator_class).to be_a(Class)
    end

    it 'loads EnhancedHandlerRegistry successfully' do
      expect(registry_class).to be_a(Class)
    end
  end

  describe 'EnhancedHandlerRegistry' do
    describe 'callable registration' do
      it 'registers Proc successfully' do
        expect {
          registry.register_proc('TestHandler') do |task, sequence, step|
            { status: 'completed', output: { message: "Processed step #{step.step_id}" } }
          end
        }.not_to raise_error
      end

      it 'registers Lambda successfully' do
        test_lambda = ->(task, sequence, step) { { status: 'completed', output: 'lambda result' } }
        
        expect {
          registry.register_callable('LambdaHandler', test_lambda)
        }.not_to raise_error
      end
    end

    describe 'callable retrieval' do
      before do
        registry.register_proc('TestHandler') do |task, sequence, step|
          { status: 'completed', output: { message: "Processed step #{step.step_id}" } }
        end
      end

      it 'retrieves registered callable' do
        callable = registry.get_callable_for_class('TestHandler')
        expect(callable).to respond_to(:call)
      end

      it 'provides registry statistics' do
        stats = registry.stats
        expect(stats).to be_a(Hash)
        expect(stats[:total_callables]).to be > 0
      end
    end
  end

  describe 'BatchStepExecutionOrchestrator' do
    let(:orchestrator) do
      orchestrator_class.new(
        step_sub_endpoint: 'inproc://test_steps',
        result_pub_endpoint: 'inproc://test_results',
        max_workers: 5,
        handler_registry: registry
      )
    end

    before do
      registry.register_proc('TestHandler') do |task, sequence, step|
        { status: 'completed', output: { message: "Processed step #{step.step_id}" } }
      end
    end

    describe 'instantiation' do
      it 'creates orchestrator successfully' do
        expect(orchestrator).to be_a(orchestrator_class)
      end

      it 'provides orchestrator statistics' do
        stats = orchestrator.stats
        expect(stats).to be_a(Hash)
        expect(stats[:max_workers]).to eq(5)
        expect(stats[:running]).to be false
      end
    end

    describe 'data structures' do
      describe 'TaskStruct' do
        it 'creates TaskStruct successfully' do
          task_struct = orchestrator_class::TaskStruct.new(
            task_id: 123,
            context: { order_id: 'order_123', customer: 'John Doe' },
            metadata: { priority: 'high' }
          )

          expect(task_struct.task_id).to eq(123)
          expect(task_struct.context[:order_id]).to eq('order_123')
          expect(task_struct.metadata[:priority]).to eq('high')
        end
      end

      describe 'SequenceStruct' do
        it 'creates SequenceStruct successfully' do
          sequence_struct = orchestrator_class::SequenceStruct.new(
            sequence_number: 1,
            total_steps: 3,
            previous_results: { validate_order: { status: 'completed' } }
          )

          expect(sequence_struct.sequence_number).to eq(1)
          expect(sequence_struct.total_steps).to eq(3)
          expect(sequence_struct.previous_results[:validate_order][:status]).to eq('completed')
        end
      end

      describe 'StepStruct' do
        it 'creates StepStruct successfully' do
          step_struct = orchestrator_class::StepStruct.new(
            step_id: 456,
            step_name: 'process_payment',
            handler_config: { timeout_seconds: 30 },
            timeout_ms: 30000,
            retry_limit: 3
          )

          expect(step_struct.step_id).to eq(456)
          expect(step_struct.step_name).to eq('process_payment')
          expect(step_struct.handler_config[:timeout_seconds]).to eq(30)
          expect(step_struct.timeout_ms).to eq(30000)
          expect(step_struct.retry_limit).to eq(3)
        end
      end
    end

    describe 'callable resolution and execution' do
      let(:task_struct) do
        orchestrator_class::TaskStruct.new(
          task_id: 123,
          context: { order_id: 'order_123', customer: 'John Doe' },
          metadata: { priority: 'high' }
        )
      end

      let(:sequence_struct) do
        orchestrator_class::SequenceStruct.new(
          sequence_number: 1,
          total_steps: 3,
          previous_results: { validate_order: { status: 'completed' } }
        )
      end

      let(:step_struct) do
        orchestrator_class::StepStruct.new(
          step_id: 456,
          step_name: 'process_payment',
          handler_config: { timeout_seconds: 30 },
          timeout_ms: 30000,
          retry_limit: 3
        )
      end

      let(:step_data) do
        {
          step_id: 456,
          task_id: 123,
          step_name: 'process_payment',
          handler_class: 'TestHandler',
          handler_config: { timeout_seconds: 30 },
          task_context: { order_id: 'order_123' },
          previous_results: {},
          metadata: { sequence: 1, total_steps: 3 }
        }
      end

      it 'resolves callable for handler class' do
        callable = registry.get_callable_for_class(step_data[:handler_class])
        expect(callable).not_to be_nil
        expect(callable).to respond_to(:call)
      end

      it 'executes callable successfully' do
        callable = registry.get_callable_for_class(step_data[:handler_class])
        result = callable.call(task_struct, sequence_struct, step_struct)
        
        expect(result).to be_a(Hash)
        expect(result[:status]).to eq('completed')
        expect(result[:output]).to be_a(Hash)
      end
    end
  end

  describe 'Integration Readiness' do
    it 'has all required components for ZeroMQ integration' do
      expect(orchestrator_class).to respond_to(:new)
      expect(registry_class).to respond_to(:new)
      
      orchestrator = orchestrator_class.new(
        step_sub_endpoint: 'inproc://test',
        result_pub_endpoint: 'inproc://test_results',
        max_workers: 1,
        handler_registry: registry
      )
      
      expect(orchestrator).to respond_to(:start)
      expect(orchestrator).to respond_to(:stop)
      expect(orchestrator).to respond_to(:stats)
    end

    it 'validates component dependencies' do
      # Verify required gems are available
      expect { require 'ffi-rzmq' }.not_to raise_error
      expect { require 'concurrent-ruby' }.not_to raise_error
      expect { require 'dry-struct' }.not_to raise_error
    end
  end
end