#!/usr/bin/env python3
"""Writes checksum/vectors.json: known checksums of fixed inputs and composite multipart checksums."""
import base64, json, pathlib

from checksums import ALGORITHMS, checksum_b64

INPUTS = {
    "empty": b"",
    "check-string": b"123456789",
    "hello": b"Hello, ViGrise Storage!",
    "repeated-a": b"a" * 66560,
}


def composite(algorithm, parts):
    """Checksum of the concatenated binary part checksums, suffixed with the part count."""
    digests = b"".join(ALGORITHMS[algorithm](part) for part in parts)
    return f"{checksum_b64(algorithm, digests)}-{len(parts)}"


def main():
    out = {"checksums": [], "composite": []}
    for name, data in INPUTS.items():
        out["checksums"].append({
            "input": name, "length": len(data), "data_utf8": data.decode() if len(data) < 64 else None,
            "repeat": {"byte": "a", "length": len(data)} if len(data) >= 64 else None,
            "values": {algorithm: checksum_b64(algorithm, data) for algorithm in ALGORITHMS},
        })
    parts = [b"a" * 5, b"b" * 7, b"c" * 3]
    for algorithm in ("CRC32", "CRC32C", "SHA1", "SHA256"):
        out["composite"].append({
            "algorithm": algorithm,
            "parts": [{"byte": p.decode()[0], "length": len(p)} for p in parts],
            "value": composite(algorithm, parts),
        })
    out["full_object"] = [{
        "algorithm": algorithm,
        "parts": [{"byte": p.decode()[0], "length": len(p)} for p in parts],
        "value": checksum_b64(algorithm, b"".join(parts)),
    } for algorithm in ("CRC32", "CRC32C", "CRC64NVME")]
    path = pathlib.Path(__file__).parent / "checksum" / "vectors.json"
    path.write_text(json.dumps(out, indent=2) + "\n")
    print(f"wrote {path.name}")


if __name__ == "__main__":
    main()
