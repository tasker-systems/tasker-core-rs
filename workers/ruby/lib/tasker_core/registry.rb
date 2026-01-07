# frozen_string_literal: true

# TAS-93: Resolver chain infrastructure (load first - used by HandlerRegistry)
require_relative 'registry/resolvers'
require_relative 'registry/resolver_chain'

require_relative 'registry/handler_registry'

module TaskerCore
  module Registry
  end
end
