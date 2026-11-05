"""Compatibility launcher. The Rust component owns ADB, locking and recovery."""
import argparse
from pathlib import Path
import subprocess


def _varint(value):
    result = bytearray()
    while value >= 128:
        result.append((value & 127) | 128)
        value >>= 7
    result.append(value)
    return bytes(result)


def _request(root, adb, serial, apk, probe):
    body = b"\x08\x01"
    for field, value in enumerate((root, adb, serial, apk, probe), 2):
        encoded = str(value).encode("utf-8")
        body += _varint(field * 8 + 2) + _varint(len(encoded)) + encoded
    if len(body) > 16384:
        raise ValueError("Native input request exceeds its IPC budget")
    return len(body).to_bytes(4, "big") + body


def setup_input(executable, serial, root):
    from prebuilt_services import input_setup_command
    from prebuilt_phone import installer_apk, notification_helper
    root = Path(root).resolve()
    command = input_setup_command(root)
    # Sealed client resources are verified here; source development retains its
    # existing maintainer compiler path. No install decision lives in this shim.
    apk = installer_apk(root, "input")
    probe = notification_helper(root)
    result = subprocess.run(
        [*command, "--setup-phone-input"],
        input=_request(root, executable, serial, apk, probe),
        stdout=subprocess.PIPE, stderr=subprocess.PIPE,
    )
    if result.returncode:
        raise RuntimeError("Native Android input setup failed; inspect its device readiness and local setup state")
    # Version 1 has two scalar fields; an unrelated successful exit is not ready.
    body = result.stdout
    if len(body) < 4 or int.from_bytes(body[:4], "big") != len(body) - 4 or body[4:] not in (b"\x08\x01", b"\x08\x01\x10\x01"):
        raise RuntimeError("Native Android input setup returned an invalid receipt")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--adb", type=Path, required=True)
    parser.add_argument("--serial", required=True)
    parser.add_argument("--root", type=Path, required=True)
    args = parser.parse_args()
    try:
        setup_input(args.adb, args.serial, args.root)
    except (RuntimeError, OSError, ValueError) as error:
        raise SystemExit(str(error)) from None


if __name__ == "__main__":
    main()
