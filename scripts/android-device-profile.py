#!/usr/bin/env python3
"""Apply a reversible display identity to this project's Android; preserve OS/ABI/root properties."""
from pathlib import Path
import argparse, datetime, hashlib, json, re, subprocess, os

ROOT = Path(__file__).resolve().parent.parent
ADB = str(ROOT / "scripts/android-adb")
FILES = ["/system/build.prop", "/product/etc/build.prop", "/vendor/build.prop",
         "/system_ext/etc/build.prop", "/odm/etc/build.prop"]
VALUES = {"brand": "Xiaomi", "manufacturer": "Xiaomi", "model": "MI 8", "device": "dipper", "name": "dipper"}

def adb(*args):
    return subprocess.check_output([ADB, *args], timeout=30)

def apply(profile):
    global VALUES
    profiles=json.loads((ROOT/"config/device-presets.json").read_text())["identities"]
    selected=next(v for v in profiles if v["id"]==profile)
    VALUES={k:selected[k] for k in ["brand","manufacturer","model","device"]};VALUES["name"]=selected["device"]
    if adb("shell", "id", "-u").strip() != b"0":
        raise SystemExit("Run android-ready.sh first; root ADB is required")
    stamp = datetime.datetime.now().strftime("%Y%m%d-%H%M%S")
    backup = ROOT / "data/android/backups" / ("identity-" + os.environ.get("AI_PHONE_SERIAL","emulator-5564") + "-" + stamp)
    backup.mkdir(parents=True, exist_ok=False)
    original = {path: adb("exec-out", "cat", path) for path in FILES}
    for path, content in original.items():
        saved = backup / path.lstrip("/")
        saved.parent.mkdir(parents=True, exist_ok=True)
        saved.write_bytes(content)
    old_names = {key: adb("shell", "settings", "get", "global", key).decode().strip()
                 for key in ["device_name", "bluetooth_name"]}
    manifest = {"files": {path: hashlib.sha256(content).hexdigest() for path, content in original.items()}, "names": old_names}
    (backup / "manifest.json").write_text(json.dumps(manifest, indent=2) + "\n")
    # A custom Android 15 build remains Android 15/userdebug/test-keys. Never claim a stock signed Xiaomi ROM.
    fingerprint = adb("shell", "getprop", "ro.build.fingerprint").decode().strip()
    suffix = fingerprint.split(":", 1)[1]
    fingerprint = VALUES["brand"].replace(" ","_")+"/"+VALUES["device"]+"/"+VALUES["device"]+":"+suffix
    adb("remount")
    for path, content in original.items():
        output = []
        for line in content.decode().splitlines():
            key, sep, value = line.partition("=")
            field = re.fullmatch(r"ro\.product\.(?:(?:system|system_ext|vendor|product|odm)\.)?(brand|manufacturer|model|device|name)", key)
            if field: value = VALUES[field.group(1)]
            elif key == "ro.build.product": value = VALUES["device"]
            elif re.fullmatch(r"ro\.(?:(?:system|system_ext|vendor|product|odm)\.)?build\.fingerprint", key): value = fingerprint
            elif key == "ro.build.description": value = re.sub(r"^\S+", VALUES["device"]+"-" + adb("shell", "getprop", "ro.build.type").decode().strip(), value)
            output.append(key + sep + value if sep else line)
        updated = ("\n".join(output) + "\n").encode()
        temporary = backup / "next.prop"
        temporary.write_bytes(updated)
        adb("push", str(temporary), "/data/local/tmp/aiphone-identity.prop")
        adb("shell", "sh", "-c", f"'cat /data/local/tmp/aiphone-identity.prop > {path} && restorecon {path}'")
        if adb("exec-out", "cat", path) != updated:
            raise SystemExit(f"Profile readback failed: {path}; originals preserved in {backup}")
    adb("shell", "rm", "/data/local/tmp/aiphone-identity.prop")
    (backup / "next.prop").unlink()
    for key in old_names:
        adb("shell", "settings", "put", "global", key, selected["name"])
    print(json.dumps({"profile": VALUES, "backup": str(backup), "restart_required": True}, ensure_ascii=False))

if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--apply", action="store_true", required=True)
    parser.add_argument("--profile", choices=["aiphone","xiaomi8"], default="xiaomi8")
    args=parser.parse_args()
    apply(args.profile)
