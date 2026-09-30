"""Reference S3 additional checksums: CRC32, CRC32C, CRC64NVME, SHA-1 and SHA-256."""
import base64, hashlib


def _reflected_crc(data: bytes, poly: int, width: int, init: int, xorout: int) -> int:
    mask = (1 << width) - 1
    crc = init
    for byte in data:
        crc ^= byte
        for _ in range(8):
            crc = (crc >> 1) ^ poly if crc & 1 else crc >> 1
    return (crc ^ xorout) & mask


def crc32(data: bytes) -> bytes:
    return _reflected_crc(data, 0xEDB88320, 32, 0xFFFFFFFF, 0xFFFFFFFF).to_bytes(4, "big")


def crc32c(data: bytes) -> bytes:
    return _reflected_crc(data, 0x82F63B78, 32, 0xFFFFFFFF, 0xFFFFFFFF).to_bytes(4, "big")


def crc64nvme(data: bytes) -> bytes:
    return _reflected_crc(data, 0x9A6C9329AC4BC9B5, 64, 0xFFFFFFFFFFFFFFFF, 0xFFFFFFFFFFFFFFFF).to_bytes(8, "big")


ALGORITHMS = {
    "CRC32": crc32,
    "CRC32C": crc32c,
    "CRC64NVME": crc64nvme,
    "SHA1": lambda d: hashlib.sha1(d).digest(),
    "SHA256": lambda d: hashlib.sha256(d).digest(),
}


def checksum_b64(algorithm: str, data: bytes) -> str:
    return base64.b64encode(ALGORITHMS[algorithm](data)).decode()
