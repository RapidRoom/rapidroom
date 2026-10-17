#!/usr/bin/env python3
"""Check a Linux model cache, or copy verified local models into missing entries.

Python 3.11+, standard library only. Does not download files or launch RapidRoom.
"""
import argparse
import ctypes
import errno
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import stat
import sys
import tempfile

REGISTRY = Path(__file__).parent / "models/offline-registry.json"


def load_registry(path):
    registry = json.loads(path.read_text())
    if registry["version"] != 1 or not registry["assets"]:
        raise ValueError("Unsupported or empty model registry")
    names = set()
    for asset in registry["assets"]:
        name = asset["file"]
        if (not re.fullmatch(r"[A-Za-z0-9][A-Za-z0-9_.-]*", name)
                or name.casefold() in names):
            raise ValueError("Unsafe or duplicate model filename")
        names.add(name.casefold())
        if (not re.fullmatch(r"[a-f0-9]{64}", asset["sha256"])
                or type(asset["bytes"]) is not int
                or not 0 < asset["bytes"] <= 2 * 1024**3
                or type(asset["application_pin"]) is not bool):
            raise ValueError("Invalid model size, checksum or pin status: " + name)
    return registry["assets"]


def identity(path, asset):
    try:
        fd = os.open(path, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK)
    except FileNotFoundError:
        return False
    except OSError as error:
        if error.errno == errno.ELOOP:
            return False
        raise
    with os.fdopen(fd, "rb") as stream:
        info = os.fstat(stream.fileno())
        return (stat.S_ISREG(info.st_mode) and info.st_size == asset["bytes"]
                and hashlib.file_digest(stream, "sha256").hexdigest() == asset["sha256"])


def publish_new(staged, destination):
    # RENAME_NOREPLACE is atomic and also works on filesystems without hard links.
    libc = ctypes.CDLL(None, use_errno=True)
    rename = libc.renameat2
    rename.argtypes = [ctypes.c_int, ctypes.c_char_p, ctypes.c_int,
                       ctypes.c_char_p, ctypes.c_uint]
    rename.restype = ctypes.c_int
    if rename(-100, os.fsencode(staged), -100, os.fsencode(destination), 1):
        number = ctypes.get_errno()
        raise OSError(number, os.strerror(number), str(destination))


def prepare(asset, source_dir, cache):
    destination = cache / asset["file"]
    fd, name = tempfile.mkstemp(prefix=".rapidroom-offline-", dir=cache)
    staged = Path(name)
    try:
        with os.fdopen(fd, "wb") as output:
            source_fd = os.open(source_dir / asset["file"], os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK)
            with os.fdopen(source_fd, "rb") as source:
                if not stat.S_ISREG(os.fstat(source.fileno()).st_mode):
                    raise ValueError("Source is not a regular file: " + asset["file"])
                digest = hashlib.sha256()
                size = 0
                while chunk := source.read(1024 * 1024):
                    size += len(chunk)
                    if size > asset["bytes"]:
                        raise ValueError("Source grew during copy: " + asset["file"])
                    output.write(chunk)
                    digest.update(chunk)
                if size != asset["bytes"] or digest.hexdigest() != asset["sha256"]:
                    raise ValueError("Source changed during copy: " + asset["file"])
            os.fchmod(output.fileno(), 0o644)
            output.flush()
            os.fsync(output.fileno())
        created = True
        try:
            publish_new(staged, destination)
        except OSError as error:
            if error.errno != errno.EEXIST or not identity(destination, asset):
                raise
            created = False
        directory = os.open(cache, os.O_RDONLY | os.O_DIRECTORY)
        try:
            os.fsync(directory)
        finally:
            os.close(directory)
        if not identity(destination, asset):
            raise ValueError("Published model identity mismatch: " + asset["file"])
        return created
    finally:
        staged.unlink(missing_ok=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--registry", type=Path, default=REGISTRY)
    data_home = Path(os.environ.get("XDG_DATA_HOME") or Path.home() / ".local/share")
    parser.add_argument("--cache", type=Path,
                        default=data_home / "io.github.CyberTimon.RapidRAW/models")
    parser.add_argument("--source-dir", type=Path, help="Directory containing the registered filenames")
    mode = parser.add_mutually_exclusive_group()
    mode.add_argument("--prepare", action="store_true", help="Copy verified missing files only")
    mode.add_argument("--check", action="store_true", help="Read-only check (the default)")
    args = parser.parse_args()
    if sys.platform != "linux":
        parser.error("This helper currently supports Linux only")
    if args.prepare and args.source_dir is None:
        parser.error("--prepare requires --source-dir")
    if args.source_dir is not None and not args.prepare:
        parser.error("--source-dir is only used with --prepare")
    result = {"complete": False, "mode": "prepare" if args.prepare else "check",
              "cache": str(args.cache), "copied": [], "assets": [],
              "scope": "File identity only; rehearse the installed app separately."}
    try:
        assets = load_registry(args.registry)
        if args.cache.is_symlink():
            raise ValueError("Refusing a linked cache directory")
        for asset in assets:
            destination = args.cache / asset["file"]
            valid = identity(destination, asset)
            exists = destination.exists() or destination.is_symlink()
            result["assets"].append({"file": asset["file"], "bytes": asset["bytes"],
                                     "verified": valid, "application_pin": asset["application_pin"],
                                     "status": "verified" if valid else "invalid" if exists else "missing"})
        if args.prepare:
            if any(row["status"] == "invalid" for row in result["assets"]):
                raise ValueError("Invalid existing cache entry; no files replaced. Inspect it manually.")
            missing = [asset for asset, row in zip(assets, result["assets"]) if not row["verified"]]
            # Validate every needed source before creating the destination or copying anything.
            for asset in missing:
                if not identity(args.source_dir / asset["file"], asset):
                    raise ValueError("Missing or mismatched local source: " + asset["file"])
            if missing:
                required = sum(asset["bytes"] for asset in missing)
                ancestor = args.cache
                while not ancestor.exists():
                    ancestor = ancestor.parent
                if shutil.disk_usage(ancestor).free < required + 1024**3:
                    raise ValueError("Insufficient destination free space (keep 1 GiB spare)")
                args.cache.mkdir(parents=True, exist_ok=True)
                for asset in missing:
                    if prepare(asset, args.source_dir, args.cache):
                        result["copied"].append(asset["file"])
                for asset, row in zip(assets, result["assets"]):
                    valid = identity(args.cache / asset["file"], asset)
                    row.update(verified=valid, status="verified" if valid else "invalid")
        result["complete"] = all(row["verified"] for row in result["assets"])
    except (OSError, ValueError, KeyError, TypeError, AttributeError) as error:
        result["error"] = str(error)
    print(json.dumps(result, indent=2))
    return 0 if result["complete"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
