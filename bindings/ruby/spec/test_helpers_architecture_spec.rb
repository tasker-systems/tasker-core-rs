# frozen_string_literal: true

require 'spec_helper'

RSpec.describe 'TaskerCore::TestHelpers Architecture' do
  describe 'module structure' do
    it 'exposes TestHelpers as a nested module' do
      expect(defined?(TaskerCore::TestHelpers)).to eq('constant')
      expect(TaskerCore::TestHelpers).to be_a(Module)
    end

    it 'exposes the expected factory methods' do
      expected_methods = %w[
        create_test_task_with_factory
        create_test_workflow_step_with_factory
        create_test_foundation_with_factory
        run_migrations
        drop_schema
      ]

      available_methods = TaskerCore::TestHelpers.methods(false).map(&:to_s)

      expected_methods.each do |method|
        expect(available_methods).to include(method)
      end
    end

    it 'does not expose factory methods at the root TaskerCore level' do
      root_methods = TaskerCore.methods(false).map(&:to_s)

      factory_methods = %w[
        create_test_task_with_factory
        create_test_workflow_step_with_factory
      ]

      factory_methods.each do |method|
        expect(root_methods).not_to include(method)
      end
    end
  end

  describe 'method availability' do
    it 'methods can be called (even if they require database)' do
      # This tests that the FFI binding works, even if database connection fails
      result = TaskerCore::TestHelpers.create_test_task_with_factory({ 'test' => true })

      expect(result).to be_a(Hash)
      # If there's an error, it should be a structured error response
      if result['error']
        expect(result['error']).to be_a(String)
        expect(result['error']).to include('pool timed out') # Should be a database-related error
      else
        # If successful, should have expected structure
        expect(result).to have_key('task_id')
      end
    end
  end

  describe 'Ruby helper wrapper methods' do
    include TaskerCore::TestHelpers

    it 'helper methods exist and call the Rust functions' do
      expect(self).to respond_to(:create_test_task)
      expect(self).to respond_to(:create_test_workflow_step)
      expect(self).to respond_to(:create_test_foundation)
      expect(self).to respond_to(:run_migrations)
      expect(self).to respond_to(:drop_schema)
      expect(self).to respond_to(:reset_test_database)
    end

    it 'helper methods pass DATABASE_URL from environment' do
      # Mock the underlying Rust call to verify URL is passed
      TaskerCore::TestHelpers.method(:create_test_task_with_factory)

      expect(TaskerCore::TestHelpers).to receive(:create_test_task_with_factory) do |options|
        expect(options).to have_key('database_url')
        expect(options['database_url']).to eq(ENV.fetch('DATABASE_URL', nil))
        { 'mock' => 'response' }
      end

      create_test_task({ 'test' => true })
    end
  end
end
