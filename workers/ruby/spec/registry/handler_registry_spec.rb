# frozen_string_literal: true

require 'spec_helper'

# TAS-93: HandlerRegistry with ResolverChain Integration Tests
#
# Tests the integration of ResolverChain into HandlerRegistry,
# verifying the production dispatch path uses the resolver chain
# for handler resolution with method dispatch support.

RSpec.describe TaskerCore::Registry::HandlerRegistry do
  # Test handler classes
  before(:all) do
    module HandlerRegistryTestHandlers
      class PaymentHandler < TaskerCore::StepHandler::Base
        attr_reader :config

        def initialize(config: {})
          super()
          @config = config
        end

        def call(_context)
          { status: :success, handler: 'PaymentHandler', method: :call }
        end

        def process(_context)
          { status: :success, handler: 'PaymentHandler', method: :process }
        end

        def refund(_context)
          { status: :success, handler: 'PaymentHandler', method: :refund }
        end
      end

      class InventoryHandler < TaskerCore::StepHandler::Base
        def call(_context)
          { status: :success, handler: 'InventoryHandler' }
        end
      end

      class ConfigurableHandler < TaskerCore::StepHandler::Base
        attr_reader :config

        def initialize(config: {})
          super()
          @config = config
        end

        def call(_context)
          { status: :success, config: @config }
        end
      end
    end
  end

  # Access the singleton instance
  let(:registry) { described_class.instance }

  # Reset resolver chain between tests for isolation
  before do
    # Re-initialize the resolver chain to ensure clean state
    registry.instance_variable_set(:@resolver_chain, TaskerCore::Registry::ResolverChain.default)
  end

  describe '#resolve_handler with string callable' do
    before do
      registry.register_handler(
        'HandlerRegistryTestHandlers::PaymentHandler',
        HandlerRegistryTestHandlers::PaymentHandler
      )
    end

    it 'resolves handler by class name string' do
      handler = registry.resolve_handler('HandlerRegistryTestHandlers::PaymentHandler')

      expect(handler).to be_a(HandlerRegistryTestHandlers::PaymentHandler)
    end

    it 'returns nil for unknown handler' do
      handler = registry.resolve_handler('NonExistent::Handler')

      expect(handler).to be_nil
    end

    it 'handler responds to call' do
      handler = registry.resolve_handler('HandlerRegistryTestHandlers::PaymentHandler')

      expect(handler).to respond_to(:call)
    end
  end

  describe '#resolve_handler with HandlerDefinition' do
    before do
      registry.register_handler(
        'HandlerRegistryTestHandlers::PaymentHandler',
        HandlerRegistryTestHandlers::PaymentHandler
      )
    end

    context 'without method dispatch' do
      let(:definition) do
        TaskerCore::Types::HandlerDefinition.new(
          callable: 'HandlerRegistryTestHandlers::PaymentHandler'
        )
      end

      it 'resolves handler from definition' do
        handler = registry.resolve_handler(definition)

        expect(handler).to be_a(HandlerRegistryTestHandlers::PaymentHandler)
      end

      it 'handler is not wrapped' do
        handler = registry.resolve_handler(definition)

        expect(handler).not_to be_a(TaskerCore::Registry::Resolvers::MethodDispatchWrapper)
      end
    end

    context 'with method dispatch' do
      let(:definition) do
        TaskerCore::Types::HandlerDefinition.new(
          callable: 'HandlerRegistryTestHandlers::PaymentHandler',
          handler_method: 'refund'
        )
      end

      it 'wraps handler for method dispatch' do
        handler = registry.resolve_handler(definition)

        expect(handler).to be_a(TaskerCore::Registry::Resolvers::MethodDispatchWrapper)
      end

      it 'call() invokes the specified method' do
        handler = registry.resolve_handler(definition)
        result = handler.call({})

        expect(result[:method]).to eq(:refund)
      end

      it 'unwrap returns the underlying handler' do
        handler = registry.resolve_handler(definition)

        expect(handler.unwrap).to be_a(HandlerRegistryTestHandlers::PaymentHandler)
      end
    end

    context 'with initialization config' do
      let(:definition) do
        TaskerCore::Types::HandlerDefinition.new(
          callable: 'HandlerRegistryTestHandlers::ConfigurableHandler',
          initialization: { api_key: 'secret', timeout: 30 }
        )
      end

      before do
        registry.register_handler(
          'HandlerRegistryTestHandlers::ConfigurableHandler',
          HandlerRegistryTestHandlers::ConfigurableHandler
        )
      end

      it 'passes config to handler' do
        handler = registry.resolve_handler(definition)

        expect(handler.config).to eq(api_key: 'secret', timeout: 30)
      end
    end
  end

  describe '#resolve_handler with HandlerWrapper (FFI)' do
    before do
      registry.register_handler(
        'HandlerRegistryTestHandlers::PaymentHandler',
        HandlerRegistryTestHandlers::PaymentHandler
      )
    end

    let(:handler_wrapper) do
      TaskerCore::Models::HandlerWrapper.new(
        callable: 'HandlerRegistryTestHandlers::PaymentHandler',
        initialization: { source: 'ffi' }
      )
    end

    it 'resolves from HandlerWrapper' do
      handler = registry.resolve_handler(handler_wrapper)

      expect(handler).to be_a(HandlerRegistryTestHandlers::PaymentHandler)
    end

    it 'passes initialization from wrapper' do
      registry.register_handler(
        'HandlerRegistryTestHandlers::ConfigurableHandler',
        HandlerRegistryTestHandlers::ConfigurableHandler
      )

      wrapper = TaskerCore::Models::HandlerWrapper.new(
        callable: 'HandlerRegistryTestHandlers::ConfigurableHandler',
        initialization: { from_ffi: true }
      )

      handler = registry.resolve_handler(wrapper)

      # HandlerWrapper uses HashWithIndifferentAccess, so keys may be strings
      expect(handler.config[:from_ffi] || handler.config['from_ffi']).to be true
    end

    context 'with TAS-93 method dispatch from FFI' do
      let(:wrapper_with_method) do
        # Simulates data from Rust FFI with 'method' field
        TaskerCore::Models::HandlerWrapper.new(
          callable: 'HandlerRegistryTestHandlers::PaymentHandler',
          initialization: {},
          method: 'refund'
        )
      end

      it 'wraps handler for method dispatch when method is specified' do
        handler = registry.resolve_handler(wrapper_with_method)

        expect(handler).to be_a(TaskerCore::Registry::Resolvers::MethodDispatchWrapper)
      end

      it 'invokes the specified method via call()' do
        handler = registry.resolve_handler(wrapper_with_method)
        result = handler.call({})

        expect(result[:method]).to eq(:refund)
      end

      it 'exposes handler_method attribute' do
        expect(wrapper_with_method.handler_method).to eq('refund')
      end
    end

    context 'with TAS-93 resolver hint from FFI' do
      let(:wrapper_with_resolver) do
        TaskerCore::Models::HandlerWrapper.new(
          callable: 'HandlerRegistryTestHandlers::PaymentHandler',
          initialization: {},
          resolver: 'explicit'
        )
      end

      it 'exposes resolver attribute' do
        expect(wrapper_with_resolver.resolver).to eq('explicit')
      end
    end
  end

  describe '#register_handler' do
    it 'registers in both resolver chain and legacy hash' do
      registry.register_handler('test.handler', HandlerRegistryTestHandlers::InventoryHandler)

      # Should be in legacy hash
      expect(registry.handlers).to have_key('test.handler')

      # Should be resolvable via chain
      expect(registry.handler_available?('test.handler')).to be true
    end
  end

  describe '#handler_available?' do
    it 'returns true for registered handlers' do
      registry.register_handler('available.handler', HandlerRegistryTestHandlers::InventoryHandler)

      expect(registry.handler_available?('available.handler')).to be true
    end

    it 'returns false for unknown handlers' do
      expect(registry.handler_available?('unknown.handler')).to be false
    end
  end

  describe '#registered_handlers' do
    it 'includes handlers from both chain and legacy hash' do
      registry.register_handler('chain.handler', HandlerRegistryTestHandlers::InventoryHandler)

      expect(registry.registered_handlers).to include('chain.handler')
    end

    it 'returns sorted unique list' do
      registry.register_handler('z.handler', HandlerRegistryTestHandlers::InventoryHandler)
      registry.register_handler('a.handler', HandlerRegistryTestHandlers::InventoryHandler)

      handlers = registry.registered_handlers
      z_index = handlers.index('z.handler')
      a_index = handlers.index('a.handler')

      expect(a_index).to be < z_index
    end
  end

  describe '#add_resolver' do
    it 'adds custom resolver to the chain' do
      custom_resolver = TaskerCore::Registry::Resolvers::ExplicitMappingResolver.new('custom_test')
      custom_resolver.register('custom.handler', HandlerRegistryTestHandlers::InventoryHandler)

      registry.add_resolver(custom_resolver)

      expect(registry.resolver_chain.resolver_names).to include('custom_test')
    end

    it 'custom resolver can resolve handlers' do
      custom_resolver = TaskerCore::Registry::Resolvers::ExplicitMappingResolver.new('payment_resolver')
      custom_resolver.register('payment.custom', HandlerRegistryTestHandlers::PaymentHandler)

      registry.add_resolver(custom_resolver)

      handler = registry.resolve_handler('payment.custom')
      expect(handler).to be_a(HandlerRegistryTestHandlers::PaymentHandler)
    end
  end

  describe 'cross-language aliases' do
    before do
      registry.register_handler('alias.test', HandlerRegistryTestHandlers::InventoryHandler)
    end

    it 'register is alias for register_handler' do
      registry.register('alias.register', HandlerRegistryTestHandlers::InventoryHandler)
      expect(registry.handler_available?('alias.register')).to be true
    end

    it 'resolve is alias for resolve_handler' do
      handler = registry.resolve('alias.test')
      expect(handler).to be_a(HandlerRegistryTestHandlers::InventoryHandler)
    end

    it 'is_registered is alias for handler_available?' do
      expect(registry.is_registered('alias.test')).to be true
    end

    it 'list_handlers is alias for registered_handlers' do
      expect(registry.list_handlers).to include('alias.test')
    end
  end

  describe 'resolver chain integration' do
    it 'explicit registration takes priority over class constant' do
      # Register a different class under PaymentHandler name
      registry.register_handler(
        'HandlerRegistryTestHandlers::PaymentHandler',
        HandlerRegistryTestHandlers::InventoryHandler # Different class!
      )

      handler = registry.resolve_handler('HandlerRegistryTestHandlers::PaymentHandler')

      # Should get InventoryHandler because explicit registration wins
      expect(handler).to be_a(HandlerRegistryTestHandlers::InventoryHandler)
    end

    it 'class constant resolution works for unregistered classes' do
      # Don't register - let class constant resolver find it
      handler = registry.resolve_handler('HandlerRegistryTestHandlers::InventoryHandler')

      expect(handler).to be_a(HandlerRegistryTestHandlers::InventoryHandler)
    end
  end

  describe 'thread safety' do
    it 'concurrent registration is safe' do
      threads = 10.times.map do |i|
        Thread.new do
          registry.register_handler("concurrent.handler.#{i}", HandlerRegistryTestHandlers::InventoryHandler)
        end
      end

      threads.each(&:join)

      # All registrations should succeed
      count = registry.registered_handlers.count { |h| h.start_with?('concurrent.handler.') }
      expect(count).to eq(10)
    end
  end
end
