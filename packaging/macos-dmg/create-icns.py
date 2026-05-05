#!/usr/bin/env python3
import os
import importlib.util
import struct
import sys

SCRIPT_DIR = os.path.dirname(os.path.abspath(__file__))
ICONSET_SCRIPT = os.path.join(SCRIPT_DIR, "create-iconset.py")
spec = importlib.util.spec_from_file_location("create_iconset", ICONSET_SCRIPT)
create_iconset = importlib.util.module_from_spec(spec)
spec.loader.exec_module(create_iconset)
write_png = create_iconset.write_png


CHUNKS = [
    ("icp4", 16),
    ("icp5", 32),
    ("icp6", 64),
    ("ic07", 128),
    ("ic08", 256),
    ("ic09", 512),
    ("ic10", 1024),
]


def main():
    if len(sys.argv) != 2:
        raise SystemExit("usage: create-icns.py <output.icns>")

    output = sys.argv[1]
    temp_dir = os.path.join(os.path.dirname(output), ".icon-pngs")
    os.makedirs(temp_dir, exist_ok=True)

    chunks = []
    for chunk_type, size in CHUNKS:
        png_path = os.path.join(temp_dir, f"{size}.png")
        write_png(png_path, size)
        with open(png_path, "rb") as file:
            data = file.read()
        chunks.append(chunk_type.encode("ascii") + struct.pack(">I", len(data) + 8) + data)

    body = b"".join(chunks)
    with open(output, "wb") as file:
        file.write(b"icns" + struct.pack(">I", len(body) + 8) + body)


if __name__ == "__main__":
    main()
