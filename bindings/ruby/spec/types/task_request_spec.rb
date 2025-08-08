# frozen_string_literal: true

require 'spec_helper'

RSpec.describe TaskerCore::Types::TaskTypes::TaskRequest do
  describe 'initialization with new distributed orchestration fields' do
    let(:basic_attributes) do
      {
        namespace: 'fulfillment',
        name: 'process_order',
        context: { order_id: 12_345 },
        initiator: 'test_user',
        source_system: 'test_suite',
        reason: 'testing new fields'
      }
    end

    context 'with default values' do
      subject { described_class.new(basic_attributes) }

      it 'sets priority to default value of 0' do
        expect(subject.priority).to eq(0)
      end

      it 'sets claim_timeout_seconds to default value of 60' do
        expect(subject.claim_timeout_seconds).to eq(60)
      end

      it 'creates a valid TaskRequest' do
        expect(subject).to be_valid_for_creation
      end
    end

    context 'with custom priority and claim timeout' do
      subject { described_class.new(custom_attributes) }

      let(:custom_attributes) do
        basic_attributes.merge(
          priority: 10,
          claim_timeout_seconds: 120
        )
      end

      it 'sets the custom priority value' do
        expect(subject.priority).to eq(10)
      end

      it 'sets the custom claim timeout value' do
        expect(subject.claim_timeout_seconds).to eq(120)
      end

      it 'maintains backward compatibility with validation' do
        expect(subject).to be_valid_for_creation
      end
    end

    context 'with edge case values' do
      it 'accepts zero priority' do
        task_request = described_class.new(basic_attributes.merge(priority: 0))
        expect(task_request.priority).to eq(0)
      end

      it 'accepts high priority values' do
        task_request = described_class.new(basic_attributes.merge(priority: 999))
        expect(task_request.priority).to eq(999)
      end

      it 'accepts minimal claim timeout' do
        task_request = described_class.new(basic_attributes.merge(claim_timeout_seconds: 1))
        expect(task_request.claim_timeout_seconds).to eq(1)
      end

      it 'accepts long claim timeout' do
        task_request = described_class.new(basic_attributes.merge(claim_timeout_seconds: 3600))
        expect(task_request.claim_timeout_seconds).to eq(3600)
      end
    end
  end

  describe '#to_ffi_hash for Rust integration' do
    let(:task_request) do
      described_class.new(
        namespace: 'fulfillment',
        name: 'process_order',
        context: { order_id: 12_345, customer_id: 67_890 },
        initiator: 'test_user',
        source_system: 'test_suite',
        reason: 'Phase 5.1 distributed orchestration testing',
        tags: %w[test phase_5_1],
        priority: 5,
        claim_timeout_seconds: 90,
        max_retries: 2,
        timeout_seconds: 300
      )
    end

    let(:ffi_hash) { task_request.to_ffi_hash }

    it 'includes priority as a direct field (not in options)' do
      expect(ffi_hash[:priority]).to eq(5)
      expect(ffi_hash[:options]).not_to include('priority') if ffi_hash[:options]
    end

    it 'includes claim_timeout_seconds as a direct field (not in options)' do
      expect(ffi_hash[:claim_timeout_seconds]).to eq(90)
      expect(ffi_hash[:options]).not_to include('claim_timeout_seconds') if ffi_hash[:options]
    end

    it 'maintains legacy fields in options hash' do
      expect(ffi_hash[:options]).to include('max_retries' => 2)
      expect(ffi_hash[:options]).to include('timeout_seconds' => 300)
    end

    it 'includes all required core fields' do
      expect(ffi_hash[:namespace]).to eq('fulfillment')
      expect(ffi_hash[:name]).to eq('process_order')
      expect(ffi_hash[:context]).to eq({ order_id: 12_345, customer_id: 67_890 })
      expect(ffi_hash[:initiator]).to eq('test_user')
      expect(ffi_hash[:source_system]).to eq('test_suite')
      expect(ffi_hash[:reason]).to eq('Phase 5.1 distributed orchestration testing')
    end

    it 'properly formats the requested_at timestamp' do
      expect(ffi_hash[:requested_at]).to match(/\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}/)
    end

    context 'with omitted priority and claim_timeout_seconds' do
      let(:task_request_with_defaults) do
        described_class.new(
          namespace: 'fulfillment',
          name: 'process_order',
          context: { order_id: 12_345 },
          initiator: 'test_user',
          source_system: 'test_suite',
          reason: 'testing default values'
          # priority and claim_timeout_seconds omitted - will use defaults
        )
      end

      it 'includes default values in FFI hash' do
        hash = task_request_with_defaults.to_ffi_hash
        expect(hash[:priority]).to eq(0) # Default priority
        expect(hash[:claim_timeout_seconds]).to eq(60) # Default claim timeout
      end
    end
  end

  describe '.build_test factory method' do
    it 'supports new priority and claim_timeout_seconds options' do
      task_request = described_class.build_test(
        namespace: 'inventory',
        name: 'stock_update',
        context: { product_id: 'ABC123' },
        priority: 7,
        claim_timeout_seconds: 45
      )

      expect(task_request.priority).to eq(7)
      expect(task_request.claim_timeout_seconds).to eq(45)
      expect(task_request.namespace).to eq('inventory')
      expect(task_request.name).to eq('stock_update')
    end
  end

  describe 'backward compatibility' do
    context 'existing code without new fields' do
      let(:legacy_task_request) do
        described_class.new(
          namespace: 'payments',
          name: 'charge_card',
          context: { amount: 99.99 },
          initiator: 'legacy_system',
          source_system: 'old_api',
          reason: 'backward compatibility test'
        )
      end

      it 'works without specifying priority or claim_timeout_seconds' do
        expect { legacy_task_request }.not_to raise_error
        expect(legacy_task_request).to be_valid_for_creation
      end

      it 'uses sensible defaults' do
        expect(legacy_task_request.priority).to eq(0)
        expect(legacy_task_request.claim_timeout_seconds).to eq(60)
      end

      it 'generates proper FFI hash' do
        hash = legacy_task_request.to_ffi_hash
        expect(hash[:priority]).to eq(0)
        expect(hash[:claim_timeout_seconds]).to eq(60)
        expect(hash[:namespace]).to eq('payments')
      end
    end
  end

  describe 'integration with orchestration architecture' do
    let(:high_priority_task) do
      described_class.new(
        namespace: 'fulfillment',
        name: 'urgent_order',
        context: { order_id: 99_999, urgent: true },
        initiator: 'customer_service',
        source_system: 'support_dashboard',
        reason: 'customer escalation',
        priority: 100, # High priority for urgent processing
        claim_timeout_seconds: 30 # Short timeout for quick recovery
      )
    end

    let(:standard_task) do
      described_class.new(
        namespace: 'fulfillment',
        name: 'regular_order',
        context: { order_id: 88_888 },
        initiator: 'automated_system',
        source_system: 'order_api',
        reason: 'standard processing'
        # Uses defaults: priority=0, claim_timeout_seconds=60
      )
    end

    it 'supports priority-based orchestration scenarios' do
      expect(high_priority_task.priority).to be > standard_task.priority
      expect(high_priority_task.claim_timeout_seconds).to be < standard_task.claim_timeout_seconds
    end

    it 'generates correct FFI data for distributed orchestration' do
      urgent_hash = high_priority_task.to_ffi_hash
      standard_hash = standard_task.to_ffi_hash

      # Verify urgent task has higher priority
      expect(urgent_hash[:priority]).to eq(100)
      expect(standard_hash[:priority]).to eq(0)

      # Verify urgent task has shorter claim timeout
      expect(urgent_hash[:claim_timeout_seconds]).to eq(30)
      expect(standard_hash[:claim_timeout_seconds]).to eq(60)

      # Both should have proper core data
      expect(urgent_hash[:namespace]).to eq('fulfillment')
      expect(standard_hash[:namespace]).to eq('fulfillment')
    end
  end

  describe 'string representation' do
    let(:task_request) do
      described_class.new(
        namespace: 'inventory',
        name: 'restock_alert',
        context: { product_id: 'SKU123' },
        initiator: 'inventory_system',
        source_system: 'warehouse_api',
        reason: 'low stock detected',
        priority: 3,
        claim_timeout_seconds: 75
      )
    end

    it 'includes basic information in string representation' do
      str = task_request.to_s
      expect(str).to include('inventory/restock_alert')
      expect(str).to include('inventory_system')
    end

    it 'provides detailed inspect output' do
      expect(task_request.inspect).to eq(task_request.to_s)
    end
  end
end
