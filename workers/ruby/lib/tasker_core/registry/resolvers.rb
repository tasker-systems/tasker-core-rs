# frozen_string_literal: true

# TAS-93: Step Handler Resolver Infrastructure
#
# This module provides the resolver chain pattern for step handler resolution.
# Resolvers are tried in priority order until one successfully resolves.
#
# == Built-in Resolvers
#
# - ExplicitMappingResolver (priority 10): Explicit key → handler mappings
# - ClassConstantResolver (priority 100): Class path inference via const_get
#
# == Custom Resolvers
#
# Extend RegistryResolver for developer-friendly custom resolution:
#
#   class PaymentResolver < TaskerCore::Registry::Resolvers::RegistryResolver
#     handles_pattern(/^Payments::/)
#     priority_value 50
#
#     def resolve_handler(definition, config)
#       # Custom resolution logic
#     end
#   end
#
# == Method Dispatch
#
# When HandlerDefinition specifies `method: refund`, the chain automatically
# wraps the handler with MethodDispatchWrapper to redirect .call() → .refund()
#
require_relative 'resolvers/base_resolver'
require_relative 'resolvers/registry_resolver'
require_relative 'resolvers/explicit_mapping_resolver'
require_relative 'resolvers/class_constant_resolver'
require_relative 'resolvers/method_dispatch_wrapper'

module TaskerCore
  module Registry
    module Resolvers
    end
  end
end
