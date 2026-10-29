#![forbid(unsafe_code)]
use std::time::Instant;
use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, RefreshKind, System};
use y_core::{SessionId, TurnId, YCodeEngine};

/// Guardrail thresholds. Each is checked, and a failure exits non-zero, so the
/// scripts and CI can rely on the result.
const MEMORY_SLA_MB: f64 = 25.0;
const TURN_P99_SLA_MS: f64 = 50.0;
const PREFIX_LOOKUP_SLA_NS: u128 = 50_000;
const SHM_ROUNDTRIP_SLA_NS: u128 = 10_000;
const TURNS: usize = 50;

struct Failure {
    guardrail: &'static str,
    detail: String,
}

impl Failure {
    fn new(guardrail: &'static str, detail: impl Into<String>) -> Self {
        Self {
            guardrail,
            detail: detail.into(),
        }
    }
}

fn passed(guardrail: &str, measurement: &str) {
    println!("   Status:{guardrail:>27} \u{2705} PASSED");
    println!("      {measurement}");
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("==========================================================================");
    println!("          Y CODE MASTER SYSTEMS VERIFICATION & BENCHMARK SUITE            ");
    println!("==========================================================================");

    let db_path = std::env::temp_dir().join(format!("y_bench_{}.db", TurnId::new()));
    let engine = YCodeEngine::new(&db_path)?;
    let mut failures: Vec<Failure> = Vec::new();

    // 1. Idle memory footprint
    let mut sys = System::new_with_specifics(
        RefreshKind::new().with_processes(ProcessRefreshKind::new().with_memory()),
    );
    let current_pid = Pid::from_u32(std::process::id());
    sys.refresh_processes(ProcessesToUpdate::Some(&[current_pid]), true);
    let mem_kb = sys.process(current_pid).map(|p| p.memory()).unwrap_or(0);
    let mem_mb = mem_kb as f64 / (1024.0 * 1024.0);

    println!("\n[GUARDRAIL 1: IDLE MEMORY FOOTPRINT]");
    println!("   Base Engine Idle Memory:  {mem_mb:.2} MB (SLA: < {MEMORY_SLA_MB:.1} MB)");
    if mem_mb < MEMORY_SLA_MB {
        passed(
            "GUARDRAIL 1",
            &format!("{mem_mb:.2} MB < {MEMORY_SLA_MB:.1} MB"),
        );
    } else {
        println!("   Status:                   \u{274c} FAILED");
        failures.push(Failure::new(
            "1 idle memory",
            format!("{mem_mb:.2} MB exceeds the {MEMORY_SLA_MB:.1} MB budget"),
        ));
    }

    // 2. Inter-turn harness overhead
    println!("\n[GUARDRAIL 2: INTER-TURN HARNESS OVERHEAD]");
    let session_id = SessionId::new();
    let mut session = engine.create_session(session_id);
    let mut latencies_ns = Vec::with_capacity(TURNS);
    for i in 0..TURNS {
        latencies_ns.push(
            session
                .execute_turn(format!("Benchmark turn {i}"))
                .await?
                .harness_overhead_ns,
        );
    }
    latencies_ns.sort_unstable();
    let p50_ms = latencies_ns[latencies_ns.len() / 2] as f64 / 1_000_000.0;
    let p95_ms = latencies_ns[latencies_ns.len() * 95 / 100] as f64 / 1_000_000.0;
    let p99_ms = latencies_ns[latencies_ns.len() * 99 / 100] as f64 / 1_000_000.0;
    println!("   P50 Turn Overhead:        {p50_ms:.3} ms");
    println!("   P95 Turn Overhead:        {p95_ms:.3} ms");
    println!("   P99 Turn Overhead:        {p99_ms:.3} ms (SLA: < {TURN_P99_SLA_MS:.1} ms)");
    if p99_ms < TURN_P99_SLA_MS {
        passed(
            "GUARDRAIL 2",
            &format!("P99 {p99_ms:.3} ms < {TURN_P99_SLA_MS:.1} ms"),
        );
    } else {
        println!("   Status:                   \u{274c} FAILED");
        failures.push(Failure::new(
            "2 turn overhead",
            format!("P99 {p99_ms:.3} ms exceeds the {TURN_P99_SLA_MS:.1} ms budget"),
        ));
    }

    // 3. Radix prefix lookup
    println!("\n[GUARDRAIL 3: RADIX PREFIX KV-CACHE LOOKUP]");
    let cache = engine.cache();
    let seq: Vec<u32> = (0..1024).collect();
    cache.insert_sequence(&seq);
    const LOOKUPS: u128 = 10_000;
    let lookup_start = Instant::now();
    for _ in 0..LOOKUPS {
        let _ = cache.find_longest_prefix(&seq[..512]);
    }
    let avg_lookup_ns = lookup_start.elapsed().as_nanos() / LOOKUPS;
    println!("   Avg Prefix Lookup:        {avg_lookup_ns} ns (SLA: < {PREFIX_LOOKUP_SLA_NS} ns)");
    if avg_lookup_ns < PREFIX_LOOKUP_SLA_NS {
        passed(
            "GUARDRAIL 3",
            &format!("{avg_lookup_ns} ns < {PREFIX_LOOKUP_SLA_NS} ns"),
        );
    } else {
        println!("   Status:                   \u{274c} FAILED");
        failures.push(Failure::new(
            "3 prefix lookup",
            format!("{avg_lookup_ns} ns exceeds the {PREFIX_LOOKUP_SLA_NS} ns budget"),
        ));
    }

    // 4. Ring round-trip
    println!("\n[GUARDRAIL 4: RING BUFFER ROUND-TRIP]");
    let mut ring = y_core::ShmRingBuffer::new(4096);
    let payload = b"BENCHMARK_PAYLOAD_32_BYTES_1234";
    let mut buf = [0u8; 64];
    const ROUNDTRIPS: u128 = 10_000;
    let shm_start = Instant::now();
    for _ in 0..ROUNDTRIPS {
        ring.enqueue(payload)?;
        let _ = ring.dequeue_into_private(&mut buf)?;
    }
    let avg_shm_ns = shm_start.elapsed().as_nanos() / ROUNDTRIPS;
    println!("   Avg Ring Roundtrip:       {avg_shm_ns} ns (SLA: < {SHM_ROUNDTRIP_SLA_NS} ns)");
    if avg_shm_ns < SHM_ROUNDTRIP_SLA_NS {
        passed(
            "GUARDRAIL 4",
            &format!("{avg_shm_ns} ns < {SHM_ROUNDTRIP_SLA_NS} ns"),
        );
    } else {
        println!("   Status:                   \u{274c} FAILED");
        failures.push(Failure::new(
            "4 ring roundtrip",
            format!("{avg_shm_ns} ns exceeds the {SHM_ROUNDTRIP_SLA_NS} ns budget"),
        ));
    }

    engine.shutdown();
    let _ = std::fs::remove_file(db_path);

    println!("\n==========================================================================");
    if failures.is_empty() {
        println!(
            "          ALL GUARDRAILS PASSED ({}/4)                                          ",
            4
        );
        println!("==========================================================================");
        return Ok(());
    }
    println!(
        "          {} OF 4 GUARDRAILS FAILED                                          ",
        failures.len()
    );
    println!("==========================================================================");
    for failure in &failures {
        eprintln!("GUARDRAIL {} FAILED: {}", failure.guardrail, failure.detail);
    }
    std::process::exit(1);
}
