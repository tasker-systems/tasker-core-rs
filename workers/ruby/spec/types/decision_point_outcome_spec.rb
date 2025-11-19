# frozen_string_literal: true

require 'spec_helper'

RSpec.describe TaskerCore::Types::DecisionPointOutcome do
  describe '.no_branches' do
    it 'creates a NoBranches outcome' do
      outcome = described_class.no_branches

      expect(outcome).to be_a(TaskerCore::Types::DecisionPointOutcome::NoBranches)
      expect(outcome.type).to eq('no_branches')
      expect(outcome.requires_step_creation?).to be false
      expect(outcome.step_names).to eq([])
    end

    it 'serializes correctly for Rust with snake_case' do
      outcome = described_class.no_branches
      hash = outcome.to_h

      expect(hash).to eq({ type: 'no_branches' })
    end
  end

  describe '.create_steps' do
    context 'with single step' do
      it 'creates a CreateSteps outcome' do
        outcome = described_class.create_steps(['auto_approve'])

        expect(outcome).to be_a(TaskerCore::Types::DecisionPointOutcome::CreateSteps)
        expect(outcome.type).to eq('create_steps')
        expect(outcome.step_names).to eq(['auto_approve'])
        expect(outcome.requires_step_creation?).to be true
      end

      it 'serializes correctly for Rust with snake_case' do
        outcome = described_class.create_steps(['auto_approve'])
        hash = outcome.to_h

        expect(hash).to eq({
                             type: 'create_steps',
                             step_names: ['auto_approve']
                           })
      end
    end

    context 'with multiple steps' do
      it 'creates CreateSteps outcome with all step names' do
        steps = %w[manager_approval finance_review compliance_check]
        outcome = described_class.create_steps(steps)

        expect(outcome.type).to eq('create_steps')
        expect(outcome.step_names).to eq(steps)
        expect(outcome.requires_step_creation?).to be true
      end

      it 'serializes all step names correctly' do
        steps = %w[manager_approval finance_review]
        outcome = described_class.create_steps(steps)
        hash = outcome.to_h

        expect(hash[:step_names]).to eq(steps)
        expect(hash[:step_names].size).to eq(2)
      end
    end

    context 'with invalid input' do
      it 'raises ArgumentError for nil step_names' do
        expect do
          described_class.create_steps(nil)
        end.to raise_error(ArgumentError, 'step_names cannot be empty')
      end

      it 'raises ArgumentError for empty array' do
        expect do
          described_class.create_steps([])
        end.to raise_error(ArgumentError, 'step_names cannot be empty')
      end

      it 'raises Dry::Struct::Error for non-string step names' do
        expect do
          described_class.create_steps([123, 456])
        end.to raise_error(Dry::Struct::Error)
      end
    end
  end

  describe '.from_hash' do
    context 'with no_branches outcome' do
      it 'parses hash with symbol keys' do
        hash = { type: 'no_branches' }
        outcome = described_class.from_hash(hash)

        expect(outcome).to be_a(TaskerCore::Types::DecisionPointOutcome::NoBranches)
        expect(outcome.type).to eq('no_branches')
      end

      it 'parses hash with string keys (from Rust)' do
        hash = { 'type' => 'no_branches' }
        outcome = described_class.from_hash(hash)

        expect(outcome).to be_a(TaskerCore::Types::DecisionPointOutcome::NoBranches)
        expect(outcome.type).to eq('no_branches')
      end
    end

    context 'with create_steps outcome' do
      it 'parses hash with symbol keys' do
        hash = {
          type: 'create_steps',
          step_names: %w[approval_required notify_manager]
        }
        outcome = described_class.from_hash(hash)

        expect(outcome).to be_a(TaskerCore::Types::DecisionPointOutcome::CreateSteps)
        expect(outcome.type).to eq('create_steps')
        expect(outcome.step_names).to eq(%w[approval_required notify_manager])
      end

      it 'parses hash with string keys (from Rust)' do
        hash = {
          'type' => 'create_steps',
          'step_names' => ['manager_approval']
        }
        outcome = described_class.from_hash(hash)

        expect(outcome).to be_a(TaskerCore::Types::DecisionPointOutcome::CreateSteps)
        expect(outcome.step_names).to eq(['manager_approval'])
      end
    end

    context 'with invalid input' do
      it 'returns nil for non-hash input' do
        expect(described_class.from_hash('invalid')).to be_nil
        expect(described_class.from_hash(123)).to be_nil
        expect(described_class.from_hash(nil)).to be_nil
      end

      it 'returns nil for hash with invalid type' do
        hash = { type: 'unknown_type' }
        expect(described_class.from_hash(hash)).to be_nil
      end

      it 'returns nil for create_steps without step_names' do
        hash = { type: 'create_steps' }
        expect(described_class.from_hash(hash)).to be_nil
      end

      it 'returns nil for create_steps with empty step_names' do
        hash = { type: 'create_steps', step_names: [] }
        expect(described_class.from_hash(hash)).to be_nil
      end

      it 'returns nil for create_steps with non-array step_names' do
        hash = { type: 'create_steps', step_names: 'not_an_array' }
        expect(described_class.from_hash(hash)).to be_nil
      end
    end
  end

  describe 'type discriminator consistency' do
    it 'maintains consistent type field across NoBranches operations' do
      outcome = described_class.no_branches

      expect(outcome.type).to eq('no_branches')
      expect(outcome.to_h[:type]).to eq('no_branches')
    end

    it 'maintains consistent type field across CreateSteps operations' do
      outcome = described_class.create_steps(['test_step'])

      expect(outcome.type).to eq('create_steps')
      expect(outcome.to_h[:type]).to eq('create_steps')
    end
  end

  describe 'round-trip serialization' do
    it 'successfully round-trips NoBranches outcome' do
      original = described_class.no_branches
      hash = original.to_h
      parsed = described_class.from_hash(hash)

      expect(parsed.type).to eq(original.type)
      expect(parsed.requires_step_creation?).to eq(original.requires_step_creation?)
      expect(parsed.step_names).to eq(original.step_names)
    end

    it 'successfully round-trips CreateSteps outcome' do
      steps = %w[step1 step2 step3]
      original = described_class.create_steps(steps)
      hash = original.to_h
      parsed = described_class.from_hash(hash)

      expect(parsed.type).to eq(original.type)
      expect(parsed.step_names).to eq(original.step_names)
      expect(parsed.requires_step_creation?).to eq(original.requires_step_creation?)
    end
  end

  describe 'Rust FFI integration expectations' do
    it 'uses snake_case for type discriminator' do
      no_branches = described_class.no_branches
      create_steps = described_class.create_steps(['test'])

      expect(no_branches.to_h[:type]).to eq('no_branches')
      expect(create_steps.to_h[:type]).to eq('create_steps')
    end

    it 'uses snake_case for field names' do
      outcome = described_class.create_steps(['test_step'])
      hash = outcome.to_h

      # Rust expects snake_case due to #[serde(rename_all = "snake_case")]
      expect(hash.keys).to include(:type)
      expect(hash.keys).to include(:step_names)
      expect(hash.keys).not_to include(:stepNames) # Not camelCase
      expect(hash.keys).not_to include(:Type) # Not PascalCase
    end
  end
end
