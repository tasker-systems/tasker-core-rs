/**
 * TAS-93: Handler definition type for resolver chain.
 *
 * Represents the configuration for a step handler, including:
 * - callable: The handler address/identifier
 * - method: Optional method to invoke (defaults to "call")
 * - resolver: Optional resolver hint to bypass chain
 * - initialization: Optional configuration passed to handler
 *
 * @example
 * ```typescript
 * const definition: HandlerDefinition = {
 *   callable: 'payment_handler',
 *   method: 'refund',
 *   resolver: 'explicit_mapping',
 *   initialization: { apiKey: 'secret' },
 * };
 *
 * // Check if method dispatch is needed
 * if (usesMethodDispatch(definition)) {
 *   // Wrap handler for method dispatch
 * }
 * ```
 */

import type { HandlerDefinitionDto } from '../ffi/generated/HandlerDefinitionDto.js';

/**
 * Handler definition with resolution metadata.
 */
export interface HandlerDefinition {
  /** The handler address/identifier (required) */
  callable: string;

  /** Method to invoke on the handler (defaults to "call") */
  method?: string | null;

  /** Resolver hint to bypass chain resolution */
  resolver?: string | null;

  /** Initialization configuration passed to handler */
  initialization?: Record<string, unknown>;
}

/**
 * Get the effective method to invoke.
 *
 * @param definition - Handler definition
 * @returns The method name (defaults to "call")
 */
export function effectiveMethod(definition: HandlerDefinition): string {
  // Handle null, undefined, and empty string
  if (!definition.method || definition.method.length === 0) {
    return 'call';
  }
  return definition.method;
}

/**
 * Check if method dispatch is needed.
 *
 * Returns true if a non-default method is specified.
 *
 * @param definition - Handler definition
 * @returns True if method dispatch wrapper is needed
 */
export function usesMethodDispatch(definition: HandlerDefinition): boolean {
  // Handle null, undefined, empty string, and 'call'
  if (!definition.method || definition.method.length === 0) {
    return false;
  }
  return definition.method !== 'call';
}

/**
 * Check if a resolver hint is provided.
 *
 * @param definition - Handler definition
 * @returns True if resolver hint should be used
 */
export function hasResolverHint(definition: HandlerDefinition): boolean {
  return definition.resolver != null && definition.resolver.length > 0;
}

/**
 * Create a HandlerDefinition from a callable string.
 *
 * @param callable - Handler callable string
 * @returns HandlerDefinition with defaults
 */
export function fromCallable(callable: string): HandlerDefinition {
  return {
    callable,
    method: null,
    resolver: null,
    initialization: {},
  };
}

/**
 * Create a HandlerDefinition from FFI DTO.
 *
 * Maps Rust field names to TypeScript:
 * - Rust uses 'method' field (matches our interface)
 *
 * @param dto - FFI handler definition DTO
 * @returns HandlerDefinition
 */
export function fromDto(dto: HandlerDefinitionDto | null | undefined): HandlerDefinition {
  if (!dto) {
    return {
      callable: '',
      method: null,
      resolver: null,
      initialization: {},
    };
  }

  // FFI DTO may have method and resolver fields
  const dtoRecord = dto as Record<string, unknown>;
  const method = dtoRecord.method as string | null | undefined;
  const resolver = dtoRecord.resolver as string | null | undefined;

  return {
    callable: dto.callable ?? '',
    // Convert undefined to null for exactOptionalPropertyTypes compatibility
    method: method ?? null,
    resolver: resolver ?? null,
    initialization: dto.initialization ?? {},
  };
}

/**
 * Type alias for handler specification input.
 *
 * Accepts:
 * - string: Simple callable name
 * - HandlerDefinition: Full definition with method/resolver
 * - HandlerDefinitionDto: FFI DTO from Rust
 */
export type HandlerSpec = string | HandlerDefinition | HandlerDefinitionDto;

/**
 * Normalize any handler spec to HandlerDefinition.
 *
 * @param spec - Handler specification (string, definition, or DTO)
 * @returns Normalized HandlerDefinition
 */
export function normalizeToDefinition(spec: HandlerSpec): HandlerDefinition {
  if (typeof spec === 'string') {
    return fromCallable(spec);
  }

  // Check if it's already a HandlerDefinition (has method/resolver properties)
  if ('method' in spec || 'resolver' in spec) {
    return spec as HandlerDefinition;
  }

  // Assume it's a DTO
  return fromDto(spec as HandlerDefinitionDto);
}
