# frozen_string_literal: true

require 'spec_helper'

RSpec.describe TaskerCore::DomainEvents::PublisherRegistry do
  let(:registry) { described_class.instance }

  before do
    registry.reset!
  end

  describe '#register' do
    it 'registers a publisher' do
      publisher = TaskerCore::TestHelpers::MockEventPublisher.new(name: 'TestPublisher')
      registry.register(publisher)

      expect(registry.get('TestPublisher')).to eq(publisher)
    end

    it 'raises error if publisher does not inherit from BasePublisher' do
      not_a_publisher = Object.new

      expect { registry.register(not_a_publisher) }.to raise_error(ArgumentError)
    end

    it 'raises error when registering duplicate publisher name' do
      publisher1 = TaskerCore::TestHelpers::MockEventPublisher.new(name: 'DuplicateName')
      publisher2 = TaskerCore::TestHelpers::MockEventPublisher.new(name: 'DuplicateName')

      registry.register(publisher1)
      expect { registry.register(publisher2) }.to raise_error(TaskerCore::DomainEvents::PublisherRegistry::DuplicatePublisherError)
    end

    it 'prevents registration after freeze' do
      registry.freeze!
      publisher = TaskerCore::TestHelpers::MockEventPublisher.new(name: 'LatePublisher')

      expect { registry.register(publisher) }.to raise_error(TaskerCore::DomainEvents::PublisherRegistry::RegistryFrozenError)
    end
  end

  describe '#get' do
    it 'returns nil for unknown publisher' do
      expect(registry.get('NonExistent')).to be_nil
    end

    it 'returns registered publisher' do
      publisher = TaskerCore::TestHelpers::MockEventPublisher.new(name: 'MyPublisher')
      registry.register(publisher)

      expect(registry.get('MyPublisher')).to eq(publisher)
    end
  end

  describe '#get_or_default' do
    it 'returns registered publisher if exists' do
      publisher = TaskerCore::TestHelpers::MockEventPublisher.new(name: 'Custom')
      registry.register(publisher)

      expect(registry.get_or_default('Custom')).to eq(publisher)
    end

    it 'returns default publisher if not found' do
      result = registry.get_or_default('NonExistent')

      expect(result).to be_a(TaskerCore::DomainEvents::PublisherRegistry::DefaultPublisher)
    end
  end

  describe '#get_strict' do
    it 'returns registered publisher' do
      publisher = TaskerCore::TestHelpers::MockEventPublisher.new(name: 'StrictTest')
      registry.register(publisher)

      expect(registry.get_strict('StrictTest')).to eq(publisher)
    end

    it 'raises error for unknown publisher' do
      expect { registry.get_strict('Unknown') }.to raise_error(TaskerCore::DomainEvents::PublisherRegistry::PublisherNotFoundError)
    end
  end

  describe '#validate_required!' do
    it 'succeeds when all publishers are registered' do
      registry.register(TaskerCore::TestHelpers::MockEventPublisher.new(name: 'Publisher1'))
      registry.register(TaskerCore::TestHelpers::MockEventPublisher.new(name: 'Publisher2'))

      expect { registry.validate_required!(%w[Publisher1 Publisher2]) }.not_to raise_error
    end

    it 'raises error listing missing publishers' do
      registry.register(TaskerCore::TestHelpers::MockEventPublisher.new(name: 'OnlyOne'))

      expect { registry.validate_required!(%w[OnlyOne Missing1 Missing2]) }
        .to raise_error(TaskerCore::DomainEvents::PublisherRegistry::ValidationError) do |error|
          expect(error.message).to include('Missing1')
          expect(error.message).to include('Missing2')
        end
    end
  end

  describe '#registered_names' do
    it 'returns list of registered publisher names' do
      registry.register(TaskerCore::TestHelpers::MockEventPublisher.new(name: 'First'))
      registry.register(TaskerCore::TestHelpers::MockEventPublisher.new(name: 'Second'))

      expect(registry.registered_names).to contain_exactly('First', 'Second')
    end
  end
end
