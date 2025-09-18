#!/usr/bin/env ruby
# frozen_string_literal: true

require 'mkmf'
require 'rb_sys/mkmf'

# Ensure we have the required tools
unless find_executable('cargo')
  abort <<~MSG

    ❌ Rust toolchain not found!

    tasker-worker-rb requires Rust to compile the native extension.

    Please install Rust:
      curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
      source $HOME/.cargo/env

    Or visit: https://rustup.rs/

  MSG
end

# Check Rust version
rust_version = `cargo --version`.strip
puts "🦀 Using Rust: #{rust_version}"

# Enable test-helpers feature when running tests
if ENV['ENABLE_TEST_HELPERS'] || ENV['TASKER_ENV'] == 'test'
  ENV['CARGO_FEATURE_TEST_HELPERS'] = '1'
  puts '🧪 Enabling test helpers for development/test environment'
end

# Create the Rust makefile for the extension
# This will compile the Rust code into a Ruby-loadable shared library
create_rust_makefile('tasker_worker_rb')

puts "✅ Configuration complete! Run 'make' to compile the extension."
