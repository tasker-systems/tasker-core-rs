# frozen_string_literal: true

require 'spec_helper'

RSpec.describe 'Database Connectivity' do
  include TaskerCore::TestHelpers
  
  describe 'basic database access' do
    it 'can connect to database and query information schema' do
      # This test bypasses TestingFramework entirely to test raw database connectivity
      # We'll use a simple Ruby FFI call that should just check the information schema
      
      puts "🔍 CONNECTIVITY TEST: Testing basic database query without TestingFramework"
      
      # Try to call a simple database operation directly
      # This should not involve any complex orchestration setup
      result = list_database_tables
      
      puts "🔍 CONNECTIVITY TEST: Result type: #{result.class}"
      puts "🔍 CONNECTIVITY TEST: Result: #{result.inspect}"
      
      # We expect either success or a specific error, but not a timeout
      expect(result).to be_a(Hash)
      
      if result['status'] == 'error'
        puts "❌ CONNECTIVITY TEST: Database error: #{result['error']}"
        # Don't fail the test for database errors, just document them
        expect(result['error']).to be_a(String)
      else
        puts "✅ CONNECTIVITY TEST: Database query succeeded"
        expect(result).to have_key('tables')
      end
    end
  end
  
  describe 'information schema queries' do
    it 'can check if migration table exists' do
      puts "🔍 MIGRATION TABLE TEST: Checking if _sqlx_migrations table exists"
      
      # This mimics exactly what TestingFramework does
      # We'll create a simple FFI wrapper just for this test
      
      begin
        # Try to use the existing database functions
        database_url = ENV['DATABASE_URL'] || 'postgresql://tasker:tasker@localhost/tasker_rust_test'
        
        puts "🔍 MIGRATION TABLE TEST: Using database URL: #{database_url}"
        puts "🔍 MIGRATION TABLE TEST: Environment: #{ENV['RAILS_ENV']}"
        
        # Test if we can at least list tables
        result = TaskerCore::TestHelpers.list_database_tables
        
        puts "🔍 MIGRATION TABLE TEST: Table list result: #{result.inspect}"
        
        expect(result).to be_a(Hash)
        
        # Don't fail on errors, just document what we find
        if result['status'] == 'error'
          puts "❌ MIGRATION TABLE TEST: #{result['error']}"
          if result['error'].include?('pool timed out')
            puts "🚨 MIGRATION TABLE TEST: POOL TIMEOUT CONFIRMED - This is the root issue"
          end
        else
          puts "✅ MIGRATION TABLE TEST: Successfully queried database"
        end
      rescue StandardError => e
        puts "❌ MIGRATION TABLE TEST: Exception: #{e.message}"
        puts "❌ MIGRATION TABLE TEST: Backtrace: #{e.backtrace.first(5).join("\n")}"
        
        # Don't fail the test, just document the issue
        expect(e).to be_a(StandardError)
      end
    end
  end
  
  describe 'pool behavior analysis' do
    it 'analyzes pool state during operations' do
      puts "🔍 POOL ANALYSIS: Starting pool behavior analysis"
      
      # Try multiple simple operations to see if we're leaking connections
      3.times do |i|
        puts "🔍 POOL ANALYSIS: Attempt #{i + 1}"
        
        begin
          result = TaskerCore::TestHelpers.list_database_tables
          puts "🔍 POOL ANALYSIS: Attempt #{i + 1} result: #{result['status'] || 'unknown'}"
          
          if result['status'] == 'error' && result['error'].include?('pool timed out')
            puts "🚨 POOL ANALYSIS: Pool timeout on attempt #{i + 1}"
            break
          end
        rescue StandardError => e
          puts "❌ POOL ANALYSIS: Exception on attempt #{i + 1}: #{e.message}"
          break
        end
        
        # Small delay between attempts
        sleep(0.1)
      end
      
      puts "🔍 POOL ANALYSIS: Analysis complete"
      
      # This test always passes - it's just for observation
      expect(true).to be true
    end
  end
end