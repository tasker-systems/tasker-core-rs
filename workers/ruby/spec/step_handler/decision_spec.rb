# frozen_string_literal: true

require 'spec_helper'

RSpec.describe TaskerCore::StepHandler::Decision do
  # Test implementation of Decision handler
  class TestDecisionHandler < described_class
    def call(_task, _sequence, _step)
      decision_success(
        steps: ['test_step'],
        result_data: { test: 'data' }
      )
    end
  end

  let(:handler) { TestDecisionHandler.new }

  describe '#capabilities' do
    it 'includes decision-specific capabilities' do
      capabilities = handler.capabilities

      expect(capabilities).to include('decision_point')
      expect(capabilities).to include('dynamic_workflow')
      expect(capabilities).to include('step_creation')
    end

    it 'includes base handler capabilities' do
      capabilities = handler.capabilities

      expect(capabilities).to include('process')
    end
  end

  describe '#config_schema' do
    it 'includes decision-specific schema properties' do
      schema = handler.config_schema

      expect(schema[:properties]).to have_key(:decision_thresholds)
      expect(schema[:properties]).to have_key(:decision_metadata)
    end

    it 'merges with base handler schema' do
      schema = handler.config_schema

      expect(schema).to have_key(:type)
      expect(schema).to have_key(:properties)
    end
  end

  describe '#decision_success' do
    context 'with single step' do
      it 'creates success result with CreateSteps outcome' do
        result = handler.decision_success(
          steps: 'single_step',
          result_data: { route_type: 'auto' }
        )

        expect(result).to be_a(TaskerCore::Types::StepHandlerCallResult::Success)
        expect(result.success).to be true
        expect(result.result[:route_type]).to eq('auto')
        expect(result.result[:decision_point_outcome]).to be_a(Hash)
        expect(result.result[:decision_point_outcome][:type]).to eq('create_steps')
        expect(result.result[:decision_point_outcome][:step_names]).to eq(['single_step'])
      end

      it 'includes decision metadata' do
        result = handler.decision_success(
          steps: 'test_step',
          result_data: {}
        )

        expect(result.metadata[:decision_point]).to be true
        expect(result.metadata[:outcome_type]).to eq('create_steps')
        expect(result.metadata[:branches_created]).to eq(1)
        expect(result.metadata[:processed_by]).to eq('test_decision_handler')
      end
    end

    context 'with multiple steps' do
      it 'creates result with all steps in outcome' do
        steps = %w[manager_approval finance_review compliance_check]
        result = handler.decision_success(
          steps: steps,
          result_data: { route_type: 'complex_approval' }
        )

        expect(result.success).to be true
        expect(result.result[:decision_point_outcome][:step_names]).to eq(steps)
        expect(result.metadata[:branches_created]).to eq(3)
      end
    end

    context 'with array of steps' do
      it 'handles array input correctly' do
        result = handler.decision_success(
          steps: %w[step1 step2],
          result_data: { test: 'data' }
        )

        expect(result.result[:decision_point_outcome][:step_names]).to eq(%w[step1 step2])
      end
    end

    context 'with custom metadata' do
      it 'merges custom metadata with decision metadata' do
        result = handler.decision_success(
          steps: 'test',
          result_data: {},
          metadata: { custom_field: 'value', operation: 'routing' }
        )

        expect(result.metadata[:custom_field]).to eq('value')
        expect(result.metadata[:operation]).to eq('routing')
        expect(result.metadata[:decision_point]).to be true
        expect(result.metadata[:processed_at]).to be_a(String)
      end
    end

    context 'with invalid steps' do
      it 'raises error for empty array' do
        expect do
          handler.decision_success(steps: [], result_data: {})
        end.to raise_error(TaskerCore::Errors::PermanentError, /step_names must be non-empty array/)
      end

      it 'raises error for non-string step name' do
        expect do
          handler.decision_success(steps: [123], result_data: {})
        end.to raise_error(TaskerCore::Errors::PermanentError)
      end

      it 'raises error for empty string step name' do
        expect do
          handler.decision_success(steps: [''], result_data: {})
        end.to raise_error(TaskerCore::Errors::PermanentError, /All step names must be non-empty strings/)
      end

      it 'raises error for mixed valid/invalid step names' do
        expect do
          handler.decision_success(steps: ['valid', 123, 'also_valid'], result_data: {})
        end.to raise_error(TaskerCore::Errors::PermanentError)
      end
    end
  end

  describe '#decision_no_branches' do
    it 'creates success result with NoBranches outcome' do
      result = handler.decision_no_branches(
        result_data: { reason: 'amount_below_threshold', amount: 50 }
      )

      expect(result).to be_a(TaskerCore::Types::StepHandlerCallResult::Success)
      expect(result.success).to be true
      expect(result.result[:reason]).to eq('amount_below_threshold')
      expect(result.result[:amount]).to eq(50)
      expect(result.result[:decision_point_outcome]).to be_a(Hash)
      expect(result.result[:decision_point_outcome][:type]).to eq('no_branches')
    end

    it 'includes decision metadata' do
      result = handler.decision_no_branches(result_data: {})

      expect(result.metadata[:decision_point]).to be true
      expect(result.metadata[:outcome_type]).to eq('no_branches')
      expect(result.metadata[:branches_created]).to eq(0)
      expect(result.metadata[:processed_by]).to eq('test_decision_handler')
      expect(result.metadata[:processed_at]).to be_a(String)
    end

    it 'merges custom metadata' do
      result = handler.decision_no_branches(
        result_data: {},
        metadata: { skip_reason: 'below_threshold' }
      )

      expect(result.metadata[:skip_reason]).to eq('below_threshold')
      expect(result.metadata[:decision_point]).to be true
    end
  end

  describe '#validate_decision_outcome!' do
    context 'with valid CreateSteps outcome' do
      it 'validates hash with symbol keys' do
        outcome = { outcome_type: 'CreateSteps', step_names: ['test'] }

        expect do
          handler.send(:validate_decision_outcome!, outcome)
        end.not_to raise_error
      end

      it 'validates hash with string keys' do
        outcome = { 'outcome_type' => 'CreateSteps', 'step_names' => ['test'] }

        expect do
          handler.send(:validate_decision_outcome!, outcome)
        end.not_to raise_error
      end

      it 'validates DecisionPointOutcome object' do
        outcome = TaskerCore::Types::DecisionPointOutcome.create_steps(['test'])

        expect do
          handler.send(:validate_decision_outcome!, outcome)
        end.not_to raise_error
      end
    end

    context 'with valid NoBranches outcome' do
      it 'validates hash with outcome_type NoBranches' do
        outcome = { outcome_type: 'NoBranches' }

        expect do
          handler.send(:validate_decision_outcome!, outcome)
        end.not_to raise_error
      end

      it 'validates DecisionPointOutcome object' do
        outcome = TaskerCore::Types::DecisionPointOutcome.no_branches

        expect do
          handler.send(:validate_decision_outcome!, outcome)
        end.not_to raise_error
      end
    end

    context 'with invalid outcomes' do
      it 'raises error for non-hash/non-outcome object' do
        expect do
          handler.send(:validate_decision_outcome!, 'invalid')
        end.to raise_error(TaskerCore::Errors::PermanentError, /Outcome must be Hash or DecisionPointOutcome/)
      end

      it 'raises error for invalid outcome_type' do
        outcome = { outcome_type: 'InvalidType' }

        expect do
          handler.send(:validate_decision_outcome!, outcome)
        end.to raise_error(TaskerCore::Errors::PermanentError, /Invalid outcome_type: InvalidType/)
      end

      it 'raises error for CreateSteps without step_names' do
        outcome = { outcome_type: 'CreateSteps' }

        expect do
          handler.send(:validate_decision_outcome!, outcome)
        end.to raise_error(TaskerCore::Errors::PermanentError)
      end

      it 'raises error for CreateSteps with empty step_names' do
        outcome = { outcome_type: 'CreateSteps', step_names: [] }

        expect do
          handler.send(:validate_decision_outcome!, outcome)
        end.to raise_error(TaskerCore::Errors::PermanentError, /step_names must be non-empty array/)
      end
    end
  end

  describe '#decision_with_custom_outcome' do
    it 'creates result with custom CreateSteps outcome' do
      custom_outcome = TaskerCore::Types::DecisionPointOutcome.create_steps(['custom_step'])

      result = handler.decision_with_custom_outcome(
        outcome: custom_outcome,
        result_data: { custom: 'data' }
      )

      expect(result.success).to be true
      expect(result.result[:custom]).to eq('data')
      expect(result.result[:decision_point_outcome][:type]).to eq('create_steps')
      expect(result.result[:decision_point_outcome][:step_names]).to eq(['custom_step'])
    end

    it 'creates result with custom NoBranches outcome' do
      custom_outcome = TaskerCore::Types::DecisionPointOutcome.no_branches

      result = handler.decision_with_custom_outcome(
        outcome: custom_outcome,
        result_data: { reason: 'custom_logic' }
      )

      expect(result.success).to be true
      expect(result.result[:reason]).to eq('custom_logic')
      expect(result.result[:decision_point_outcome][:type]).to eq('no_branches')
    end

    it 'validates the custom outcome' do
      invalid_outcome = { outcome_type: 'InvalidType' }

      expect do
        handler.decision_with_custom_outcome(outcome: invalid_outcome, result_data: {})
      end.to raise_error(TaskerCore::Errors::PermanentError, /Invalid outcome_type/)
    end

    it 'includes decision metadata' do
      outcome = TaskerCore::Types::DecisionPointOutcome.create_steps(['test'])

      result = handler.decision_with_custom_outcome(
        outcome: outcome,
        result_data: {},
        metadata: { custom_meta: 'value' }
      )

      expect(result.metadata[:custom_meta]).to eq('value')
      expect(result.metadata[:decision_point]).to be true
      expect(result.metadata[:outcome_type]).to eq('create_steps')
    end
  end

  describe 'integration with StepHandlerCallResult' do
    it 'creates properly structured success results' do
      result = handler.decision_success(
        steps: ['test_step'],
        result_data: { amount: 1000 }
      )

      expect(result).to respond_to(:success)
      expect(result).to respond_to(:result)
      expect(result).to respond_to(:metadata)
      expect(result.success).to be true
    end

    it 'embeds decision_point_outcome in result hash' do
      result = handler.decision_success(
        steps: ['test_step'],
        result_data: { amount: 1000 }
      )

      expect(result.result).to have_key(:decision_point_outcome)
      expect(result.result).to have_key(:amount)
    end

    it 'preserves all result_data fields' do
      result = handler.decision_success(
        steps: ['test'],
        result_data: { field1: 'value1', field2: 'value2', field3: { nested: 'data' } }
      )

      expect(result.result[:field1]).to eq('value1')
      expect(result.result[:field2]).to eq('value2')
      expect(result.result[:field3]).to eq({ nested: 'data' })
      expect(result.result[:decision_point_outcome]).to be_a(Hash)
    end
  end

  describe 'Rust FFI serialization compatibility' do
    it 'serializes CreateSteps with snake_case fields' do
      result = handler.decision_success(
        steps: %w[manager_approval finance_review],
        result_data: { route_type: 'dual_approval' }
      )

      outcome_hash = result.result[:decision_point_outcome]

      # Rust expects snake_case due to #[serde(rename_all = "snake_case")]
      expect(outcome_hash[:type]).to eq('create_steps')
      expect(outcome_hash[:step_names]).to be_a(Array)
      expect(outcome_hash.keys).not_to include(:stepNames) # Not camelCase
    end

    it 'serializes NoBranches with correct structure' do
      result = handler.decision_no_branches(result_data: {})

      outcome_hash = result.result[:decision_point_outcome]

      expect(outcome_hash[:type]).to eq('no_branches')
      expect(outcome_hash.keys).to eq([:type])
    end
  end

  describe 'real-world decision logic patterns' do
    class AmountBasedRoutingHandler < described_class
      SMALL_THRESHOLD = 1000
      LARGE_THRESHOLD = 5000

      def call(task, _sequence, _step)
        amount = task.context['amount']

        if amount < SMALL_THRESHOLD
          decision_success(
            steps: ['auto_approve'],
            result_data: { route_type: 'auto', amount: amount }
          )
        elsif amount < LARGE_THRESHOLD
          decision_success(
            steps: ['manager_approval'],
            result_data: { route_type: 'manager_only', amount: amount }
          )
        else
          decision_success(
            steps: %w[manager_approval finance_review],
            result_data: { route_type: 'dual_approval', amount: amount }
          )
        end
      end
    end

    let(:routing_handler) { AmountBasedRoutingHandler.new }
    let(:task_wrapper) { double('task', context: { 'amount' => amount }) }
    let(:sequence) { double('sequence') }
    let(:step) { double('step') }

    context 'with small amount' do
      let(:amount) { 500 }

      it 'routes to auto_approve' do
        result = routing_handler.call(task_wrapper, sequence, step)

        expect(result.result[:route_type]).to eq('auto')
        expect(result.result[:decision_point_outcome][:step_names]).to eq(['auto_approve'])
      end
    end

    context 'with medium amount' do
      let(:amount) { 3000 }

      it 'routes to manager_approval only' do
        result = routing_handler.call(task_wrapper, sequence, step)

        expect(result.result[:route_type]).to eq('manager_only')
        expect(result.result[:decision_point_outcome][:step_names]).to eq(['manager_approval'])
      end
    end

    context 'with large amount' do
      let(:amount) { 10_000 }

      it 'routes to dual approval path' do
        result = routing_handler.call(task_wrapper, sequence, step)

        expect(result.result[:route_type]).to eq('dual_approval')
        expect(result.result[:decision_point_outcome][:step_names]).to eq(%w[manager_approval finance_review])
        expect(result.metadata[:branches_created]).to eq(2)
      end
    end
  end
end
