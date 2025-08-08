# frozen_string_literal: true

require_relative 'lib/tasker_core/version'

Gem::Specification.new do |spec|
  spec.name          = 'tasker-core-rb'
  spec.version       = TaskerCore::VERSION
  spec.authors       = ['Pete Taylor']
  spec.email         = ['pete.jc.taylor@hey.com']

  spec.summary       = 'High-performance Ruby bindings for Tasker workflow orchestration'
  spec.description   = <<~DESC
    Ruby FFI bindings for tasker-core-rs, providing 10-100x performance improvements
    for workflow orchestration, dependency resolution, and state management.

    This gem enables Rails applications using the Tasker engine to leverage
    Rust's performance for computationally intensive orchestration operations
    while maintaining Ruby's flexibility for business logic.
  DESC

  spec.homepage      = 'https://github.com/tasker-systems/tasker-core-rs'
  spec.license       = 'MIT'
  spec.required_ruby_version = '>= 3.0.0'

  spec.metadata['homepage_uri'] = spec.homepage
  spec.metadata['source_code_uri'] = 'https://github.com/tasker-systems/tasker-core-rs/tree/main/bindings/ruby'
  spec.metadata['changelog_uri'] = 'https://github.com/tasker-systems/tasker-core-rs/blob/main/bindings/ruby/CHANGELOG.md'
  spec.metadata['documentation_uri'] = 'https://github.com/tasker-systems/tasker-core-rs/blob/main/docs/RUBY.md'
  spec.metadata['bug_tracker_uri'] = 'https://github.com/tasker-systems/tasker-core-rs/issues'

  # Include all necessary files for the gem
  spec.files = Dir[
    'lib/**/*',
    'src/**/*',
    'ext/**/*',
    'Cargo.toml',
    'README.md',
    'CHANGELOG.md',
    'LICENSE'
  ].select { |f| File.file?(f) }

  spec.require_paths = ['lib']
  spec.extensions    = ['ext/tasker_core/extconf.rb']

  # Magnus and Rust compilation dependencies
  spec.add_dependency 'rb_sys', '~> 0.9.39'

  spec.add_dependency 'activemodel', '~> 8.0'
  spec.add_dependency 'activerecord', '~> 8.0'

  spec.add_dependency 'json-schema', '~> 2.4', '>= 2.4.0'
  spec.add_dependency 'logger', '~> 1.6'

  spec.add_dependency 'dry-events', '~> 1.1'
  spec.add_dependency 'dry-struct', '~> 1.8'
  spec.add_dependency 'dry-types', '~> 1.8'
  spec.add_dependency 'dry-validation', '~> 1.10'

  spec.add_dependency 'dotenv', '~> 2.8'
  spec.add_dependency 'faraday', '~> 2.12.2'
  spec.add_dependency 'pg', '~> 1.5'
  # Concurrent execution support for BatchStepExecutionOrchestrator
  spec.add_dependency 'concurrent-ruby', '~> 1.2'

  # Ensure we have a Rust toolchain for compilation
  spec.metadata['allowed_push_host'] = 'https://rubygems.org'
  spec.metadata['rubygems_mfa_required'] = 'true'

  # Post-install message
  spec.post_install_message = <<~MSG

    🦀 tasker-core-rb successfully installed!

    This gem provides high-performance Rust-powered workflow orchestration.

    Documentation: https://github.com/tasker-systems/tasker-core-rs/blob/main/docs/RUBY.md
    Examples: https://github.com/tasker-systems/tasker-core-rs/tree/main/examples

    For Rails integration, see the tasker-engine gem documentation.

  MSG
end
