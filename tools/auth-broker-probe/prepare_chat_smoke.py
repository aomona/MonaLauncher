"""Fetch pinned official classes for the no-account, no-renderer Java signing probe."""
import argparse
from concurrent.futures import ThreadPoolExecutor
import hashlib
import json
from pathlib import Path, PurePosixPath
import urllib.request

PINS = {"1.21.8": "40b799c3a89053b9dc9ae43492814ea414ab30ae", "26.2": "bc42e43dfe43d65a2f6c2c1dbb322c75134e51fe"}


def download(url, target, digest, size=None):
    if target.is_file():
        data = target.read_bytes()
        if hashlib.sha1(data).hexdigest() == digest and (size is None or len(data) == size):
            return data
    with urllib.request.urlopen(url, timeout=60) as response:
        data = response.read((size if size is not None else 2 * 1024 * 1024) + 1)
    if hashlib.sha1(data).hexdigest() != digest or (size is not None and len(data) != size):
        raise ValueError("official artifact digest/size mismatch")
    target.parent.mkdir(parents=True, exist_ok=True)
    temporary = target.with_suffix(target.suffix + ".part")
    temporary.write_bytes(data)
    temporary.replace(target)
    return data


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("version", choices=PINS)
    parser.add_argument("root", type=Path)
    args = parser.parse_args()
    version, root = args.version, args.root.resolve()
    digest = PINS[version]
    metadata = json.loads(download(f"https://piston-meta.mojang.com/v1/packages/{digest}/{version}.json", root / f"versions/{version}/{version}.json", digest))
    client = metadata["downloads"]["client"]
    download(client["url"], root / f"versions/{version}/{version}.jar", client["sha1"], client["size"])
    def library(entry):
        artifact = entry.get("downloads", {}).get("artifact")
        if artifact is None:
            return
        path = PurePosixPath(artifact["path"])
        if path.is_absolute() or ".." in path.parts or "\\" in str(path) or ":" in str(path):
            raise ValueError("invalid artifact path")
        download(artifact["url"], root / "libraries" / path, artifact["sha1"], artifact["size"])
    with ThreadPoolExecutor(max_workers=8) as pool:
        list(pool.map(library, metadata["libraries"]))
    print(f"Verified official {version} classes: {root}")


if __name__ == "__main__":
    main()
