# frozen_string_literal: true

require 'spec_helper'

RSpec.describe 'TaskerCore version constants' do
  describe 'VERSION' do
    it 'is a string matching semver format' do
      expect(TaskerCore::VERSION).to be_a(String)
      expect(TaskerCore::VERSION).to match(/\A\d+\.\d+\.\d+\z/)
    end
  end

  describe 'Version' do
    it 'is a string matching semver format' do
      expect(TaskerCore::Version).to be_a(String)
      expect(TaskerCore::Version).to match(/\A\d+\.\d+\.\d+\z/)
    end

    it 'matches VERSION' do
      expect(TaskerCore::Version).to eq(TaskerCore::VERSION)
    end
  end

  describe 'RUST_CORE_VERSION' do
    it 'is a string matching semver format' do
      expect(TaskerCore::RUST_CORE_VERSION).to be_a(String)
      expect(TaskerCore::RUST_CORE_VERSION).to match(/\A\d+\.\d+\.\d+\z/)
    end
  end

  describe '.version_info' do
    subject(:info) { TaskerCore.version_info }

    it 'returns a hash with ruby_bindings and rust_core keys' do
      expect(info).to be_a(Hash)
      expect(info).to have_key(:ruby_bindings)
      expect(info).to have_key(:rust_core)
    end

    it 'contains version strings' do
      expect(info[:ruby_bindings]).to eq(TaskerCore::VERSION)
      expect(info[:rust_core]).to eq(TaskerCore::RUST_CORE_VERSION)
    end
  end
end
