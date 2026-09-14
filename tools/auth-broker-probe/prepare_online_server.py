"""Prepare a pinned loopback server without accepting its EULA or starting it."""
import argparse
import hashlib
import json
from pathlib import Path
import tempfile
import urllib.request

PINS = {
    "1.21.8": ("6bce4ef400e4efaa63a13d5e6f6b500be969ef81", 57555044),
    "26.2": ("823e2250d24b3ddac457a60c92a6a941943fcd6a", 60894273),
}
PROPERTIES = """server-ip=127.0.0.1
server-port=35565
online-mode=true
enforce-secure-profile=true
enable-rcon=false
enable-query=false
max-players=2
view-distance=3
simulation-distance=3
spawn-protection=0
level-name=auth-probe-world
motd=MonaLauncher local authentication validation
"""


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("version", nargs="?", default="26.2", choices=PINS)
    version = parser.parse_args().version
    SHA1, SIZE = PINS[version]
    URL = f"https://piston-data.mojang.com/v1/objects/{SHA1}/server.jar"
    ROOT = Path(__file__).resolve().parent / "build" / f"online-server-{version}"
    ROOT.mkdir(parents=True, exist_ok=True)
    jar = ROOT / "server.jar"
    if jar.is_symlink():
        raise SystemExit("Refusing a server.jar symlink")
    if not jar.exists():
        temporary = None
        try:
            with tempfile.NamedTemporaryFile(dir=ROOT, prefix="server-", suffix=".part", delete=False) as output:
                temporary = Path(output.name)
                with urllib.request.urlopen(URL, timeout=60) as response:
                    count = 0
                    while chunk := response.read(1024 * 1024):
                        count += len(chunk)
                        if count > SIZE:
                            raise ValueError("Server download exceeds pinned size")
                        output.write(chunk)
            data = temporary.read_bytes()
            if len(data) != SIZE or hashlib.sha1(data).hexdigest() != SHA1:
                raise ValueError("Server download checksum mismatch")
            temporary.replace(jar)
        finally:
            if temporary is not None:
                temporary.unlink(missing_ok=True)
    data = jar.read_bytes()
    if len(data) != SIZE or hashlib.sha1(data).hexdigest() != SHA1:
        raise SystemExit("Existing server.jar does not match the official pin")
    properties = ROOT / "server.properties"
    if properties.exists() and properties.read_text() != PROPERTIES:
        raise SystemExit("Existing server.properties differs; inspect before changing it")
    if not properties.exists():
        properties.write_text(PROPERTIES)
    eula = ROOT / "eula.txt"
    if not eula.exists():
        eula.write_text("# https://www.minecraft.net/en-us/eula\neula=false\n")
    report = {"minecraft": version, "jarSource": URL, "jarSha1": SHA1,
              "jarSha256": hashlib.sha256(data).hexdigest(), "jarSize": len(data),
              "endpoint": "127.0.0.1:35565", "onlineMode": True, "enforceSecureProfile": True,
              "eulaAccepted": "eula=true" in eula.read_text().splitlines(), "serverStarted": False}
    (ROOT / "preparation.json").write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(report, indent=2))
    print(ROOT)


if __name__ == "__main__":
    main()
