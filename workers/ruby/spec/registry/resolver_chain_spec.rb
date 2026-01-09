# frozen_string_literal: true

require 'spec_helper'

# TAS-93: Resolver Chain Unit Tests
#
# These tests verify the resolver chain pattern for step handler resolution.

RSpec.describe TaskerCore::Registry::ResolverChain do
  # Test fixtures - handler classes
  before(:all) do
    # Define test handler classes in a test namespace
    module TestHandlers
      class SimpleHandler
        attr_reader :config

        def initialize(config: {})
          @config = config
        end

        def call(_context)
          { status: :success, handler: 'SimpleHandler' }
        end
      end

      class MultiMethodHandler
        attr_reader :config

        def initialize(config: {})
          @config = config
        end

        def call(_context)
          { status: :success, method: :call }
        end

        def process(_context)
          { status: :success, method: :process }
        end

        def refund(_context)
          { status: :success, method: :refund }
        end
      end

      class NoArgHandler
        def call(_context)
          { status: :success, handler: 'NoArgHandler' }
        end
      end
    end
  end

  describe TaskerCore::Types::HandlerDefinition do
    describe 'TAS-93 extensions' do
      context 'with default values' do
        subject(:definition) do
          described_class.new(callable: 'TestHandlers::SimpleHandler')
        end

        it 'defaults handler_method to nil' do
          expect(definition.handler_method).to be_nil
        end

        it 'defaults resolver to nil' do
          expect(definition.resolver).to be_nil
        end

        it 'returns "call" as effective_method' do
          expect(definition.effective_method).to eq('call')
        end

        it 'returns false for uses_method_dispatch?' do
          expect(definition.uses_method_dispatch?).to be false
        end

        it 'returns false for has_resolver_hint?' do
          expect(definition.has_resolver_hint?).to be false
        end
      end

      context 'with custom handler_method' do
        subject(:definition) do
          described_class.new(
            callable: 'TestHandlers::MultiMethodHandler',
            handler_method: 'refund'
          )
        end

        it 'stores the handler_method value' do
          expect(definition.handler_method).to eq('refund')
        end

        it 'returns the custom method as effective_method' do
          expect(definition.effective_method).to eq('refund')
        end

        it 'returns true for uses_method_dispatch?' do
          expect(definition.uses_method_dispatch?).to be true
        end
      end

      context 'with handler_method explicitly set to "call"' do
        subject(:definition) do
          described_class.new(
            callable: 'TestHandlers::SimpleHandler',
            handler_method: 'call'
          )
        end

        it 'returns false for uses_method_dispatch?' do
          expect(definition.uses_method_dispatch?).to be false
        end
      end

      context 'with resolver hint' do
        subject(:definition) do
          described_class.new(
            callable: 'TestHandlers::SimpleHandler',
            resolver: 'payment_resolver'
          )
        end

        it 'stores the resolver value' do
          expect(definition.resolver).to eq('payment_resolver')
        end

        it 'returns true for has_resolver_hint?' do
          expect(definition.has_resolver_hint?).to be true
        end
      end
    end
  end

  describe TaskerCore::Registry::Resolvers::ExplicitMappingResolver do
    subject(:resolver) { described_class.new }

    it 'has name "explicit_mapping"' do
      expect(resolver.name).to eq('explicit_mapping')
    end

    it 'has priority 10' do
      expect(resolver.priority).to eq(10)
    end

    describe '#register and #resolve' do
      let(:definition) do
        TaskerCore::Types::HandlerDefinition.new(callable: 'payment.process')
      end

      it 'resolves registered handler class' do
        resolver.register('payment.process', TestHandlers::SimpleHandler)

        handler = resolver.resolve(definition, {})

        expect(handler).to be_a(TestHandlers::SimpleHandler)
      end

      it 'resolves registered handler instance' do
        instance = TestHandlers::SimpleHandler.new
        resolver.register('payment.process', instance)

        handler = resolver.resolve(definition, {})

        expect(handler).to be(instance)
      end

      it 'resolves registered Proc' do
        handler_proc = proc { |_ctx| { status: :success } }
        resolver.register('payment.process', handler_proc)

        handler = resolver.resolve(definition, {})

        expect(handler).to be(handler_proc)
      end

      it 'returns nil for unregistered callable' do
        definition = TaskerCore::Types::HandlerDefinition.new(callable: 'unknown.handler')

        handler = resolver.resolve(definition, {})

        expect(handler).to be_nil
      end
    end

    describe '#can_resolve?' do
      it 'returns true for registered callable' do
        resolver.register('test.handler', TestHandlers::SimpleHandler)
        definition = TaskerCore::Types::HandlerDefinition.new(callable: 'test.handler')

        expect(resolver.can_resolve?(definition, {})).to be true
      end

      it 'returns false for unregistered callable' do
        definition = TaskerCore::Types::HandlerDefinition.new(callable: 'unknown.handler')

        expect(resolver.can_resolve?(definition, {})).to be false
      end
    end

    describe '#registered_callables' do
      it 'returns list of registered keys' do
        resolver.register('handler.a', TestHandlers::SimpleHandler)
        resolver.register('handler.b', TestHandlers::SimpleHandler)

        expect(resolver.registered_callables).to contain_exactly('handler.a', 'handler.b')
      end
    end
  end

  describe TaskerCore::Registry::Resolvers::ClassConstantResolver do
    subject(:resolver) { described_class.new }

    it 'has name "class_constant"' do
      expect(resolver.name).to eq('class_constant')
    end

    it 'has priority 100' do
      expect(resolver.priority).to eq(100)
    end

    describe '#resolve' do
      it 'resolves handler class from constant path' do
        definition = TaskerCore::Types::HandlerDefinition.new(
          callable: 'TestHandlers::SimpleHandler'
        )

        handler = resolver.resolve(definition, {})

        expect(handler).to be_a(TestHandlers::SimpleHandler)
      end

      it 'passes initialization config to handler' do
        definition = TaskerCore::Types::HandlerDefinition.new(
          callable: 'TestHandlers::SimpleHandler',
          initialization: { api_key: 'secret' }
        )

        handler = resolver.resolve(definition, {})

        expect(handler.config).to eq(api_key: 'secret')
      end

      it 'instantiates handlers without config arg' do
        definition = TaskerCore::Types::HandlerDefinition.new(
          callable: 'TestHandlers::NoArgHandler'
        )

        handler = resolver.resolve(definition, {})

        expect(handler).to be_a(TestHandlers::NoArgHandler)
      end

      it 'returns nil for non-existent class' do
        definition = TaskerCore::Types::HandlerDefinition.new(
          callable: 'NonExistent::Handler'
        )

        handler = resolver.resolve(definition, {})

        expect(handler).to be_nil
      end

      it 'returns nil for class without effective_method' do
        # String class doesn't have .call
        definition = TaskerCore::Types::HandlerDefinition.new(callable: 'String')

        handler = resolver.resolve(definition, {})

        expect(handler).to be_nil
      end
    end

    describe '#can_resolve?' do
      it 'returns true for class-like paths' do
        definition = TaskerCore::Types::HandlerDefinition.new(
          callable: 'TestHandlers::SimpleHandler'
        )

        expect(resolver.can_resolve?(definition, {})).to be true
      end

      it 'returns false for non-class-like paths' do
        definition = TaskerCore::Types::HandlerDefinition.new(callable: 'not_a_class')

        expect(resolver.can_resolve?(definition, {})).to be false
      end
    end
  end

  describe TaskerCore::Registry::Resolvers::MethodDispatchWrapper do
    let(:handler) { TestHandlers::MultiMethodHandler.new }

    describe '#call' do
      it 'redirects call to target method' do
        wrapper = described_class.new(handler, :refund)
        context = double('context')

        result = wrapper.call(context)

        expect(result).to eq({ status: :success, method: :refund })
      end

      it 'raises ArgumentError if handler lacks target method' do
        expect do
          described_class.new(handler, :nonexistent)
        end.to raise_error(ArgumentError, /does not respond to/)
      end
    end

    describe '#unwrap' do
      it 'returns the underlying handler' do
        wrapper = described_class.new(handler, :process)

        expect(wrapper.unwrap).to be(handler)
      end
    end

    describe '#respond_to?' do
      it 'responds to :call' do
        wrapper = described_class.new(handler, :process)

        expect(wrapper.respond_to?(:call)).to be true
      end

      it 'delegates to underlying handler' do
        wrapper = described_class.new(handler, :process)

        expect(wrapper.respond_to?(:refund)).to be true
      end
    end
  end

  describe TaskerCore::Registry::ResolverChain do
    describe '.default' do
      subject(:chain) { described_class.default }

      it 'includes ExplicitMappingResolver' do
        expect(chain.resolver_names).to include('explicit_mapping')
      end

      it 'includes ClassConstantResolver' do
        expect(chain.resolver_names).to include('class_constant')
      end

      it 'orders resolvers by priority' do
        expect(chain.resolver_names).to eq(%w[explicit_mapping class_constant])
      end
    end

    describe '#resolve' do
      subject(:chain) { described_class.default }

      context 'with explicit registration' do
        it 'resolves registered handler' do
          chain.register('my.handler', TestHandlers::SimpleHandler)
          definition = TaskerCore::Types::HandlerDefinition.new(callable: 'my.handler')

          handler = chain.resolve(definition)

          expect(handler).to be_a(TestHandlers::SimpleHandler)
        end

        it 'prefers explicit over class constant' do
          # Register custom instance
          custom_instance = TestHandlers::SimpleHandler.new(config: { custom: true })
          chain.register('TestHandlers::SimpleHandler', custom_instance)

          definition = TaskerCore::Types::HandlerDefinition.new(
            callable: 'TestHandlers::SimpleHandler'
          )

          handler = chain.resolve(definition)

          expect(handler).to be(custom_instance)
          expect(handler.config).to eq(custom: true)
        end
      end

      context 'with class constant fallback' do
        it 'resolves handler class from constant path' do
          definition = TaskerCore::Types::HandlerDefinition.new(
            callable: 'TestHandlers::SimpleHandler'
          )

          handler = chain.resolve(definition)

          expect(handler).to be_a(TestHandlers::SimpleHandler)
        end
      end

      context 'with resolver hint' do
        it 'uses only the named resolver' do
          # Add a custom resolver
          custom_resolver = TaskerCore::Registry::Resolvers::ExplicitMappingResolver.new('custom')
          custom_resolver.register('my.handler', TestHandlers::NoArgHandler)
          chain.add_resolver(custom_resolver)

          definition = TaskerCore::Types::HandlerDefinition.new(
            callable: 'my.handler',
            resolver: 'custom'
          )

          handler = chain.resolve(definition)

          expect(handler).to be_a(TestHandlers::NoArgHandler)
        end

        it 'raises error for unknown resolver' do
          definition = TaskerCore::Types::HandlerDefinition.new(
            callable: 'my.handler',
            resolver: 'nonexistent'
          )

          expect do
            chain.resolve(definition)
          end.to raise_error(TaskerCore::Registry::ResolverChain::ResolverNotFoundError)
        end
      end

      context 'with method dispatch' do
        it 'wraps handler when handler_method is specified' do
          definition = TaskerCore::Types::HandlerDefinition.new(
            callable: 'TestHandlers::MultiMethodHandler',
            handler_method: 'refund'
          )

          handler = chain.resolve(definition)

          expect(handler).to be_a(TaskerCore::Registry::Resolvers::MethodDispatchWrapper)
          expect(handler.target_method).to eq(:refund)
        end

        it 'redirects call to specified method' do
          definition = TaskerCore::Types::HandlerDefinition.new(
            callable: 'TestHandlers::MultiMethodHandler',
            handler_method: 'process'
          )

          handler = chain.resolve(definition)
          result = handler.call(double('context'))

          expect(result).to eq({ status: :success, method: :process })
        end

        it 'does not wrap when handler_method is "call"' do
          definition = TaskerCore::Types::HandlerDefinition.new(
            callable: 'TestHandlers::SimpleHandler',
            handler_method: 'call'
          )

          handler = chain.resolve(definition)

          expect(handler).not_to be_a(TaskerCore::Registry::Resolvers::MethodDispatchWrapper)
        end
      end

      context 'when no resolver can handle' do
        it 'returns nil' do
          definition = TaskerCore::Types::HandlerDefinition.new(
            callable: 'NonExistent::Handler'
          )

          handler = chain.resolve(definition)

          expect(handler).to be_nil
        end
      end
    end

    describe '#can_resolve?' do
      subject(:chain) { described_class.default }

      it 'returns true for registered handler' do
        chain.register('test.handler', TestHandlers::SimpleHandler)
        definition = TaskerCore::Types::HandlerDefinition.new(callable: 'test.handler')

        expect(chain.can_resolve?(definition)).to be true
      end

      it 'returns true for class constant' do
        definition = TaskerCore::Types::HandlerDefinition.new(
          callable: 'TestHandlers::SimpleHandler'
        )

        expect(chain.can_resolve?(definition)).to be true
      end

      it 'returns true for class-like paths (ClassConstantResolver will try)' do
        # ClassConstantResolver.can_resolve? returns true for any class-like path
        # because it's an eligibility check, not a guarantee. The actual resolve()
        # returns nil if the class doesn't exist.
        definition = TaskerCore::Types::HandlerDefinition.new(
          callable: 'NonExistent::Handler'
        )

        expect(chain.can_resolve?(definition)).to be true
        # But resolve() should return nil
        expect(chain.resolve(definition)).to be_nil
      end

      it 'returns false for non-class-like paths' do
        definition = TaskerCore::Types::HandlerDefinition.new(
          callable: 'not_a_class_path'
        )

        expect(chain.can_resolve?(definition)).to be false
      end
    end

    describe '#chain_info' do
      subject(:chain) { described_class.default }

      it 'returns resolver information' do
        chain.register('test.handler', TestHandlers::SimpleHandler)

        info = chain.chain_info

        expect(info.size).to eq(2)
        expect(info.first[:name]).to eq('explicit_mapping')
        expect(info.first[:priority]).to eq(10)
        expect(info.first[:callables]).to eq(1)
      end
    end
  end

  describe TaskerCore::Registry::Resolvers::RegistryResolver do
    # Define a test resolver subclass
    before(:all) do
      class TestPatternResolver < TaskerCore::Registry::Resolvers::RegistryResolver
        handles_pattern(/^Test::.*Handler$/)
        priority_value 50
        resolver_name 'test_pattern'

        def resolve_handler(definition, _config)
          klass = definition.callable.constantize
          klass.new(config: definition.initialization || {})
        rescue NameError
          nil
        end
      end

      class TestPrefixResolver < TaskerCore::Registry::Resolvers::RegistryResolver
        handles_prefix 'Legacy::'
        priority_value 75

        def resolve_handler(definition, _config)
          # Custom legacy resolution
          klass = definition.callable.sub('Legacy::', 'TestHandlers::').constantize
          klass.new(config: definition.initialization || {})
        rescue NameError
          nil
        end
      end

      # Define a test class that matches the pattern
      module Test
        class SampleHandler
          attr_reader :config

          def initialize(config: {})
            @config = config
          end

          def call(_context)
            { status: :success }
          end
        end
      end
    end

    describe 'pattern matching' do
      subject(:resolver) { TestPatternResolver.new }

      it 'has custom name from DSL' do
        expect(resolver.name).to eq('test_pattern')
      end

      it 'has custom priority from DSL' do
        expect(resolver.priority).to eq(50)
      end

      it 'can resolve matching patterns' do
        definition = TaskerCore::Types::HandlerDefinition.new(
          callable: 'Test::SampleHandler'
        )

        expect(resolver.can_resolve?(definition, {})).to be true
      end

      it 'cannot resolve non-matching patterns' do
        definition = TaskerCore::Types::HandlerDefinition.new(
          callable: 'Other::NotAHandler'
        )

        expect(resolver.can_resolve?(definition, {})).to be false
      end

      it 'resolves handler for matching pattern' do
        definition = TaskerCore::Types::HandlerDefinition.new(
          callable: 'Test::SampleHandler',
          initialization: { key: 'value' }
        )

        handler = resolver.resolve(definition, {})

        expect(handler).to be_a(Test::SampleHandler)
        expect(handler.config).to eq(key: 'value')
      end
    end

    describe 'prefix matching' do
      subject(:resolver) { TestPrefixResolver.new }

      it 'has generated name from class name' do
        expect(resolver.name).to eq('test_prefix_resolver')
      end

      it 'can resolve matching prefixes' do
        definition = TaskerCore::Types::HandlerDefinition.new(
          callable: 'Legacy::SimpleHandler'
        )

        expect(resolver.can_resolve?(definition, {})).to be true
      end

      it 'resolves with custom logic' do
        definition = TaskerCore::Types::HandlerDefinition.new(
          callable: 'Legacy::SimpleHandler'
        )

        handler = resolver.resolve(definition, {})

        expect(handler).to be_a(TestHandlers::SimpleHandler)
      end
    end

    describe 'explicit registration via DSL' do
      subject(:resolver) { ExplicitDSLResolver.new }

      before(:all) do
        class ExplicitDSLResolver < TaskerCore::Registry::Resolvers::RegistryResolver
          register_handler 'shortcut.handler', TestHandlers::SimpleHandler
          register_handler 'another.handler', TestHandlers::NoArgHandler
        end
      end

      it 'resolves explicitly registered handlers' do
        definition = TaskerCore::Types::HandlerDefinition.new(callable: 'shortcut.handler')

        handler = resolver.resolve(definition, {})

        expect(handler).to be_a(TestHandlers::SimpleHandler)
      end

      it 'tracks registered callables' do
        expect(resolver.registered_callables).to include('shortcut.handler', 'another.handler')
      end
    end
  end
end
