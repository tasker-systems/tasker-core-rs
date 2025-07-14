# frozen_string_literal: true

module TaskerCore
  # Version synchronization with the core Rust crate
  # This should be kept in sync with the Cargo.toml version
  VERSION = '0.1.0'

  # Rust core version compatibility
  RUST_CORE_VERSION = '0.1.0'

  def self.version_info
    {
      ruby_bindings: VERSION,
      rust_core: RUST_CORE_VERSION,
      built_at: defined?(BUILT_AT) ? BUILT_AT : 'development'
    }
  end
end
