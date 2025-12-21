# FFI Memory Management in TypeScript Workers

**Status**: Active  
**Applies To**: TypeScript/Bun/Node.js FFI (TAS-100)  
**Related**: Ruby (Magnus), Python (PyO3)

---

## Overview

This document explains the memory management pattern used when calling Rust functions from TypeScript via FFI (Foreign Function Interface). Understanding this pattern is critical for preventing memory leaks and undefined behavior.

**Key Principle**: When Rust hands memory to JavaScript across the FFI boundary, Rust's ownership system no longer applies. The JavaScript code becomes responsible for explicitly freeing that memory.

---

## The Memory Handoff Pattern

### Three-Step Process

```typescript
// 1. ALLOCATE: Rust allocates memory and returns a pointer
const ptr = this.lib.symbols.get_worker_status() as Pointer;

// 2. READ: JavaScript reads/copies the data from that pointer
const json = new CString(ptr);              // Read C string into JS string
const status = JSON.parse(json);            // Parse into JS object

// 3. FREE: JavaScript tells Rust to deallocate the memory
this.lib.symbols.free_rust_string(ptr);     // Rust frees the memory

// After this point, 'status' is a safe JavaScript object
// and the Rust memory has been freed (no leak)
```

### Why This Pattern Exists

When Rust returns a pointer across the FFI boundary, it **deliberately leaks the memory** from Rust's perspective:

```rust
// Rust side:
#[no_mangle]
pub extern "C" fn get_worker_status() -> *mut c_char {
    let status = WorkerStatus { /* ... */ };
    let json = serde_json::to_string(&status).unwrap();
    
    // into_raw() transfers ownership OUT of Rust's memory system
    CString::new(json).unwrap().into_raw()
    // Rust's Drop trait will NOT run on this memory!
}
```

The `.into_raw()` method:
- Converts `CString` to a raw pointer
- **Prevents Rust from freeing the memory** when it goes out of scope
- Transfers ownership responsibility to the caller

Without this, Rust would free the memory immediately, and JavaScript would read garbage data (use-after-free).

---

## The Free Function

JavaScript must call back into Rust to free the memory:

```rust
// Rust side:
#[no_mangle]
pub extern "C" fn free_rust_string(ptr: *mut c_char) {
    if ptr.is_null() {
        return;
    }
    
    // SAFETY: We know this pointer came from CString::into_raw()
    // and this function is only called once per pointer
    unsafe {
        let _ = CString::from_raw(ptr);
        // CString goes out of scope here and properly frees the memory
    }
}
```

This reconstructs the `CString` from the raw pointer, which causes Rust's `Drop` trait to run and free the memory.

---

## Safety Guarantees

This pattern is safe because of three key properties:

### 1. Single-Threaded JavaScript Runtime

JavaScript (and TypeScript) runs on a single thread (ignoring Web Workers), which means:

- **No race conditions**: The read → free sequence is atomic from Rust's perspective
- **No concurrent access**: Only one piece of code can access the pointer at a time
- **Predictable execution order**: Steps always happen in sequence

### 2. One-Way Handoff

Rust follows a strict contract:

```
Rust allocates → Returns pointer → NEVER TOUCHES IT AGAIN
```

- Rust doesn't keep any references to the memory
- Rust never reads or writes to that memory after returning the pointer
- The memory is "orphaned" from Rust's perspective until `free_rust_string` is called

### 3. JavaScript Copies Before Freeing

JavaScript creates a new copy of the data before freeing:

```typescript
const ptr = this.lib.symbols.get_worker_status() as Pointer;

// Step 1: Read bytes from Rust memory into a JavaScript string
const json = new CString(ptr);  // COPY operation

// Step 2: Parse string into JavaScript objects
const status = JSON.parse(json);  // Creates new JS objects

// Step 3: Free the Rust memory
this.lib.symbols.free_rust_string(ptr);

// At this point:
// - 'status' is pure JavaScript (managed by V8/JavaScriptCore)
// - Rust memory has been freed (no leak)
// - 'ptr' is invalid (but we never use it again)
```

The `status` object is fully owned by JavaScript's garbage collector. It has no connection to the freed Rust memory.

---

## Comparison to Ruby and Python FFI

### Ruby (Magnus)

```ruby
# Ruby FFI with Magnus
result = TaskerCore::FFI.get_worker_status()
# No explicit free needed - Magnus manages memory via Rust Drop traits
```

**How it works**: Magnus creates a bridge between Ruby's GC and Rust's ownership system. When Ruby no longer references the object, Rust's `Drop` trait eventually runs.

### Python (PyO3)

```python
# Python FFI with PyO3
result = tasker_core.get_worker_status()
# No explicit free needed - PyO3 uses Python's reference counting
```

**How it works**: PyO3 wraps Rust data in `PyObject` wrappers. When Python's reference count reaches zero, the Rust data is dropped.

### TypeScript (Bun/Node FFI)

```typescript
// TypeScript FFI - manual memory management required
const ptr = lib.symbols.get_worker_status();
const json = new CString(ptr);
const status = JSON.parse(json);
lib.symbols.free_rust_string(ptr);  // MUST call explicitly
```

**Why different**: Bun and Node.js use raw C FFI (similar to ctypes in Python or FFI gem in Ruby). There's no automatic memory management bridge, so we must manually free.

**Tradeoff**: More verbose, but gives us complete control and makes memory lifetime explicit.

---

## Common Pitfalls and How We Avoid Them

### 1. Memory Leak (Forgetting to Free)

**Problem**:
```typescript
// BAD: Memory leak
const ptr = this.lib.symbols.get_worker_status();
const json = new CString(ptr);
const status = JSON.parse(json);
// Oops! Forgot to call free_rust_string(ptr)
```

**How we avoid it**: Every code path that allocates a pointer must free it. We wrap this in methods like `pollStepEvents()` that handle the complete lifecycle:

```typescript
pollStepEvents(): FfiStepEvent[] {
  const ptr = this.lib.symbols.poll_step_events() as Pointer;
  if (!ptr) {
    return [];  // No allocation, no free needed
  }
  
  const json = new CString(ptr);
  const events = JSON.parse(json);
  this.lib.symbols.free_rust_string(ptr);  // Always freed
  return events;
}
```

### 2. Double-Free

**Problem**:
```typescript
// BAD: Double-free (undefined behavior)
const ptr = this.lib.symbols.get_worker_status();
const json = new CString(ptr);
this.lib.symbols.free_rust_string(ptr);
this.lib.symbols.free_rust_string(ptr);  // CRASH! Already freed
```

**How we avoid it**: We free the pointer exactly once in each code path, and we never store pointers for reuse. Each pointer is used in a single scope and immediately freed.

### 3. Use-After-Free

**Problem**:
```typescript
// BAD: Use-after-free
const ptr = this.lib.symbols.get_worker_status();
this.lib.symbols.free_rust_string(ptr);
const json = new CString(ptr);  // CRASH! Memory is gone
```

**How we avoid it**: We always read/copy before freeing. The order is strictly: allocate → read → free.

---

## Pattern in Practice

### Example: Worker Status

```typescript
getWorkerStatus(): WorkerStatus {
  // 1. Allocate: Rust allocates memory for JSON string
  const ptr = this.lib.symbols.get_worker_status() as Pointer;
  
  // 2. Read: Copy data into JavaScript
  const json = new CString(ptr);        // Rust memory → JS string
  const status = JSON.parse(json);      // JS string → JS object
  
  // 3. Free: Deallocate Rust memory
  this.lib.symbols.free_rust_string(ptr);
  
  // 4. Return: Pure JavaScript object (safe)
  return status;
}
```

### Example: Polling Step Events

```typescript
pollStepEvents(): FfiStepEvent[] {
  const ptr = this.lib.symbols.poll_step_events() as Pointer;
  
  // Handle null pointer (no events available)
  if (!ptr) {
    return [];
  }
  
  const json = new CString(ptr);
  const events = JSON.parse(json);
  this.lib.symbols.free_rust_string(ptr);
  
  return events;
}
```

### Example: Bootstrap Worker

```typescript
bootstrapWorker(config: BootstrapConfig): BootstrapResult {
  const configJson = JSON.stringify(config);
  
  // Pass JavaScript data TO Rust (no pointer returned)
  const ptr = this.lib.symbols.bootstrap_worker(configJson) as Pointer;
  
  // Read the result
  const json = new CString(ptr);
  const result = JSON.parse(json);
  
  // Free the result pointer
  this.lib.symbols.free_rust_string(ptr);
  
  return result;
}
```

---

## Memory Lifetime Diagrams

### Successful Pattern

```
Time →

JavaScript:    [allocate ptr] → [read data] → [free ptr] → [use data]
Rust Memory:   [allocated]    → [allocated] → [freed]    → [freed]
JS Objects:    [none]         → [created]   → [exists]   → [exists]
                                  ↑
                            Data copied here
```

### Memory Leak (Anti-Pattern)

```
Time →

JavaScript:    [allocate ptr] → [read data] → [use data] → ...
Rust Memory:   [allocated]    → [allocated] → [LEAK]     → [LEAK]
JS Objects:    [none]         → [created]   → [exists]   → [exists]
                                                ↑
                                    Forgot to free! Memory leaked
```

### Use-After-Free (Anti-Pattern)

```
Time →

JavaScript:    [allocate ptr] → [free ptr] → [read ptr] → CRASH!
Rust Memory:   [allocated]    → [freed]    → [freed]
JS Objects:    [none]         → [none]     → [CORRUPT]
                                              ↑
                                    Reading freed memory!
```

---

## Best Practices

### 1. Keep Pointer Lifetime Short

```typescript
// GOOD: Pointer freed in same scope
const result = this.getWorkerStatus();

// BAD: Don't store pointers
this.statusPtr = this.lib.symbols.get_worker_status();  // Leak risk
```

### 2. Always Free in Same Method

```typescript
// GOOD: Allocate and free in same method
pollStepEvents(): FfiStepEvent[] {
  const ptr = this.lib.symbols.poll_step_events();
  if (!ptr) return [];
  
  const json = new CString(ptr);
  const events = JSON.parse(json);
  this.lib.symbols.free_rust_string(ptr);
  return events;
}

// BAD: Returning pointer for later freeing
getPtrToStatus(): Pointer {
  return this.lib.symbols.get_worker_status();  // Who will free this?
}
```

### 3. Handle Null Pointers

```typescript
// GOOD: Check for null before freeing
const ptr = this.lib.symbols.poll_step_events();
if (!ptr) {
  return [];  // No memory allocated, nothing to free
}

const json = new CString(ptr);
const events = JSON.parse(json);
this.lib.symbols.free_rust_string(ptr);
return events;
```

### 4. Document Ownership in Comments

```typescript
/**
 * Poll for step events from FFI.
 * 
 * MEMORY: This function manages the lifetime of the pointer returned
 * by poll_step_events(). The pointer is freed before returning.
 */
pollStepEvents(): FfiStepEvent[] {
  // ...
}
```

---

## Testing Memory Safety

### Rust Tests

Rust's test suite can verify FFI functions don't leak:

```rust
#[test]
fn test_status_no_leak() {
    let ptr = get_worker_status();
    assert!(!ptr.is_null());
    
    // Manually free to ensure it works
    free_rust_string(ptr);
    
    // If we had a leak, tools like valgrind or AddressSanitizer
    // would catch it
}
```

### TypeScript Tests

TypeScript tests verify proper usage:

```typescript
test('status retrieval frees memory', () => {
  const runtime = new BunTaskerRuntime();
  
  // This should not leak - memory freed internally
  const status = runtime.getWorkerStatus();
  
  expect(status.running).toBeDefined();
  
  // Call multiple times to stress test
  for (let i = 0; i < 100; i++) {
    runtime.getWorkerStatus();
  }
  // If we leaked, we'd have 100 leaked strings
});
```

### Leak Detection Tools

- **Valgrind** (Linux): Detects memory leaks in Rust code
- **AddressSanitizer**: Detects use-after-free and double-free
- **Process memory monitoring**: Track RSS growth over time

---

## When in Doubt

**Golden Rule**: Every `*mut c_char` pointer returned by a Rust FFI function must have a corresponding `free_rust_string()` call in the TypeScript code, executed exactly once per pointer, after all reads are complete.

If you see a pattern like:
```typescript
const ptr = this.lib.symbols.some_function();
```

Ask yourself:
1. Does this return a pointer to allocated memory? (Check Rust signature)
2. Am I reading the data before freeing?
3. Am I freeing exactly once?
4. Am I never using `ptr` after freeing?

If the answer to all is "yes", you're following the pattern correctly.

---

## References

- **Rust FFI Guidelines**: https://doc.rust-lang.org/nomicon/ffi.html
- **Bun FFI Documentation**: https://bun.sh/docs/api/ffi
- **Node.js ffi-napi**: https://github.com/node-ffi-napi/node-ffi-napi
- **TAS-101**: TypeScript FFI Bridge implementation
- **docs/worker-crates/patterns-and-practices.md**: General worker patterns
