"""Print missing direct imports when a native Windows test fails before its harness."""
import ctypes
import pathlib
import struct
import sys


def check(path):
    data = path.read_bytes()
    pe = struct.unpack_from('<I', data, 0x3c)[0]
    count = struct.unpack_from('<H', data, pe + 6)[0]
    optional_size = struct.unpack_from('<H', data, pe + 20)[0]
    optional = pe + 24
    is64 = struct.unpack_from('<H', data, optional)[0] == 0x20b
    directories = optional + (112 if is64 else 96)
    imports = struct.unpack_from('<I', data, directories + 8)[0]
    sections = []
    for index in range(count):
        section = optional + optional_size + index * 40
        virtual_size, address, raw_size, raw = struct.unpack_from('<IIII', data, section + 8)
        sections.append((address, max(virtual_size, raw_size), raw))

    def offset(rva):
        for address, size, raw in sections:
            if address <= rva < address + size:
                return raw + rva - address
        return rva

    def string(rva):
        begin = offset(rva)
        return data[begin:data.index(b'\0', begin)].decode('ascii')

    descriptor = offset(imports)
    checked = 0
    failures = []
    while any(data[descriptor:descriptor + 20]):
        original, _, _, name, first = struct.unpack_from('<IIIII', data, descriptor)
        dll = string(name)
        try:
            module = ctypes.WinDLL(dll)
        except OSError as error:
            failures.append(f'{dll}: cannot load: {error}')
            descriptor += 20
            continue
        thunk = offset(original or first)
        size = 8 if is64 else 4
        mask = 1 << (size * 8 - 1)
        while True:
            entry = int.from_bytes(data[thunk:thunk + size], 'little')
            if not entry:
                break
            symbol = entry & 0xffff if entry & mask else string(entry + 2)
            checked += 1
            try:
                module[symbol]
            except AttributeError:
                failures.append(f'{dll}: missing {symbol}')
            thunk += size
        descriptor += 20
    print(f'{path.name}: checked {checked} imports; {len(failures)} missing')
    for failure in failures:
        print(failure)


if __name__ == '__main__':
    if sys.platform != 'win32':
        raise SystemExit('Windows only')
    for filename in pathlib.Path('src-tauri/target/debug/deps').glob('wallta_lib-*.exe'):
        check(filename)
