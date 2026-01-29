# frozen_string_literal: true

# SimpleCov configuration for tasker-worker-rb
# Activated when COVERAGE=true environment variable is set

SimpleCov.start do
  # Only run when explicitly requested
  return unless ENV['COVERAGE']

  # Output formats
  formatter SimpleCov::Formatter::MultiFormatter.new([
                                                       SimpleCov::Formatter::HTMLFormatter,
                                                       SimpleCov::Formatter::JSONFormatter
                                                     ])

  # Coverage directory
  coverage_dir 'coverage'

  # Track files in lib/ directory
  track_files 'lib/**/*.rb'

  # Add filters
  add_filter '/spec/'
  add_filter '/vendor/'
  add_filter '/ext/'

  # Groups for organization
  add_group 'Core', 'lib/tasker'
  add_group 'Handlers', 'lib/handlers'

  # Minimum coverage threshold
  # Skip threshold when COVERAGE_COLLECT_ONLY is set (data collection mode);
  # threshold enforcement happens via cargo make coverage-check
  unless ENV['COVERAGE_COLLECT_ONLY']
    minimum_coverage 70
    refuse_coverage_drop
  end

  # Enable branch coverage
  enable_coverage :branch
end
