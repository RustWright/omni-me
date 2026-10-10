#!/usr/bin/env python3
"""Build the minimal RC4-40 encrypted PDF this directory's README describes.

Standard security handler, revision 2 — the shape a bank's own statement uses.
Hand-rolled because the obvious tools are not project dependencies: qpdf and
reportlab are both absent, and poppler (which is a dependency) only reads.

    python3 make-encrypted-pdf.py encrypted-statement.pdf letmein
"""
import hashlib
import struct
import sys

PAD = bytes([
    0x28, 0xBF, 0x4E, 0x5E, 0x4E, 0x75, 0x8A, 0x41, 0x64, 0x00, 0x4E, 0x56,
    0xFF, 0xFA, 0x01, 0x08, 0x2E, 0x2E, 0x00, 0xB6, 0xD0, 0x68, 0x3E, 0x80,
    0x2F, 0x0C, 0xA9, 0xFE, 0x64, 0x53, 0x69, 0x7A,
])


def pad_pw(pw: bytes) -> bytes:
    return (pw + PAD)[:32]


def rc4(key: bytes, data: bytes) -> bytes:
    s = list(range(256))
    j = 0
    for i in range(256):
        j = (j + s[i] + key[i % len(key)]) & 0xFF
        s[i], s[j] = s[j], s[i]
    out = bytearray()
    i = j = 0
    for byte in data:
        i = (i + 1) & 0xFF
        j = (j + s[i]) & 0xFF
        s[i], s[j] = s[j], s[i]
        out.append(byte ^ s[(s[i] + s[j]) & 0xFF])
    return bytes(out)


def build(path: str, user_pw: str, owner_pw: str = "owner") -> None:
    file_id = bytes.fromhex("0123456789abcdef0123456789abcdef")
    perms = -1  # allow everything; stored as a signed 32-bit int
    user, owner = user_pw.encode(), owner_pw.encode()

    # Algorithm 3: O
    o_key = hashlib.md5(pad_pw(owner)).digest()[:5]
    o_value = rc4(o_key, pad_pw(user))

    # Algorithm 2: the file encryption key
    md5 = hashlib.md5()
    md5.update(pad_pw(user))
    md5.update(o_value)
    md5.update(struct.pack("<i", perms))
    md5.update(file_id)
    file_key = md5.digest()[:5]

    # Algorithm 4: U
    u_value = rc4(file_key, PAD)

    def object_key(num: int, gen: int = 0) -> bytes:
        extra = struct.pack("<I", num)[:3] + struct.pack("<I", gen)[:2]
        return hashlib.md5(file_key + extra).digest()[:10]

    content = (
        b"BT /F1 14 Tf 72 720 Td (Globepay Chequing - Statement) Tj "
        b"0 -22 Td (Period 2024-06-01 to 2024-06-30) Tj "
        b"0 -22 Td (Closing balance 1284.00) Tj ET\n"
    )
    encrypted_content = rc4(object_key(4), content)

    objects = [
        b"<< /Type /Catalog /Pages 2 0 R >>",
        b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
        b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] "
        b"/Resources << /Font << /F1 5 0 R >> >> /Contents 4 0 R >>",
        b"<< /Length "
        + str(len(encrypted_content)).encode()
        + b" >>\nstream\n"
        + encrypted_content
        + b"\nendstream",
        b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>",
        # The Encrypt dictionary's own strings are stored unencrypted.
        b"<< /Filter /Standard /V 1 /R 2 /O <"
        + o_value.hex().encode()
        + b"> /U <"
        + u_value.hex().encode()
        + b"> /P "
        + str(perms).encode()
        + b" >>",
    ]

    out = bytearray(b"%PDF-1.4\n")
    offsets = []
    for i, body in enumerate(objects, start=1):
        offsets.append(len(out))
        out += f"{i} 0 obj\n".encode() + body + b"\nendobj\n"

    xref_at = len(out)
    out += f"xref\n0 {len(objects) + 1}\n".encode()
    out += b"0000000000 65535 f \n"
    for off in offsets:
        out += f"{off:010d} 00000 n \n".encode()
    out += (
        b"trailer\n<< /Size "
        + str(len(objects) + 1).encode()
        + b" /Root 1 0 R /Encrypt 6 0 R /ID [<"
        + file_id.hex().encode()
        + b"> <"
        + file_id.hex().encode()
        + b">] >>\nstartxref\n"
        + str(xref_at).encode()
        + b"\n%%EOF\n"
    )
    with open(path, "wb") as fh:
        fh.write(bytes(out))


if __name__ == "__main__":
    build(sys.argv[1], sys.argv[2] if len(sys.argv) > 2 else "letmein")
    print(f"wrote {sys.argv[1]}")
