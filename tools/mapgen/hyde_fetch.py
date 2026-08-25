"""Extract the 1950 population grids from the HYDE 3.2.1 archive on DANS
(CC0) without downloading the whole 5.3 GB zip: read the central directory
via HTTP Range requests, then fetch just the wanted members.

The archive uses Deflate64, which Python's zipfile can't inflate — so each
member is re-wrapped as a tiny standalone zip and handed to the system
`unzip`, which can. Usage: python3 hyde_fetch.py <out_dir>
"""
import io
import os
import struct
import subprocess
import sys
import urllib.request
import zipfile

API_URL = "https://archaeology.datastations.nl/api/access/datafile/5490328"
MEMBERS = [
    "baseline/asc/1950AD_pop/popc_1950AD.asc",
    "baseline/asc/1950AD_pop/urbc_1950AD.asc",
]


class RemoteFile(io.RawIOBase):
    def __init__(self, url):
        self.pos = 0
        self.fetched = 0
        req = urllib.request.Request(url, headers={"Range": "bytes=0-0"})
        with urllib.request.urlopen(req) as r:
            self.url = r.geturl()  # pin presigned URL after redirect
            self.size = int(r.headers["Content-Range"].split("/")[-1])

    def seek(self, offset, whence=0):
        self.pos = {0: offset, 1: self.pos + offset, 2: self.size + offset}[whence]
        return self.pos

    def tell(self):
        return self.pos

    def readable(self):
        return True

    def seekable(self):
        return True

    def readinto(self, b):
        n = len(b)
        if n == 0 or self.pos >= self.size:
            return 0
        end = min(self.pos + n, self.size) - 1
        req = urllib.request.Request(self.url, headers={"Range": f"bytes={self.pos}-{end}"})
        with urllib.request.urlopen(req) as r:
            data = r.read()
        b[: len(data)] = data
        self.pos += len(data)
        self.fetched += len(data)
        return len(data)


def main(out_dir):
    rf = RemoteFile(API_URL)
    zf = zipfile.ZipFile(io.BufferedReader(rf, buffer_size=4 * 1024 * 1024))
    for member in MEMBERS:
        info = zf.getinfo(member)
        basename = os.path.basename(member)
        # Exact span of the local record: header + name + extra + data.
        rf.seek(info.header_offset)
        lh = bytearray(30)
        rf.readinto(lh)
        assert lh[:4] == b"PK\x03\x04"
        nlen, elen = struct.unpack("<HH", lh[26:30])
        total = 30 + nlen + elen + info.compress_size
        rf.seek(info.header_offset)
        rec = bytearray()
        while len(rec) < total:
            chunk = bytearray(min(8 * 1024 * 1024, total - len(rec)))
            got = rf.readinto(chunk)
            rec += chunk[:got]
        name = basename.encode()
        lh2 = bytearray(rec[:30])
        lh2[26:28] = struct.pack("<H", len(name))
        lh2[28:30] = struct.pack("<H", 0)
        body = bytes(lh2) + name + bytes(rec[30 + nlen + elen :])
        cd = (
            struct.pack(
                "<4s6H3I5H2I",
                b"PK\x01\x02", 45, 45, info.flag_bits, info.compress_type,
                0, 0, info.CRC, info.compress_size, info.file_size,
                len(name), 0, 0, 0, 0, 0o600 << 16, 0,
            )
            + name
        )
        eocd = struct.pack("<4s4H2IH", b"PK\x05\x06", 0, 0, 1, 1, len(cd), len(body), 0)
        tmp_zip = os.path.join(out_dir, basename + ".zip")
        with open(tmp_zip, "wb") as f:
            f.write(body + cd + eocd)
        subprocess.run(["unzip", "-o", "-q", basename + ".zip"], cwd=out_dir, check=True)
        os.remove(tmp_zip)
        print(f"extracted {basename} ({info.file_size / 1e6:.1f} MB)")
    print(f"transferred {rf.fetched / 1e6:.1f} MB total (archive is {rf.size / 1e9:.2f} GB)")


if __name__ == "__main__":
    main(sys.argv[1] if len(sys.argv) > 1 else "data")
