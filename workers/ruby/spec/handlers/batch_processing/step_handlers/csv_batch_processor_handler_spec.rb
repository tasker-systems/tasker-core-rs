# frozen_string_literal: true

require 'spec_helper'
require 'tempfile'
require 'csv'

RSpec.describe TaskerCore::BatchProcessing::StepHandlers::CsvBatchProcessorHandler do
  let(:handler) { described_class.new }
  let(:mock_task) { instance_double(TaskerCore::Models::TaskWrapper, task_uuid: 'task-123', context: {}) }
  let(:mock_dependency_results) { instance_double(TaskerCore::Models::DependencyResultsWrapper) }
  let(:mock_workflow_step) { instance_double(TaskerCore::Models::WorkflowStepWrapper) }
  let(:mock_step_definition) do
    handler_def = double('handler', callable: 'CsvBatchProcessorHandler', initialization: {})
    double('step_definition', handler: handler_def)
  end
  let(:mock_context) do
    ctx = double('context',
                 task: mock_task,
                 workflow_step: mock_workflow_step,
                 dependency_results: mock_dependency_results,
                 step_definition: mock_step_definition)
    # Mock get_dependency_result to delegate to the dependency_results mock
    allow(ctx).to receive(:get_dependency_result) do |step_name|
      mock_dependency_results.get_results(step_name)
    end
    ctx
  end

  describe '#call' do
    context 'with no-op placeholder worker' do
      before do
        allow(mock_workflow_step).to receive(:inputs).and_return({
                                                                   'is_no_op' => true
                                                                 })
      end

      it 'returns success with no-op metadata' do
        result = handler.call(mock_context)

        expect(result.success?).to be true
        expect(result.result['no_op']).to be true
        expect(result.result['processed_count']).to eq(0)
        expect(result.result['batch_id']).to eq('unknown')
      end

      it 'does not process any CSV data' do
        result = handler.call(mock_context)

        expect(result.result.keys).to contain_exactly(
          'batch_id',
          'no_op',
          'processed_count'
        )
      end
    end

    context 'with real batch worker' do
      let(:csv_file) do
        Tempfile.new(['test_products', '.csv']).tap do |file|
          file.write("id,title,description,category,price,discountPercentage,rating,stock,brand,sku,weight\n")
          file.write("1,Product 1,Description 1,Electronics,100.00,5.0,4.5,50,Brand A,SKU1,1000\n")
          file.write("2,Product 2,Description 2,Electronics,150.00,10.0,4.8,30,Brand B,SKU2,1200\n")
          file.write("3,Product 3,Description 3,Furniture,200.00,15.0,4.2,20,Brand C,SKU3,5000\n")
          file.write("4,Product 4,Description 4,Electronics,250.00,20.0,4.9,40,Brand D,SKU4,800\n")
          file.write("5,Product 5,Description 5,Furniture,300.00,25.0,4.1,10,Brand E,SKU5,6000\n")
          file.close
        end
      end

      after do
        csv_file.unlink
      end

      # TAS-112: Updated to use 0-indexed cursors
      before do
        allow(mock_workflow_step).to receive(:inputs).and_return({
                                                                   'cursor' => {
                                                                     'batch_id' => '001',
                                                                     'start_cursor' => 0,
                                                                     'end_cursor' => 2
                                                                   },
                                                                   'batch_metadata' => {}
                                                                 })
        allow(mock_dependency_results).to receive(:get_results)
          .with('analyze_csv')
          .and_return({ 'csv_file_path' => csv_file.path })
      end

      it 'processes CSV rows in specified range' do
        result = handler.call(mock_context)

        expect(result.success?).to be true
        expect(result.result['batch_id']).to eq('001')
        expect(result.result['start_row']).to eq(0)
        expect(result.result['end_row']).to eq(2)
        expect(result.result['processed_count']).to eq(2)
      end

      it 'calculates inventory metrics correctly' do
        result = handler.call(mock_context)

        # Product 1: 100 * 50 = 5000
        # Product 2: 150 * 30 = 4500
        # Total = 9500
        expect(result.result['total_inventory_value']).to eq(9500.00)
      end

      it 'counts products by category' do
        result = handler.call(mock_context)

        expect(result.result['category_counts']).to eq({
                                                         'Electronics' => 2
                                                       })
      end

      it 'tracks maximum price product' do
        result = handler.call(mock_context)

        expect(result.result['max_price']).to eq(150.00)
        expect(result.result['max_price_product']).to eq('Product 2')
      end

      it 'calculates average rating' do
        result = handler.call(mock_context)

        # (4.5 + 4.8) / 2 = 4.65
        expect(result.result['average_rating']).to eq(4.65)
      end

      # TAS-112: Updated to use 0-indexed cursors (indices 2-4 = Products 3,4,5)
      context 'with different cursor range' do
        before do
          allow(mock_workflow_step).to receive(:inputs).and_return({
                                                                     'cursor' => {
                                                                       'batch_id' => '002',
                                                                       'start_cursor' => 2,
                                                                       'end_cursor' => 5
                                                                     }
                                                                   })
        end

        it 'processes different rows correctly' do
          result = handler.call(mock_context)

          expect(result.success?).to be true
          expect(result.result['batch_id']).to eq('002')
          expect(result.result['processed_count']).to eq(3)
          expect(result.result['category_counts']).to eq({
                                                           'Furniture' => 2,
                                                           'Electronics' => 1
                                                         })
        end
      end

      context 'with zero-row range' do
        before do
          allow(mock_workflow_step).to receive(:inputs).and_return({
                                                                     'cursor' => {
                                                                       'batch_id' => '003',
                                                                       'start_cursor' => 10,
                                                                       'end_cursor' => 10
                                                                     }
                                                                   })
        end

        it 'handles empty range gracefully' do
          result = handler.call(mock_context)

          expect(result.success?).to be true
          expect(result.result['processed_count']).to eq(0)
          expect(result.result['total_inventory_value']).to eq(0.00)
          expect(result.result['average_rating']).to eq(0.00)
        end
      end
    end

    context 'with missing CSV file path' do
      before do
        allow(mock_workflow_step).to receive(:inputs).and_return({
                                                                   'cursor' => {
                                                                     'batch_id' => '001',
                                                                     'start_cursor' => 0,
                                                                     'end_cursor' => 100
                                                                   }
                                                                 })
        allow(mock_dependency_results).to receive(:get_results)
          .with('analyze_csv')
          .and_return(nil)
      end

      it 'raises ArgumentError' do
        expect do
          handler.call(mock_context)
        end.to raise_error(ArgumentError, 'csv_file_path not found in analyze_csv results')
      end
    end

    context 'with missing result data' do
      before do
        allow(mock_workflow_step).to receive(:inputs).and_return({
                                                                   'cursor' => {
                                                                     'batch_id' => '001',
                                                                     'start_cursor' => 0,
                                                                     'end_cursor' => 100
                                                                   }
                                                                 })
        allow(mock_dependency_results).to receive(:get_results)
          .with('analyze_csv')
          .and_return({})
      end

      it 'raises ArgumentError when csv_file_path is missing' do
        expect do
          handler.call(mock_context)
        end.to raise_error(ArgumentError, 'csv_file_path not found in analyze_csv results')
      end
    end
  end

  describe 'handler inheritance' do
    it 'inherits from Batchable' do
      expect(handler).to be_a(TaskerCore::StepHandler::Batchable)
    end

    it 'has batch processing capabilities' do
      expect(handler.capabilities).to include('batchable')
      expect(handler.capabilities).to include('batch_processing')
      expect(handler.capabilities).to include('parallel_execution')
      expect(handler.capabilities).to include('cursor_based')
      expect(handler.capabilities).to include('deferred_convergence')
    end

    it 'has access to batch processing helper classes' do
      # Batch processing classes are auto-loaded via tasker_core.rb
      expect(TaskerCore::BatchProcessing::BatchWorkerContext).to be_a(Class)
      expect(TaskerCore::BatchProcessing::BatchAggregationScenario).to be_a(Class)
      expect(TaskerCore::Types::BatchProcessingOutcome).to be_a(Module)
    end
  end

  describe 'CSV parsing' do
    let(:csv_file) do
      Tempfile.new(['test_products', '.csv']).tap do |file|
        file.write("id,title,description,category,price,discountPercentage,rating,stock,brand,sku,weight\n")
        file.write("1,Product 1,Desc,Cat,99.99,5.5,4.7,100,Brand,SKU,500\n")
        file.close
      end
    end

    after do
      csv_file.unlink
    end

    # TAS-112: Updated to use 0-indexed cursors
    before do
      allow(mock_workflow_step).to receive(:inputs).and_return({
                                                                 'cursor' => {
                                                                   'batch_id' => '001',
                                                                   'start_cursor' => 0,
                                                                   'end_cursor' => 1
                                                                 }
                                                               })
      allow(mock_dependency_results).to receive(:get_results)
        .with('analyze_csv')
        .and_return({ 'csv_file_path' => csv_file.path })
    end

    it 'parses all CSV fields correctly' do
      result = handler.call(mock_context)

      expect(result.success?).to be true
      expect(result.result['processed_count']).to eq(1)
      expect(result.result['max_price']).to eq(99.99)
      expect(result.result['average_rating']).to eq(4.7)
    end
  end
end
