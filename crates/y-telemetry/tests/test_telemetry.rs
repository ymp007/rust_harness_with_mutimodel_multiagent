use y_telemetry::{ExecutionInterval, HardwareEnergyMeter, LebesgueDagCalculator};

#[test]
fn test_lebesgue_dag_overlapping_intervals() {
    // 3 agents with overlapping schedules:
    // Agent 1: [100, 300] (duration 200)
    // Agent 2: [200, 400] (duration 200, overlaps with 1)
    // Agent 3: [500, 600] (duration 100, disjoint)
    let intervals = vec![
        ExecutionInterval::new(100, 300),
        ExecutionInterval::new(200, 400),
        ExecutionInterval::new(500, 600),
    ];

    // Simple sum would be 200 + 200 + 100 = 500
    // Exact Lebesgue union is [100, 400] (300) + [500, 600] (100) = 400!
    let dag_wall_time = LebesgueDagCalculator::compute_dag_wall_time(intervals);
    assert_eq!(dag_wall_time, 400);
}

#[test]
fn test_hardware_energy_meter_snapshot() {
    let meter = HardwareEnergyMeter::new();
    let snapshot = meter.capture_snapshot();

    assert!(snapshot.total_memory_mb > 0.0);
    assert!(snapshot.timestamp_ns > 0);
}
