import sys, base64
with open(sys.argv[1], 'rb') as f:
    data = f.read().replace(b'\r', b'').replace(b'\n', b'').strip()
with open(sys.argv[2], 'wb') as f:
    f.write(base64.b64decode(data))
print('Decoded successfully')
