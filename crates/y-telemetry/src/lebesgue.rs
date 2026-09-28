/// Formal Lebesgue Measure DAG Latency and Energy Disaggregation Calculator.
pub struct LebesgueDagCalculator;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExecutionInterval {
    pub start_ns: u64,
    pub end_ns: u64,
}

impl ExecutionInterval {
    pub fn new(start_ns: u64, end_ns: u64) -> Self {
        assert!(end_ns >= start_ns, "End time must be >= start time");
        Self { start_ns, end_ns }
    }

    pub fn duration_ns(&self) -> u64 {
        self.end_ns - self.start_ns
    }
}

impl LebesgueDagCalculator {
    /// Computes the exact Lebesgue measure (union of time intervals) for a multi-agent DAG.
    /// T_dag = \int \mathbf{1}_{\bigcup [s_i, e_i]}(t) dt
    pub fn compute_dag_wall_time(mut intervals: Vec<ExecutionInterval>) -> u64 {
        if intervals.is_empty() {
            return 0;
        }

        // 1. Sort intervals by start time
        intervals.sort_by_key(|i| i.start_ns);

        let mut merged = Vec::with_capacity(intervals.len());
        let mut current = intervals[0];

        for next in intervals.into_iter().skip(1) {
            if next.start_ns <= current.end_ns {
                // Overlapping or contiguous interval -> merge
                current.end_ns = current.end_ns.max(next.end_ns);
            } else {
                merged.push(current);
                current = next;
            }
        }
        merged.push(current);

        // 2. Sum the Lebesgue measure of disjoint merged intervals
        merged.iter().map(|i| i.duration_ns()).sum()
    }

    /// Disaggregates total energy across agents based on active thread ratio:
    /// E_agent = E_total * (ActiveTime_agent / Sum(ActiveTime))
    pub fn disaggregate_energy(
        total_energy_joules: f64,
        agent_active_ns: u64,
        total_active_ns: u64,
    ) -> f64 {
        if total_active_ns == 0 {
            return 0.0;
        }
        total_energy_joules * (agent_active_ns as f64 / total_active_ns as f64)
    }
}
