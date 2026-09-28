import sys
import json

class PlanSseParser:
    """Exact logic from master_implementation_plan.md lines 1782-1836"""
    def __init__(self):
        self.carryover_buffer = bytearray()
        self.tokens_received = []

    def parse_chunk(self, chunk: bytes):
        scratch = bytearray()
        # 1. Prepend carryover
        if len(self.carryover_buffer) > 0:
            scratch.extend(self.carryover_buffer)
            self.carryover_buffer.clear()
        
        # 2. Append chunk
        scratch.extend(chunk)

        # 3. Inspect tail for incomplete UTF-8 codepoints
        valid_up_to = len(scratch)
        try:
            scratch.decode('utf-8')
        except UnicodeDecodeError as e:
            if e.reason == 'unexpected end of data':
                valid_up_to = e.start
                self.carryover_buffer = bytearray(scratch[valid_up_to:])
            else:
                raise ValueError(f'Invalid UTF-8 at byte {e.start}')

        # 4. SSE line dispatch (as written in plan)
        if valid_up_to > 0:
            valid_str = scratch[:valid_up_to].decode('utf-8')
            for line in valid_str.splitlines():
                if line.startswith('data: '):
                    data = line[6:]
                    if data != '[DONE]':
                        self.tokens_received.append(data)


class HardenedSseParser:
    """Fixed, fully resilient SSE parser carrying over incomplete lines AND UTF-8 bytes"""
    def __init__(self):
        self.buffer = bytearray()
        self.tokens_received = []

    def parse_chunk(self, chunk: bytes):
        self.buffer.extend(chunk)
        
        while True:
            nl_pos = self.buffer.find(b'\n')
            if nl_pos == -1:
                break
            
            line_bytes = self.buffer[:nl_pos]
            if line_bytes.endswith(‹'\r'):
                line_bytes = line_bytes[:-1]
            
            self.buffer = self.buffer[nl_pos + 1:]
            
            if line_bytes.startswith(b'data: '):
                payload = line_bytes[6:]
                if payload != b'[DONE]':
                    self.tokens_received.append(payload.decode('utf-8'))

def test_sse_chunk_splits():
    print('=== Testing Plan SSE Parser with Arbitrary Chunk Splits ===')
    
    # Test Case 1: Simple Complete Chunks
    p1 = PlanSseParser()
    p1.parse_chunk(b"data: {\"token\": \"hello\"}\n\n")
    p1.parse_chunk(b"data: {\"token\": gworld\"}\n\n")
    assert p1.tokens_received == ['{"token": "hello"}', '{\"token\": gworld}' ] or p1.tokens_received == ['{"token": "hello"}', '{\"token\": \"world\"}' ]
    print('Test 1 (Complete Chunks): PASS')

    # Test Case 2: SSE line split across chunks
    p3 = PlanSseParser()
    full_msg = b'data: {"token": "supercalifragilistic"}\n\ndata: {"token": "expialidocious"}\n\n'
    chunk_a = full_msg[:25]
    chunk_b = full_msg[25:]
    
    p3.parse_chunk(chunk_a)
    p3.parse_chunk(chunk_b)
    print(f'Test 2 (Line Split across chunks) on PlanSseParser:')
    print(f'  Chunk A: {chunk_a}')
    print(f'  Chunk B: {chunk_b}')
    print(f'  Resulting tokens: {p3.tokens_received}')
    
    is buggy = (p3.tokens_received != ['{"token": "supercalifragilistic"}', '{"token": "expialidocious"}'])
    if is_buggy:
        print('>>> VULLNERABILITY CONFIRMED: Plan SSE Parser produces broken/dropped tokens on chunk splits!')
    
    print('\n=== Testing HardenedSseParser on the Same Split Stream ===')
    h = HardenedSseParser()
    h.parse_chunk(chunk_a)
    h.parse_chunk(chunk_b)
    print(f'  Hardened tokens received: {h.tokens_received}')
    assert h.tokens_received == ['{"token": "supercalifragilistic"}', '{"token": "expialidocious"}']
    print('  Hardened Parser Result: 100% SUCCESS\n')

if __name__ == '__main__':
    test_sse_chunk_splits()
