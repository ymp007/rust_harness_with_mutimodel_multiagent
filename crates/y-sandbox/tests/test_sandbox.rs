use std::time::Duration;
use y_sandbox::{HandleGuardTable, Tier1WasmSandbox, Tier2OsSandbox};

#[test]
fn handle_guard_can_outlive_its_owner_without_dangling_pointer() {
    let table = HandleGuardTable::new(1);
    let guard = table.acquire().unwrap();
    drop(table);
    drop(guard);
}

#[test]
fn test_handle_guard_table_cap_enforcement() {
    let table = HandleGuardTable::new(2);
    assert_eq!(table.active_handles(), 0);

    let guard1 = table.acquire().expect("Guard 1 acquire failed");
    assert_eq!(table.active_handles(), 1);

    let _guard2 = table.acquire().expect("Guard 2 acquire failed");
    assert_eq!(table.active_handles(), 2);

    // 3rd acquisition should fail
    let guard3 = table.acquire();
    assert!(guard3.is_err(), "Should fail when capacity is reached");

    drop(guard1);
    assert_eq!(table.active_handles(), 1);

    // Now can acquire again
    let _guard4 = table.acquire().expect("Guard 4 acquire failed");
    assert_eq!(table.active_handles(), 2);
}

#[test]
fn test_tier1_wasm_fuel_metering() {
    let mut wasm = Tier1WasmSandbox::new(1000, 16);
    assert_eq!(wasm.fuel_remaining(), 1000);

    let res = wasm.execute_guest(300, b"AST Analysis Payload").unwrap();
    assert_eq!(res, b"AST Analysis Payload");
    assert_eq!(wasm.fuel_remaining(), 700);

    // Exceed fuel
    let err = wasm.execute_guest(800, b"Heavy Operation");
    assert!(err.is_err());

    // Reset requires exclusive access and restores the full quota.
    wasm.reset_fuel();
    assert_eq!(wasm.fuel_remaining(), 1000);
}

#[test]
fn tier1_memory_ceiling_is_reported_not_enforced() {
    // No Wasm engine is bound, so this records intent only. The test asserts
    // the accessor exists precisely so a future engine binding has to revisit
    // the claim rather than inherit it silently.
    let wasm = Tier1WasmSandbox::new(1000, 256);
    assert_eq!(wasm.max_memory_pages(), 256);
    // A guest cannot grow memory because nothing executes.
    assert!(wasm.execute_guest(1, b"x").is_ok());
}

#[tokio::test]
async fn test_tier2_os_sandbox_env_isolation() {
    let temp_dir = std::env::temp_dir();
    let sandbox = Tier2OsSandbox::new(&temp_dir, Duration::from_secs(5));

    #[cfg(windows)]
    let out = sandbox
        .execute_command("cmd.exe", &["/C", "echo Y_CODE_ISOLATED"], None)
        .await
        .expect("Sandboxed command execution failed");

    #[cfg(not(windows))]
    let out = sandbox
        .execute_command("echo", &["Y_CODE_ISOLATED"], None)
        .await
        .expect("Sandboxed command execution failed");

    assert!(out.contains("Y_CODE_ISOLATED"));
    assert_eq!(sandbox.active_handles(), 0);
}

#[test]
fn tier2_reports_rather_than_claims_its_memory_limit() {
    // `memory_limit_bytes` is recorded for reporting; this crate does not
    // enforce it. See the note on `Tier2OsSandbox`.
    let sandbox = Tier2OsSandbox::new(std::env::temp_dir(), Duration::from_secs(5));
    assert_eq!(sandbox.memory_limit_bytes, 512 * 1024 * 1024);
}
