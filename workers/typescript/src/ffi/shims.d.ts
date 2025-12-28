/**
 * Type declaration shims for runtime-specific FFI modules.
 *
 * These modules are loaded dynamically and may not be available
 * in all runtimes. The declarations ensure TypeScript compilation
 * works across all environments.
 */

// Node.js FFI modules (optional peer dependencies)
declare module 'ffi-napi' {
  // biome-ignore lint/suspicious/noExplicitAny: FFI types are dynamic
  export function Library(path: string, functions: Record<string, any>): any;
}

declare module 'ref-napi' {
  export const types: {
    CString: unknown;
    int: unknown;
    void: unknown;
  };
}

// Bun FFI module
declare module 'bun:ffi' {
  export enum FFIType {
    ptr = 'ptr',
    i32 = 'i32',
    void = 'void',
  }

  // biome-ignore lint/suspicious/noExplicitAny: FFI types are dynamic
  export function dlopen(path: string, symbols: Record<string, any>): any;
  // biome-ignore lint/suspicious/noExplicitAny: FFI pointer types
  export function ptr(buffer: Uint8Array): any;
  export class CString {
    // biome-ignore lint/suspicious/noExplicitAny: FFI pointer input
    constructor(ptr: any);
    toString(): string;
  }
}
