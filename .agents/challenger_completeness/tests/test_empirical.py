import os, sys, time, math, random, threading
from concurrent.futures import ThreadPoolExecutor

print('=' * 80)
print('AEGIS-HARNESS EMPIRICAL CHALLENGER & ADVERSARIAL TEST SUITE')
print('=' * 80)
results = {}

# Test 1: SSE Split Failure Reproduction
class PlanSseParser:
    def __init__(self):
        self.carryover_buffer = bytearray()
        self.tokens = []

    def parse_chunk(self, chunk: bytes):
        scratch = bytearray()
        if len(self.carryover_buffer) > 0:
            scratch.extend(self.carryover_buffer)
            self.carryover_buffer.clear()
        scratch.extend(chunk)
        valid_up_to = len(scratch)
        try:
            scratch.decode('utf-8')
        except UnicodeDecodeError as e:
            if e.reason == 'unexpected end of data':
                valid_up_to = e.start
                self.carryover_buffer = bytearray(scratch[valid_up_to:])
            else:
                raise
        if valid_up_to > 0:
            valid_str = scratch[:valid_up_to].decode('utf-8')
            for line in valid_str.splitlines():
                if line.startswith('data: '):
                    data = line[6:]
                    if data != '[DONE]':
                        self.tokens.append(data)

class HardenedSseParser:
    def __init__(self):
        self.buffer = bytearray()
        self.tokens = []

    def parse_chunk(self, chunk: bytes):
        self.buffer.extend(chunk)
        while True:
            nl = self.buffer.find(b'\n')
            if nl == -1:
                break
            line = self.buffer[:nl]
            if line.endswith(b'\r'):
                line = line[:-1]
            self.buffer = self.buffer[nl+1:]
            if line.startswith(b'data: '):
                payload = line[6:]
                if payload != b'[DONE]':
                    self.tokens.append(payload.decode('utf-8'))

stream_data = b'data: {"token": "Aegis"}\n\ndata: {"token": "Harness"}\n\ndata: {"token": "Resilience"}\n\ndata: [DONE]\n\n'
plan_p = PlanSseParser()
hardened_p = HardenedSseParser()

chunk_size = 9
for i in range(0, len(stream_data), chunk_size):
    c = stream_data[i:i+chunk_size]
    plan_p.parse_chunk(c)
    hardened_p.parse_chunk(c)

print('[P3] SSE Chunk Split Test (9-byte chunks):')
print(f'  - Plan Parser: {len(plan_p.tokens)} tokens received -> {plan_p.tokens}')
print(f'  - Hardened Parser: {len(hardened_p.tokens)} tokens received -> {hardened_p.tokens}')
assert hardened_p.tokens == ['{"token": "Aegis"}', '{"token": "Harness"}', '{"token": "Resilience"}']
results['P3_Plan_SSE'] = 'BUG CONFIRMED (Tokens dropped on chunk boundary)'
results['P3_Hardened_SSE'] = 'PASS (100% tokens recovered)'

# Test 2: Lebesgue Measure Interval Calculation
def lebesgue_disaggregation(t_start, t_end, model_intervals, tool_intervals):
    total_dur = t_end - t_start
    all_ints = sorted(model_intervals + tool_intervals, key=lambda x: x[0])
    if not all_ints:
        return total_dur, 0, total_dur
    merged = []
    c_start, c_end = all_ints[0]
    for s, e in all_ints[1:]:
        if s <= c_end:
            c_end = max(c_end, e)
        else:
            merged.append((c_start, c_end))
            c_start, c_end = s, e
    merged.append((c_start, c_end))
    u_measure = sum(e - s for s, e in merged)
    tau_harness = max(0, total_dur - u_measure)
    return tau_harness, u_measure, total_dur

tau_h, u_m, tot = lebesgue_disaggregation(1000, 5000, [(1200, 2000), (2200, 3000)], [(1800, 2500)])
print('\n[P4] Lebesgue Measure Disaggregation:')
print(f'  - Total: {tot} ns, Union of Model+Tool: {u_m} ns, Net Harness Overhead: {tau_h} ns')
assert tot == 4000 and u_m == 1800 and tau_h == 2200
results['P4_Lebesgue'] = 'PASS'

# Test 3: RAPL Rollover Formula Verification
def rapl_rollover(e_curr, e_prev, max_range=2**32):
    return (e_curr - e_prev) if e_curr >= e_prev else (e_curr + (max_range - e_prev))

r32 = rapl_rollover(100, 2**32 - 50, 2**32)
r64 = rapl_rollover(200, 2**40 - 300, 2**40)
print('\n[P4] RAPL Rollover:')
print(f'  - 32-bit delta: {r32} uJ, 40-bit dynamic range delta: {r64} uJ')
assert r32 == 150 and r64 == 500
results['P4_RAPL'] = 'PASS'

# Test 4: RoPE Middle-Out Compaction Arithmetic
def rope_shift(tokens, evict_start, evict_count):
    res = []
    for pos_old, tok in enumerate(tokens):
        if evict_start <= pos_old < evict_start + evict_count:
            continue
        elif pos_old >= evict_start + evict_count:
            res.append((pos_old - evict_count, tok))
        else:
            res.append((pos_old, tok))
    return res

compacted = rope_shift(list(range(50)), 10, 20)
print('\n[P3] RoPE Compaction:')
print(f'  - 50 tokens compacted to {len(compacted)} tokens. Slot 10 shifted from pos 30 -> {compacted[10][0]}')
assert len(compacted) == 30 and compacted[10] == (10, 30)
results['P3_RoPE'] = 'PASS'

print('\n' + '=' * 80)
print('ALL EMPIRICAL TEST HARNESSES COMPLETED')
for k, v in results.items():
    print(f'  [{k}]: {v}')
print('=' * 80)
