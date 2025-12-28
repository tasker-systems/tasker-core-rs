import { defineConfig } from 'tsup';

export default defineConfig({
  entry: ['src/index.ts', 'src/ffi/index.ts', 'src/events/index.ts'],
  format: ['esm'],
  dts: true,
  sourcemap: true,
  clean: true,
  target: 'es2022',
  outDir: 'dist',
  splitting: false,
  treeshake: true,
  external: [
    // Runtime-specific FFI modules (loaded dynamically at runtime)
    'bun:ffi',
    'ffi-napi',
    'ref-napi',
  ],
});
