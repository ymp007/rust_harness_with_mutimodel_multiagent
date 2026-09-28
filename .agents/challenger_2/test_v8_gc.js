/**
 * Empirical Benchmark 1: V8 Garbage Collection & Event Loop Latency under Agent Workloads
 * Tests the claims of "35 ms to 85 ms GC pause spikes freezing the event loop" in Node.js V8.
 */
const { PerformanceObserver, performance, monitorEventLoopDelay } = require('perf_hooks');

const hHistogram = monitorEventLoopDelay({ resolution: 10 });
hHistogram.enable();

let gcEvents = [];
const obs = new PerformanceObserver((list) => {
    const entries = list.getEntries();
    for (const entry of entries) {
        gcEvents.push({
            kind: entry.detail ? entry.detail.kind : entry.name,
            durationMs: entry.duration,
            startTime: entry.startTime
        });
    }
});
obs.observe({ entryTypes: ['gc'] });

console.log('--- STARTING V8 GC & EVENT LOOP STRESS TEST ---');
const startMemory = process.memoryUsage();

// Simulate 100 turns of multi-turn conversation with large transcripts (100k+ tokens equivalent)
let conversationGraph = [];
let intermediateStrings = [];

const turns = 100;
const turnStart = performance.now();

for (let turn = 0; turn < turns; turn++) {
    // Generate ~1k tokens of simulated content per turn
    const userMessage = {
        role: 'user',
        content: 'Please refactor the authentication module and update the AST symbol table. ' + 'A'.repeat(2000),
        turnIndex: turn,
        timestamp: Date.now()
    };
    conversationGraph.push(userMessage);

    // Simulate model streaming 50 SSE chunks
    let assistantChunks = [];
    for (let c = 0; c < 50; c++) {
        const chunk = JSON.stringify({
            event: 'content_block_delta',
            delta: { type: 'text_delta', text: `Chunk ${c} in turn ${turn}: function authenticateToken(req, res) { return true; } ` + 'x'.repeat(200) }
        });
        const parsed = JSON.parse(chunk);
        assistantChunks.push(parsed.delta.text);
    }

    const assistantMessage = {
        role: 'assistant',
        content: assistantChunks.join(''),
        tool_calls: [
            {
                id: `call_${turn}`,
                type: 'function',
                function: {
                    name: 'file_edit',
                    arguments: JSON.stringify({ path: `src/auth_${turn}.ts`, diff: 'B'.repeat(5000) })
                }
            }
        ]
    };
    conversationGraph.push(assistantMessage);

    // Full serialization tax: stringifying entire conversation history
    const serializedHistory = JSON.stringify(conversationGraph);
    intermediateStrings.push(serializedHistory);
    if (intermediateStrings.length > 5) {
        intermediateStrings.shift(); // Evict old strings to trigger GC churn
    }

    // Double parse tool call
    for (const tc of assistantMessage.tool_calls) {
        const parsedArgs = JSON.parse(tc.function.arguments);
    }
}

const totalTimeMs = performance.now() - turnStart;
const endMemory = process.memoryUsage();

// Allow pending GC notifications to flush
setTimeout(() => {
    hHistogram.disable();
    obs.disconnect();

    console.log(`Completed ${turns} turns in ${totalTimeMs.toFixed(2)} ms`);
    console.log(`Heap Used: ${(startMemory.heapUsed / 1024 / 1024).toFixed(2)} MB -> ${(endMemory.heapUsed / 1024 / 1024).toFixed(2)} MB`);
    console.log(`Heap Total: ${(endMemory.heapTotal / 1024 / 1024).toFixed(2)} MB, RSS: ${(endMemory.rss / 1024 / 1024).toFixed(2)} MB`);
    
    console.log(`Total GC Events Captured: ${gcEvents.length}`);
    if (gcEvents.length > 0) {
        const durations = gcEvents.map(e => e.durationMs);
        const maxDuration = Math.max(...durations);
        const avgDuration = durations.reduce((a, b) => a + b, 0) / durations.length;
        const majorGCs = gcEvents.filter(e => e.durationMs > 5.0);
        
        console.log(`GC Pause Max: ${maxDuration.toFixed(3)} ms, Avg: ${avgDuration.toFixed(3)} ms`);
        console.log(`GC Events > 5ms: ${majorGCs.length}`);
        for (const mgc of majorGCs.slice(0, 5)) {
            console.log(`  - Duration: ${mgc.durationMs.toFixed(3)} ms`);
        }
    }

    console.log(`Event Loop Delay (p50): ${(hHistogram.percentile(50) / 1e6).toFixed(3)} ms`);
    console.log(`Event Loop Delay (p95): ${(hHistogram.percentile(95) / 1e6).toFixed(3)} ms`);
    console.log(`Event Loop Delay (p99): ${(hHistogram.percentile(99) / 1e6).toFixed(3)} ms`);
    console.log(`Event Loop Delay (Max): ${(hHistogram.max / 1e6).toFixed(3)} ms`);
}, 500);
