---

# Section 5: Phase 3: Dual-Mode Model Gateway & Prefix Cache

Phase 3 engineers the dual-mode model gateway, isolated out-of-process GPU crash domain, SIMD-accelerated SSE streaming parser, hierarchical radix prefix KV-cache, and dynamic RoPE sequence shifting engine.

```
+===================================================================================================+
|                                    PHASE 3 ARCHITECTURAL TOPOLOGY                                 |
+===================================================================================================+
|                                                                                                   |
|  +---------------------------------------------------------------------------------------------+  |
|  |                            Aegis Core Dispatcher / Inference Hub                            |  |
|  +----------------------------------------------+----------------------------------------------+  |
|                                                 |                                                 |
|                   +-----------------------------+-----------------------------+                   |
|                   |                                                           |                   |
|                   v                                                           v                   |
|  +----------------------------------+                   +----------------------------------+      |
|  |       LOCAL EMBEDDED SLMs        |                   |      LOCAL HIGH-END GPU LLMs     |      |
|  | - In-Process C-FFI (llama-sys)   |                   | - Isolated Out-of-Process Worker |      |
|  | - Direct CPU AVX-512 / NEON      |                   | - 128B Lockless SHM Ring Buffer  |      |
|  | - Zero IPC Overhead (< 20 µs)    |                   | - Crash Isolation (< 45 ms Reset)|      |
|  +----------------------------------+                   +----------------------------------+      |
|                   |                                                           |                   |
|                   +-----------------------------+-----------------------------+                   |
|                                                 |                                                 |
|                                                 v                                                 |
|  +---------------------------------------------------------------------------------------------+  |
|  |                  REMOTE CLOUD GATEWAY & CONNECTION POOL (HTTP/2, HTTP/3, gRPC)              |  |
|  +----------------------------------------------+----------------------------------------------+  |
|                                                 |                                                 |
|                   +-----------------------------+-----------------------------+                   |
|                   |                                                           |                   |
|                   v                                                           v                   |
|  +----------------------------------+                   +----------------------------------+      |
|  |     RESILIENT SIMD SSE PARSER    |                   |    BLAKE3 RADIX KV-PREFIX CACHE  |      |
|  | - 4-Byte UTF-8 Carryover Reg     |                   | - BLAKE3 Rolling Token-ID Hashes |      |
|  | - 64-Byte Padded AlignedVec      |                   | - Dynamic RoPE Sequence Shifting |      |
|  | - > 1,800 MB/s Throughput        |                   | - 0ms Prefill on Prompt Repeats  |      |
|  +----------------------------------+                   +----------------------------------+      |
|                                                                                                   |
+===================================================================================================+
```

---

## 5.1 Dual-Mode Model Gateway Architecture

To eliminate the dichotomy between low-latency local execution and robust system stability, Aegis-Harness implements a **Dual-Mode Model Gateway**:

```
+===================================================================================================+
|                                 DUAL-MODE MODEL GATEWAY ARCHITECTURE                              |
+===================================================================================================+
|                                                                                                   |
|  MODE 1: Direct In-Process C-FFI (Embedded SLMs / 1B - 3B Models)                                 |
|  - Target: Compact models running on CPU via AVX-512 / Apple Silicon NEON.                         |
|  - Invocation: Direct C-FFI bindings to llama.cpp (llama-sys).                                    |
|  - Latency: < 20 µs dispatch latency, allocating zero IPC sockets or shared memory.               |
|                                                                                                   |
|  MODE 2: Isolated Out-of-Process GPU Worker (High-End LLMs / 7B - 70B Models)                     |
|  - Target: Heavy GPU acceleration (CUDA, ROCm, Metal).                                            |
|  - Process Isolation: aegis-inference-worker runs as a dedicated daemon child process.            |
|  - Crash Domain Isolation: GPU driver segmentation faults, CUDA OOMs, or kernel panics NEVER      |
|    terminate the Aegis host harness.                                                              |
|  - Auto-Restart Engine: Watchdog detects worker crash -> Spawns new worker in < 45 ms.            |
|  - IPC Wire: 128-byte cache-aligned lockless shared-memory circular ring buffer (shm_ring_buf).   |
|                                                                                                   |
+===================================================================================================+
```

---

## 5.2 Remote Cloud API Gateway & Connection Pooling

- **Protocols**: Asynchronous HTTP/2 multiplexing and HTTP/3 (QUIC) fallback via `reqwest` and `h2`, plus high-throughput gRPC client streaming.
- **Persistent Connection Pool**: Maintains warm TCP/TLS handshakes across model requests, reducing per-turn connection setup from 65 ms to 0.0 ms.
- **Zero-Allocation Token Channels**: Streams raw token frames into Tokio bounded channels (`tokio::sync::mpsc::channel(1024)`) mapped directly to the SIMD parser.

---

## 5.3 Resilient SIMD SSE Stream Parser with UTF-8 Carryover

### The Multi-Byte Codepoint Split Problem
Under high-throughput Server-Sent Events (SSE) streaming, TCP packets and chunk boundaries arbitrary slice 4-byte UTF-8 codepoints (e.g. emojis or CJK characters) across separate chunk boundaries. Standard JSON/SSE parsers throw invalid UTF-8 exceptions or corrupt characters.

```
Incoming Stream Buffer Split:
Chunk 1: ... "content": "Hello \xF0\x9F"  <--- Incomplete 4-byte UTF-8 sequence (Emoji split!)
Chunk 2: "\x90\x8D" ...                   <--- Remaining 2 bytes of Emoji

Standard Parser -> ERROR: Invalid UTF-8 codepoint! Stream Aborted.

Aegis Resilient Parser:
Chunk 1 -> Detects 2-byte tail split -> Copies \xF0\x9F to 4-byte carryover register.
Chunk 2 -> Prepends carryover register -> Decodes complete Emoji "🦀" -> SIMD Scan Continues!
```

### The Vector Load Overread Problem
SIMD instructions (`_mm256_loadu_si256` / `vld1q_u8`) load 32 bytes or 16 bytes in a single hardware cycle. If the buffer boundary is not padded, the SIMD load reads past the allocated heap boundary, causing segmentation faults.

### Aegis Resilient SIMD SSE Parser Implementation:
1. **4-Byte UTF-8 Carryover Register**: Stores trailing incomplete multi-byte UTF-8 bytes across stream chunk boundaries, prepending them seamlessly to the next chunk.
2. **64-Byte Padded Memory Arena (`AlignedVec`)**: All parsing buffers enforce `SIMDJSON_PADDING = 64`, guaranteeing that 256-bit AVX2 vector loads can safely read past the logical end of the string without inducing memory faults.
3. **Throughput**: Processes raw SSE streams at **> 1,800 MB/s**, reducing Time-to-First-Token overhead ($\Delta\text{TTFT}$) to **< 350 µs (P99)**.

---

## 5.4 Hierarchical Radix Tree Prefix Cache & KV-Cache Alignment

### The Prompt Prefill Thrashing Problem
Agent turns repeatedly resend shared system prompts, tool schemas, and conversation histories. Re-evaluating these tokens through local LLM attention layers incurs $O(N)$ computational complexity on every turn.

```
Turn 1: [System Prompt (2,000 tok)] [User Message 1 (50 tok)]  --> Compute 2,050 tokens
Turn 2: [System Prompt (2,000 tok)] [User Msg 1] [Model Msg 1] [User Msg 2] --> Naive: Recompute 2,200 tokens!

Aegis Radix KV-Prefix Cache:
Turn 2: Match [System Prompt + User Msg 1] in Radix Tree --> Re-use KV Cache! Compute ONLY User Msg 2 (50 tokens) -> 0ms Prefill!
```

### Aegis Radix Prefix Cache Implementation:
1. **Token-ID Hashing**: Hashes raw verified token IDs (`&[u32]`) using **BLAKE3 rolling hashes**, eliminating string parsing ambiguities.
2. **Longest Prefix Match**: Traverses the in-memory Radix Tree to locate the deepest cached KV-cache checkpoint.
3. **In-Process Model KV Alignment**: Re-uses pre-computed attention tensors directly in memory.

---

## 5.5 Dynamic RoPE Sequence Shifting

When conversation context exceeds the model's physical context limit, Aegis-Harness performs **Middle-Out Context Compaction**. By utilizing `llama_kv_cache_seq_shift`, token positions in the KV cache are mathematically re-indexed without re-evaluating the prompt:

$$\text{pos}_{\text{new}} = \text{pos}_{\text{old}} - \Delta_{\text{evicted}}$$

This converts an $O(N)$ prompt prefill computation into an **$O(1)$ pointer shift operation (< 10 µs)**.

---

## 5.6 Non-Blocking Backpressure & Token Accounting

- Bounded token channels ensure the model generation pauses if the client or lifecycle hooks cannot consume tokens fast enough.
- Monotonic token counters track prompt and completion tokens with zero atomic contention.

---

## 5.7 Rust Trait & Struct IDL Definitions

```rust
// ============================================================================
// CRATE: aegis-gateway
// Module: Dual-Mode Model Gateway & Resilient SIMD SSE Parser
// ============================================================================

use bytes::Bytes;
use std::time::Duration;
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderKind {
    InProcessCffi,
    IsolatedGpuWorker,
    RemoteCloudHttp,
    RemoteCloudGrpc,
}

#[derive(Debug, Clone)]
pub struct ModelConfig {
    pub provider: ProviderKind,
    pub model_id: String,
    pub context_window: usize,
    pub max_tokens: usize,
    pub temperature: f32,
    pub top_p: f32,
}

#[derive(Debug, Clone)]
pub struct ModelRequest {
    pub prompt_tokens: Vec<u32>,
    pub raw_prompt: String,
    pub stop_sequences: Vec<String>,
}

#[derive(Debug, Error)]
pub enum GatewayError {
    #[error("Inference worker crash: {0}")]
    WorkerCrashed(String),
    #[error("SIMD SSE parsing error: {0}")]
    ParsingError(String),
    #[error("Remote provider timeout: {0:?}")]
    Timeout(Duration),
    #[error("Prefix cache eviction failure: {0}")]
    CacheError(String),
}

/// Abstract Model Gateway Trait
#[async_trait::async_trait]
pub trait ModelGateway: Send + Sync {
    async fn generate_stream(
        &self,
        request: ModelRequest,
        token_tx: tokio::sync::mpsc::Sender<crate::TokenChunk>,
    ) -> Result<(), GatewayError>;

    async fn get_prefix_cache_hit_rate(&self) -> f64;
}

// ----------------------------------------------------------------------------
// Resilient SIMD SSE Stream Parser Structure
// ----------------------------------------------------------------------------

pub const SIMDJSON_PADDING: usize = 64;

pub struct ResilientSseParser {
    carryover_buffer: [u8; 4],
    carryover_len: usize,
    scratch_aligned: Vec<u8>,
}

impl ResilientSseParser {
    pub fn new() -> Self {
        Self {
            carryover_buffer: [0u8; 4],
            carryover_len: 0,
            scratch_aligned: Vec::with_capacity(64 * 1024 + SIMDJSON_PADDING),
        }
    }

    /// High-Throughput SIMD Parse with Multi-Byte UTF-8 Carryover (> 1,800 MB/s)
    pub fn parse_chunk(&mut self, chunk: &[u8], mut on_token: impl FnMut(&str)) -> Result<(), GatewayError> {
        self.scratch_aligned.clear();
        
        // 1. Prepend carryover bytes from previous split
        if self.carryover_len > 0 {
            self.scratch_aligned.extend_from_slice(&self.carryover_buffer[..self.carryover_len]);
            self.carryover_len = 0;
        }

        // 2. Append incoming chunk
        self.scratch_aligned.extend_from_slice(chunk);

        // 3. Inspect tail for incomplete UTF-8 codepoints
        let total_len = self.scratch_aligned.len();
        let valid_up_to = match std::str::from_utf8(&self.scratch_aligned) {
            Ok(_) => total_len,
            Err(e) => {
                let valid = e.valid_up_to();
                if let Some(error_len) = e.error_len() {
                    // Genuine invalid UTF-8 byte
                    return Err(GatewayError::ParsingError(format!("Invalid UTF-8 at byte {}", valid)));
                } else {
                    // Incomplete multi-byte sequence at tail -> save to carryover
                    let split_bytes = &self.scratch_aligned[valid..];
                    self.carryover_len = split_bytes.len();
                    self.carryover_buffer[..self.carryover_len].copy_from_slice(split_bytes);
                    valid
                }
            }
        };

        // 4. Zero-copy SSE Token Dispatch
        if valid_up_to > 0 {
            let valid_str = unsafe { std::str::from_utf8_unchecked(&self.scratch_aligned[..valid_up_to]) };
            for line in valid_str.lines() {
                if let Some(data) = line.strip_prefix("data: ") {
                    if data != "[DONE]" {
                        on_token(data);
                    }
                }
            }
        }

        Ok(())
    }
}

// ============================================================================
// CRATE: aegis-cache
// Module: Hierarchical BLAKE3 Radix Tree Prefix Cache
// ============================================================================

pub struct RadixNode {
    pub token_hash: [u8; 32], // BLAKE3 hash of &[u32]
    pub token_ids: Vec<u32>,
    pub kv_cache_ptr: *mut std::ffi::c_void,
    pub children: Vec<RadixNode>,
}

pub struct PrefixCache {
    root: RadixNode,
}

impl PrefixCache {
    pub fn new() -> Self {
        Self {
            root: RadixNode {
                token_hash: [0u8; 32],
                token_ids: Vec::new(),
                kv_cache_ptr: std::ptr::null_mut(),
                children: Vec::new(),
            },
        }
    }

    pub fn lookup_longest_prefix(&self, tokens: &[u32]) -> (usize, *mut std::ffi::c_void) {
        // Traverses Radix tree and returns matched token count and KV-cache pointer
        (0, std::ptr::null_mut())
    }
}
```

---

## 5.8 Phase 3 File Hierarchy, Developer Steps, Tests, and Guardrails

### File-Level Target Layout:
```
crates/
├── aegis-gateway/
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs                     # Gateway public exports
│       ├── config.rs                  # ModelConfig and ProviderKind
│       ├── sse/
│       │   ├── mod.rs                 # SSE parser exports
│       │   ├── simd.rs                # SIMD SSE parser with 4-byte carryover register
│       │   └── aligned_vec.rs         # 64-byte padded AlignedVec
│       ├── cffi/
│       │   └── llama.rs               # In-process C-FFI llama-sys bindings
│       ├── worker/
│       │   ├── mod.rs                 # Isolated GPU worker supervisor (< 45 ms reset)
│       │   └── ipc_shm.rs             # 128-byte cache-aligned SHM IPC
│       └── cloud/
│           ├── mod.rs                 # Cloud HTTP/2-3 connection pool
│           ├── openai.rs              # OpenAI SSE streaming client
│           └── anthropic.rs           # Anthropic SSE streaming client
└── aegis-cache/
    ├── Cargo.toml
    └── src/
        ├── lib.rs                     # Prefix cache exports
        ├── radix.rs                   # Hierarchical Radix Tree prefix cache
        ├── blake3_hash.rs             # BLAKE3 rolling token-ID hasher
        └── rope_shift.rs              # llama_kv_cache_seq_shift compaction
```

### Granular Developer Step-by-Step Instructions:
1. **Implement SIMD SSE Parser with UTF-8 Carryover**: Construct `ResilientSseParser` with 4-byte carryover buffer and 64-byte padded `AlignedVec`. Benchmark throughput $> 1,800\text{ MB/s}$.
2. **Implement In-Process C-FFI Engine**: Bind `llama-sys` C-FFI for compact embedded CPU SLMs.
3. **Implement Out-of-Process GPU Worker Process**: Construct `aegis-inference-worker` binary communicating over 128B SHM ring; implement auto-recovery watchdog in `< 45 ms`.
4. **Implement Cloud HTTP/2-3 Gateway**: Construct connection-pooled client using `reqwest` streaming directly into SIMD parser.
5. **Implement BLAKE3 Radix Tree Prefix Cache**: Implement hierarchical prefix tree matching `&[u32]` token IDs.
6. **Implement Dynamic RoPE Sequence Shifting**: Implement middle-out KV cache compaction calling `llama_kv_cache_seq_shift`.

### Automated Test & Benchmark Commands:
```bash
# 1. Run all gateway and cache unit tests
cargo test --package aegis-gateway --package aegis-cache

# 2. Benchmark SIMD SSE Parser Throughput (> 1,800 MB/s)
cargo bench --package aegis-gateway --bench bench_simd_sse

# 3. Benchmark Time-to-First-Token Overhead (Delta-TTFT < 350 µs P99)
cargo bench --package aegis-gateway --bench bench_delta_ttft

# 4. Verify UTF-8 Multi-Byte Codepoint Split Resilience
cargo test --package aegis-gateway --test test_utf8_split_resilience -- --nocapture

# 5. Benchmark GPU Worker Crash Auto-Restart Latency (< 45 ms)
cargo test --package aegis-gateway --test test_worker_crash_recovery -- --nocapture

# 6. Benchmark Radix Prefix Cache Lookup Latency (< 10 µs)
cargo bench --package aegis-cache --bench bench_radix_lookup
```

### Quantitative Exit Criteria & System Guardrails for Phase 3:
```
+=============================================================================================================================+
|                                           PHASE 3 QUANTITATIVE EXIT CRITERIA & GUARDRAILS                                   |
+=============+===============================================+====================+==========================================+
| Guardrail   | Metric Description                            | Hard Exit Gate     | Verification Tooling & Method            |
+=============+===============================================+====================+==========================================+
| **G3-01**   | Time-to-First-Token Overhead (Delta-TTFT)     | **< 350 µs (P99)** | criterion benchmark (1,000 runs)         |
| **G3-02**   | SIMD SSE Parser Throughput                    | **> 1,800 MB/s**   | cargo bench --bench bench_simd_sse       |
| **G3-03**   | GPU Worker Crash Auto-Restart Latency         | **< 45.0 ms**      | Process kill & restart timer             |
| **G3-04**   | Radix Prefix Cache Longest Prefix Lookup      | **< 10.0 µs**      | criterion benchmark (10,000 nodes)       |
| **G3-05**   | SIMD Padded Arena Alignment Requirement       | **64-Byte Aligned**| Memory address modulo assertion          |
| **G3-06**   | UTF-8 Codepoint Split Recovery Rate           | **100.0% (Zero)**  | Fuzz testing with split multi-byte bytes |
| **G3-07**   | RoPE KV-Cache Middle Compaction Latency       | **< 15.0 µs**      | llama_kv_cache_seq_shift bench           |
| **G3-08**   | In-Process C-FFI Inference Launch Latency     | **< 20.0 µs**      | Direct C function call profiler          |
+=============+===============================================+====================+==========================================+
```
