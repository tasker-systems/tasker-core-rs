/**
 * Deno FFI Runtime Integration Tests.
 *
 * Tests the DenoRuntime implementation against the actual Rust FFI library.
 * Verifies that Deno.dlopen correctly interfaces with the native code.
 *
 * Prerequisites:
 * - Build FFI library: cargo build -p tasker-worker-ts --release
 * - (Optional) DATABASE_URL for bootstrap tests
 *
 * Run: deno test --allow-ffi --allow-env --allow-read tests/integration/ffi/deno-runtime.test.ts
 *
 * Note: Requires Deno with --allow-ffi permission and unstable FFI feature.
 */

// Type declarations for Deno's global test APIs
// biome-ignore lint/suspicious/noExplicitAny: Deno global is runtime-specific
declare const Deno: any;

// Helper to detect if running in Deno
const isDeno = typeof Deno !== 'undefined';

// Import common utilities - path adjusted for Deno module resolution
import {
  assertVersionString,
  findLibraryPath,
  SKIP_BOOTSTRAP_MESSAGE,
  SKIP_LIBRARY_MESSAGE,
  shouldRunBootstrapTests,
} from './common.ts';

// Dynamic import for DenoRuntime
let DenoRuntime: typeof import('../../../src/ffi/deno-runtime.ts').DenoRuntime;
let runtime: InstanceType<typeof DenoRuntime> | null = null;
let libraryPath: string | null = null;

// Setup function
async function setup(): Promise<void> {
  if (!isDeno) {
    console.warn('Not running in Deno environment');
    return;
  }

  libraryPath = findLibraryPath();
  if (!libraryPath) {
    console.warn(SKIP_LIBRARY_MESSAGE);
    return;
  }

  try {
    const module = await import('../../../src/ffi/deno-runtime.ts');
    DenoRuntime = module.DenoRuntime;
    runtime = new DenoRuntime();
    await runtime.load(libraryPath);
  } catch (error) {
    console.warn('Failed to load DenoRuntime:', error);
    runtime = null;
  }
}

// Teardown function
function teardown(): void {
  if (runtime?.isLoaded) {
    runtime.unload();
  }
}

// Only run tests if Deno is available
if (isDeno) {
  // Setup before all tests
  Deno.test({
    name: 'setup',
    fn: setup,
    sanitizeResources: false,
    sanitizeOps: false,
  });

  // Library loading tests
  Deno.test({
    name: 'DenoRuntime: loads FFI library successfully',
    fn: () => {
      if (!libraryPath || !runtime) {
        console.warn(SKIP_LIBRARY_MESSAGE);
        return;
      }
      if (runtime.isLoaded !== true) {
        throw new Error(`Expected isLoaded to be true, got ${runtime.isLoaded}`);
      }
    },
    sanitizeResources: false,
    sanitizeOps: false,
  });

  Deno.test({
    name: 'DenoRuntime: has correct runtime name',
    fn: () => {
      if (!libraryPath || !runtime) {
        console.warn(SKIP_LIBRARY_MESSAGE);
        return;
      }
      if (runtime.name !== 'deno') {
        throw new Error(`Expected name to be 'deno', got ${runtime.name}`);
      }
    },
    sanitizeResources: false,
    sanitizeOps: false,
  });

  // Version information tests
  Deno.test({
    name: 'DenoRuntime: returns valid version string',
    fn: () => {
      if (!libraryPath || !runtime) {
        console.warn(SKIP_LIBRARY_MESSAGE);
        return;
      }
      const version = runtime.getVersion();
      if (typeof version !== 'string') {
        throw new Error(`Expected version to be string, got ${typeof version}`);
      }
      if (version.length === 0) {
        throw new Error('Expected version to be non-empty');
      }
      if (!assertVersionString(version)) {
        throw new Error(`Expected valid version string, got ${version}`);
      }
    },
    sanitizeResources: false,
    sanitizeOps: false,
  });

  Deno.test({
    name: 'DenoRuntime: returns valid Rust version string',
    fn: () => {
      if (!libraryPath || !runtime) {
        console.warn(SKIP_LIBRARY_MESSAGE);
        return;
      }
      const rustVersion = runtime.getRustVersion();
      if (typeof rustVersion !== 'string') {
        throw new Error(`Expected rustVersion to be string, got ${typeof rustVersion}`);
      }
      if (rustVersion.length === 0) {
        throw new Error('Expected rustVersion to be non-empty');
      }
    },
    sanitizeResources: false,
    sanitizeOps: false,
  });

  // Health check tests
  Deno.test({
    name: 'DenoRuntime: passes health check when loaded',
    fn: () => {
      if (!libraryPath || !runtime) {
        console.warn(SKIP_LIBRARY_MESSAGE);
        return;
      }
      if (runtime.healthCheck() !== true) {
        throw new Error('Expected healthCheck to return true');
      }
    },
    sanitizeResources: false,
    sanitizeOps: false,
  });

  // Worker status tests
  Deno.test({
    name: 'DenoRuntime: reports worker not running initially',
    fn: () => {
      if (!libraryPath || !runtime) {
        console.warn(SKIP_LIBRARY_MESSAGE);
        return;
      }
      if (runtime.isWorkerRunning() !== false) {
        throw new Error('Expected isWorkerRunning to return false');
      }
    },
    sanitizeResources: false,
    sanitizeOps: false,
  });

  Deno.test({
    name: 'DenoRuntime: returns worker status object',
    fn: () => {
      if (!libraryPath || !runtime) {
        console.warn(SKIP_LIBRARY_MESSAGE);
        return;
      }
      const status = runtime.getWorkerStatus();
      if (status === null || status === undefined) {
        throw new Error('Expected status to be defined');
      }
      if (typeof status.running !== 'boolean') {
        throw new Error(`Expected running to be boolean, got ${typeof status.running}`);
      }
    },
    sanitizeResources: false,
    sanitizeOps: false,
  });

  // Metrics tests
  Deno.test({
    name: 'DenoRuntime: returns FfiDispatchMetrics object',
    fn: () => {
      if (!libraryPath || !runtime) {
        console.warn(SKIP_LIBRARY_MESSAGE);
        return;
      }
      const metrics = runtime.getFfiDispatchMetrics();
      if (metrics === null || metrics === undefined) {
        throw new Error('Expected metrics to be defined');
      }
      if (typeof metrics.pending_count !== 'number') {
        throw new Error(`Expected pending_count to be number, got ${typeof metrics.pending_count}`);
      }
      if (typeof metrics.starvation_detected !== 'boolean') {
        throw new Error(
          `Expected starvation_detected to be boolean, got ${typeof metrics.starvation_detected}`
        );
      }
    },
    sanitizeResources: false,
    sanitizeOps: false,
  });

  Deno.test({
    name: 'DenoRuntime: metrics have valid initial values',
    fn: () => {
      if (!libraryPath || !runtime) {
        console.warn(SKIP_LIBRARY_MESSAGE);
        return;
      }
      const metrics = runtime.getFfiDispatchMetrics();
      if (metrics.pending_count < 0) {
        throw new Error('Expected pending_count >= 0');
      }
      if (metrics.starving_event_count < 0) {
        throw new Error('Expected starving_event_count >= 0');
      }
    },
    sanitizeResources: false,
    sanitizeOps: false,
  });

  // Logging tests
  Deno.test({
    name: 'DenoRuntime: logError does not throw',
    fn: () => {
      if (!libraryPath || !runtime) {
        console.warn(SKIP_LIBRARY_MESSAGE);
        return;
      }
      runtime.logError('Test error message');
    },
    sanitizeResources: false,
    sanitizeOps: false,
  });

  Deno.test({
    name: 'DenoRuntime: logWarn does not throw',
    fn: () => {
      if (!libraryPath || !runtime) {
        console.warn(SKIP_LIBRARY_MESSAGE);
        return;
      }
      runtime.logWarn('Test warning message');
    },
    sanitizeResources: false,
    sanitizeOps: false,
  });

  Deno.test({
    name: 'DenoRuntime: logInfo does not throw',
    fn: () => {
      if (!libraryPath || !runtime) {
        console.warn(SKIP_LIBRARY_MESSAGE);
        return;
      }
      runtime.logInfo('Test info message');
    },
    sanitizeResources: false,
    sanitizeOps: false,
  });

  Deno.test({
    name: 'DenoRuntime: logDebug does not throw',
    fn: () => {
      if (!libraryPath || !runtime) {
        console.warn(SKIP_LIBRARY_MESSAGE);
        return;
      }
      runtime.logDebug('Test debug message');
    },
    sanitizeResources: false,
    sanitizeOps: false,
  });

  Deno.test({
    name: 'DenoRuntime: logTrace does not throw',
    fn: () => {
      if (!libraryPath || !runtime) {
        console.warn(SKIP_LIBRARY_MESSAGE);
        return;
      }
      runtime.logTrace('Test trace message');
    },
    sanitizeResources: false,
    sanitizeOps: false,
  });

  Deno.test({
    name: 'DenoRuntime: logging with fields does not throw',
    fn: () => {
      if (!libraryPath || !runtime) {
        console.warn(SKIP_LIBRARY_MESSAGE);
        return;
      }
      runtime.logInfo('Test with fields', {
        test_key: 'test_value',
        numeric: 42,
      });
    },
    sanitizeResources: false,
    sanitizeOps: false,
  });

  // Maintenance function tests
  Deno.test({
    name: 'DenoRuntime: checkStarvationWarnings does not throw',
    fn: () => {
      if (!libraryPath || !runtime) {
        console.warn(SKIP_LIBRARY_MESSAGE);
        return;
      }
      runtime.checkStarvationWarnings();
    },
    sanitizeResources: false,
    sanitizeOps: false,
  });

  Deno.test({
    name: 'DenoRuntime: cleanupTimeouts does not throw',
    fn: () => {
      if (!libraryPath || !runtime) {
        console.warn(SKIP_LIBRARY_MESSAGE);
        return;
      }
      runtime.cleanupTimeouts();
    },
    sanitizeResources: false,
    sanitizeOps: false,
  });

  // Event polling tests
  Deno.test({
    name: 'DenoRuntime: pollStepEvents returns null when no events',
    fn: () => {
      if (!libraryPath || !runtime) {
        console.warn(SKIP_LIBRARY_MESSAGE);
        return;
      }
      const event = runtime.pollStepEvents();
      if (event !== null) {
        throw new Error('Expected pollStepEvents to return null');
      }
    },
    sanitizeResources: false,
    sanitizeOps: false,
  });

  // Worker lifecycle tests
  Deno.test({
    name: 'DenoRuntime: bootstrapWorker returns valid result',
    fn: () => {
      if (!libraryPath || !runtime) {
        console.warn(SKIP_LIBRARY_MESSAGE);
        return;
      }
      if (!shouldRunBootstrapTests()) {
        console.warn(SKIP_BOOTSTRAP_MESSAGE);
        return;
      }

      const result = runtime.bootstrapWorker({});
      if (result === null || result === undefined) {
        throw new Error('Expected result to be defined');
      }
      if (typeof result.success !== 'boolean') {
        throw new Error(`Expected success to be boolean, got ${typeof result.success}`);
      }

      if (result.success) {
        runtime.stopWorker();
      }
    },
    sanitizeResources: false,
    sanitizeOps: false,
  });

  // Unload behavior tests
  Deno.test({
    name: 'DenoRuntime: can unload and reload library',
    fn: async () => {
      if (!libraryPath || !runtime) {
        console.warn(SKIP_LIBRARY_MESSAGE);
        return;
      }

      runtime.unload();
      if (runtime.isLoaded) {
        throw new Error('Expected isLoaded to be false after unload');
      }

      await runtime.load(libraryPath);
      // biome-ignore lint/complexity/noExtraBooleanCast: Needed to avoid TypeScript narrowing issues with getter
      if (!Boolean(runtime.isLoaded)) {
        throw new Error('Expected isLoaded to be true after reload');
      }
      if (!runtime.healthCheck()) {
        throw new Error('Expected healthCheck to return true after reload');
      }
    },
    sanitizeResources: false,
    sanitizeOps: false,
  });

  // Teardown after all tests
  Deno.test({
    name: 'teardown',
    fn: teardown,
    sanitizeResources: false,
    sanitizeOps: false,
  });
} else {
  console.warn('Deno tests skipped: Not running in Deno environment');
}
