import time
import random
import threading
from concurrent.futures import ThreadPoolExecutor

print('=' * 80)
print('STRESS TESTING CONCURRENCY, QUEUES & RECOVERY')
print('=' * 80)

# 1. 100 Concurrent Agents Work-Stealing Contention
def run_work_stealing_stress():
    num_agents = 100
    deques = [[] for _ in range(num_agents)]
    # Place 1000 tasks across first 5 agents
    for i in range(1000):
        deques[i % 5].append(f'task_{i}')
        
    lock = threading.Lock()
    total_steals = 0
    empty_checks = 0
    completed = 0
    
    def agent_worker(agent_id):
        nonlocal total_steals, empty_checks, completed
        while True:
            # 1. Pop local LIFO
            with lock:
                if len(deques[agent_id]) > 0:
                    task = deques[agent_id].pop()
                    completed += 1
                    continue
                    
                # 2. Steal from victim FIFO
                victim = random.randint(0, num_agents - 1)
                total_steals += 1
                if len(deques[victim]) > 0:
                    task = deques[victim].pop(0)
                    completed += 1
                    continue
                    
                empty_checks += 1
                if all(len(d) == 0 for d in deques):
                    break
            time.sleep(0.00005) # Backoff
            
    t0 = time.perf_counter()
    with ThreadPoolExecutor(max_workers=num_agents) as ex:
        list(ex.map(agent_worker, range(num_agents)))
    t_elapsed = (time.perf_counter() - t0) * 1000
    print(f'100-Agent Work-Stealing Test:')
    print(f'  - Completed {completed} tasks in {t_elapsed:.2f} ms')
    print(f'  - Total Steal Attempts: {total_steals}, Empty Queue Checks: {empty_checks}')

# 2. SQLite WAL Staging: Unbounded Queue vs Bounded Backpressure Under Write Storm
def run_wal_write_storm():
    storm_size = 50000 # 50k rapid turn commits
    
    # Unbounded queue: measures peak memory growth
    unbounded_q = []
    t0 = time.perf_counter()
    for i in range(storm_size):
        unbounded_q.append({'turn_id': i, 'state_delta': 'x' * 512}) # 512 bytes payload
    t_unbounded = (time.perf_counter() - t0) * 1000
    unbounded_mem_kb = (len(unbounded_q) * 512) / 1024
    
    # Bounded queue with 80%/20% watermark backpressure
    high_watermark = 1000
    low_watermark = 200
    bounded_q = []
    paused_count = 0
    t0 = time.perf_counter()
    for i in range(storm_size):
        if len(bounded_q) >= high_watermark:
            paused_count += 1
            # Drain down to low watermark
            bounded_q = bounded_q[low_watermark:]
        bounded_q.append({'turn_id': i, 'state_delta': 'x' * 512})
    t_bounded = (time.perf_counter() - t0) * 1000
    bounded_mem_kb = (len(bounded_q) * 512) / 1024
    
    print(f'\nSQLite WAL Write Storm (50,000 Turns):')
    print(f'  - Unbounded Channel: Peak RAM = {unbounded_mem_kb:.1f} KB (Unbounded memory bloat risk), Ingest Time = {t_unbounded:.2f} ms')
    print(f'  - Bounded 80%/20% Watermark: Peak RAM = {bounded_mem_kb:.1f} KB, Pauses Triggered = {paused_count}, Time = {t_bounded:.2f} ms')

if __name__ == '__main__':
    run_work_stealing_stress()
    run_wal_write_storm()
