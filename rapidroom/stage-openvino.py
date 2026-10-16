#!/usr/bin/env python3
"""Stage an explicitly supplied, pinned Linux CPU runtime; never downloads files."""
import argparse
import hashlib
import json
from pathlib import Path
import platform
import shutil

MANIFEST = Path(__file__).parent / "openvino/runtime-manifest.json"


def verify(source, manifest):
    if manifest["schema"] != 1 or manifest["platform"] != "linux-x86_64":
        raise ValueError("unsupported OpenVINO manifest")
    expected = {row["file"] for row in manifest["files"]}
    if len(expected) != len(manifest["files"]):
        raise ValueError("duplicate runtime filenames")
    for row in manifest["files"]:
        name = row["file"]
        if Path(name).name != name or name in (".", ".."):
            raise ValueError("invalid runtime filename")
        path = source / name
        if not path.is_file() or path.is_symlink():
            raise ValueError("missing regular runtime file: " + name)
        if path.stat().st_size != row["bytes"]:
            raise ValueError("wrong runtime size: " + name)
        with path.open("rb") as stream:
            digest = hashlib.file_digest(stream, "sha256").hexdigest()
        if digest != row["sha256"]:
            raise ValueError("runtime checksum mismatch: " + name)
        with path.open("rb") as stream:
            header = stream.read(20)
        if header != bytes.fromhex("7f454c4602010100000000000000000003003e00"):
            raise ValueError("runtime is not a little-endian x86_64 ELF shared object: " + name)
        if any(value != "$ORIGIN" for value in row["rpath"]):
            raise ValueError("unexpected runtime search path: " + name)
    for alias, target in manifest["aliases"].items():
        if Path(alias).name != alias or alias in (".", "..") or target not in expected:
            raise ValueError("invalid runtime alias")
    return sum(row["bytes"] for row in manifest["files"])


def stage(source, out, manifest):
    size = verify(source, manifest)
    out.mkdir(parents=True, exist_ok=False)
    created = []
    try:
        for row in manifest["files"]:
            created.append(out / row["file"])
            shutil.copy2(source / row["file"], out / row["file"])
        for alias, target in manifest["aliases"].items():
            created.append(out / alias)
            (out / alias).symlink_to(target)
        created.append(out / "runtime-manifest.json")
        (out / "runtime-manifest.json").write_text(json.dumps(manifest, indent=2) + "\n")
        notice_dir = out / "notices"
        notice_dir.mkdir()
        for row in manifest["notices"]:
            source_notice = MANIFEST.parent / "notices" / row["file"]
            if Path(row["file"]).name != row["file"] or source_notice.stat().st_size != row["bytes"]:
                raise ValueError("invalid runtime notice: " + row["file"])
            with source_notice.open("rb") as stream:
                if hashlib.file_digest(stream, "sha256").hexdigest() != row["sha256"]:
                    raise ValueError("runtime notice checksum mismatch: " + row["file"])
            created.append(notice_dir / row["file"])
            shutil.copy2(source_notice, notice_dir / row["file"])
        # Verify the copied payload, not just its source.
        verify(out, manifest)
    except Exception:
        # Delete only files this call created in a directory it exclusively created.
        for path in reversed(created):
            path.unlink(missing_ok=True)
        if (out / "notices").exists():
            (out / "notices").rmdir()
        out.rmdir()
        raise
    return size


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source", type=Path, required=True)
    parser.add_argument("--out", type=Path)
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()
    if platform.system() != "Linux" or platform.machine() != "x86_64":
        parser.error("only Linux x86_64 is supported")
    if args.check == bool(args.out):
        parser.error("choose exactly one of --check and --out")
    manifest = json.loads(MANIFEST.read_text())
    try:
        size = verify(args.source.resolve(), manifest) if args.check else stage(args.source.resolve(), args.out.resolve(), manifest)
    except (ValueError, OSError) as error:
        parser.exit(2, str(error) + "\n")
    print(json.dumps({"verified": True, "bytes_unique": size, "mib_unique": size / 1024**2,
                      "ort": manifest["ort"], "openvino": manifest["openvino"]}))


if __name__ == "__main__":
    main()
