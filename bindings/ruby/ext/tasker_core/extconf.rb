#!/usr/bin/env ruby
# frozen_string_literal: true

require "mkmf"
require "rb_sys/mkmf"

# Ensure we have the required tools
unless find_executable("cargo")
  abort <<~MSG
    
    ❌ Rust toolchain not found!
    
    tasker-core-rb requires Rust to compile the native extension.
    
    Please install Rust:
      curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
      source $HOME/.cargo/env
    
    Or visit: https://rustup.rs/
    
  MSG
end

# Check Rust version
rust_version = `cargo --version`.strip
puts "🦀 Using Rust: #{rust_version}"

# Create the Rust makefile for the extension
# This will compile the Rust code into a Ruby-loadable shared library
create_rust_makefile("tasker_core_rb")

# Ensure we have the required system dependencies
unless pkg_config("libpq")
  puts "⚠️  Warning: PostgreSQL development libraries not found via pkg-config"
  puts "   The extension may still compile if PostgreSQL is installed in standard locations"
end

puts "✅ Configuration complete! Run 'make' to compile the extension."