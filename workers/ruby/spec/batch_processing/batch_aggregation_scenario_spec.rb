# frozen_string_literal: true

require 'spec_helper'

RSpec.describe TaskerCore::BatchProcessing::BatchAggregationScenario do
  describe '.detect' do
    context 'with NoBatches scenario' do
      it 'detects when no batch workers exist' do
        dependency_results = TaskerCore::Models::DependencyResultsWrapper.new({
                                                                                'analyze_dataset' => { 'total_rows' => 0 }
                                                                              })

        scenario = described_class.detect(
          dependency_results,
          'analyze_dataset',
          'process_batch_'
        )

        expect(scenario.no_batches?).to be true
        expect(scenario.with_batches?).to be false
        expect(scenario.worker_count).to eq(0)
        expect(scenario.type).to eq(:no_batches)
      end

      it 'returns batchable result for NoBatches scenario' do
        batchable_result = { 'message' => 'No data to process' }
        dependency_results = TaskerCore::Models::DependencyResultsWrapper.new({
                                                                                'analyze_dataset' => batchable_result
                                                                              })

        scenario = described_class.detect(
          dependency_results,
          'analyze_dataset',
          'process_batch_'
        )

        expect(scenario.batchable_result).to eq(batchable_result)
      end
    end

    context 'with WithBatches scenario' do
      it 'detects when batch workers exist' do
        dependency_results = TaskerCore::Models::DependencyResultsWrapper.new({
                                                                                'analyze_dataset' => { 'total_rows' => 1000 },
                                                                                'process_batch_001' => { 'processed_count' => 100 },
                                                                                'process_batch_002' => { 'processed_count' => 150 },
                                                                                'process_batch_003' => { 'processed_count' => 200 }
                                                                              })

        scenario = described_class.detect(
          dependency_results,
          'analyze_dataset',
          'process_batch_'
        )

        expect(scenario.with_batches?).to be true
        expect(scenario.no_batches?).to be false
        expect(scenario.worker_count).to eq(3)
        expect(scenario.type).to eq(:with_batches)
      end

      it 'collects all batch worker results' do
        dependency_results = TaskerCore::Models::DependencyResultsWrapper.new({
                                                                                'analyze_dataset' => { 'total_rows' => 500 },
                                                                                'process_batch_001' => { 'total' => 100 },
                                                                                'process_batch_002' => { 'total' => 200 }
                                                                              })

        scenario = described_class.detect(
          dependency_results,
          'analyze_dataset',
          'process_batch_'
        )

        expect(scenario.batch_results['process_batch_001']).to eq({ 'total' => 100 })
        expect(scenario.batch_results['process_batch_002']).to eq({ 'total' => 200 })
        expect(scenario.batch_results.size).to eq(2)
      end
    end

    context 'with different batch worker prefixes' do
      it 'matches batch workers by prefix correctly' do
        dependency_results = TaskerCore::Models::DependencyResultsWrapper.new({
                                                                                'split_data' => { 'chunks' => 5 },
                                                                                'chunk_worker_001' => { 'result' => 'data1' },
                                                                                'chunk_worker_002' => { 'result' => 'data2' },
                                                                                'other_step_001' => { 'result' => 'other' }
                                                                              })

        scenario = described_class.detect(
          dependency_results,
          'split_data',
          'chunk_worker_'
        )

        expect(scenario.worker_count).to eq(2)
        expect(scenario.batch_results.keys).to contain_exactly(
          'chunk_worker_001',
          'chunk_worker_002'
        )
        expect(scenario.batch_results.keys).not_to include('other_step_001')
      end
    end
  end

  describe '#no_batches?' do
    it 'returns true when type is :no_batches' do
      dependency_results = TaskerCore::Models::DependencyResultsWrapper.new({
                                                                              'test_step' => {}
                                                                            })

      scenario = described_class.detect(
        dependency_results,
        'test_step',
        'batch_'
      )

      expect(scenario.no_batches?).to be true
    end

    it 'returns false when type is :with_batches' do
      dependency_results = TaskerCore::Models::DependencyResultsWrapper.new({
                                                                              'test_step' => {},
                                                                              'batch_001' => {}
                                                                            })

      scenario = described_class.detect(
        dependency_results,
        'test_step',
        'batch_'
      )

      expect(scenario.no_batches?).to be false
    end
  end

  describe '#with_batches?' do
    it 'returns true when type is :with_batches' do
      dependency_results = TaskerCore::Models::DependencyResultsWrapper.new({
                                                                              'test_step' => {},
                                                                              'batch_001' => {}
                                                                            })

      scenario = described_class.detect(
        dependency_results,
        'test_step',
        'batch_'
      )

      expect(scenario.with_batches?).to be true
    end

    it 'returns false when type is :no_batches' do
      dependency_results = TaskerCore::Models::DependencyResultsWrapper.new({
                                                                              'test_step' => {}
                                                                            })

      scenario = described_class.detect(
        dependency_results,
        'test_step',
        'batch_'
      )

      expect(scenario.with_batches?).to be false
    end
  end

  describe '#worker_count' do
    it 'returns 0 for NoBatches scenario' do
      dependency_results = TaskerCore::Models::DependencyResultsWrapper.new({
                                                                              'test_step' => {}
                                                                            })

      scenario = described_class.detect(
        dependency_results,
        'test_step',
        'batch_'
      )

      expect(scenario.worker_count).to eq(0)
    end

    it 'returns correct count for WithBatches scenario' do
      dependency_results = TaskerCore::Models::DependencyResultsWrapper.new({
                                                                              'test_step' => {},
                                                                              'batch_001' => {},
                                                                              'batch_002' => {},
                                                                              'batch_003' => {},
                                                                              'batch_004' => {},
                                                                              'batch_005' => {}
                                                                            })

      scenario = described_class.detect(
        dependency_results,
        'test_step',
        'batch_'
      )

      expect(scenario.worker_count).to eq(5)
    end
  end

  describe 'edge cases' do
    it 'handles batchable step with only itself in dependencies' do
      dependency_results = TaskerCore::Models::DependencyResultsWrapper.new({
                                                                              'single_step' => { 'status' => 'complete' }
                                                                            })

      scenario = described_class.detect(
        dependency_results,
        'single_step',
        'worker_'
      )

      expect(scenario.no_batches?).to be true
      expect(scenario.batchable_result).to eq({ 'status' => 'complete' })
    end

    it 'handles empty dependency results' do
      dependency_results = TaskerCore::Models::DependencyResultsWrapper.new({})

      scenario = described_class.detect(
        dependency_results,
        'missing_step',
        'worker_'
      )

      expect(scenario.no_batches?).to be true
      expect(scenario.batchable_result).to be_nil
    end

    it 'handles steps with similar but non-matching prefixes' do
      dependency_results = TaskerCore::Models::DependencyResultsWrapper.new({
                                                                              'test_step' => {},
                                                                              'process_batch_001' => {},
                                                                              'pre_process_batch_002' => {},
                                                                              'batch_process_003' => {}
                                                                            })

      scenario = described_class.detect(
        dependency_results,
        'test_step',
        'process_batch_'
      )

      expect(scenario.worker_count).to eq(1)
      expect(scenario.batch_results.keys).to eq(['process_batch_001'])
    end
  end

  describe 'realistic aggregation example' do
    it 'aggregates metrics from multiple batch workers' do
      dependency_results = TaskerCore::Models::DependencyResultsWrapper.new({
                                                                              'analyze_csv' => { 'total_rows' => 1000 },
                                                                              'process_batch_001' => {
                                                                                'processed_count' => 200, 'total_inventory_value' => 15_000.50
                                                                              },
                                                                              'process_batch_002' => {
                                                                                'processed_count' => 200, 'total_inventory_value' => 18_250.75
                                                                              },
                                                                              'process_batch_003' => {
                                                                                'processed_count' => 200, 'total_inventory_value' => 12_100.25
                                                                              },
                                                                              'process_batch_004' => {
                                                                                'processed_count' => 200, 'total_inventory_value' => 20_500.00
                                                                              },
                                                                              'process_batch_005' => {
                                                                                'processed_count' => 200, 'total_inventory_value' => 14_750.50
                                                                              }
                                                                            })

      scenario = described_class.detect(
        dependency_results,
        'analyze_csv',
        'process_batch_'
      )

      # Aggregate results
      total_processed = scenario.batch_results.values.sum { |r| r['processed_count'] }
      total_value = scenario.batch_results.values.sum { |r| r['total_inventory_value'] }

      expect(scenario.with_batches?).to be true
      expect(scenario.worker_count).to eq(5)
      expect(total_processed).to eq(1000)
      expect(total_value).to eq(80_602.00)
    end
  end
end
