"""Prepare the pinned, loopback-only 26.2 server without accepting its EULA or starting it."""
import hashlib
import json
from pathlib import Path
import tempfile
import urllib.request

ROOT = Path(__file__).resolve().parent / "build" / "online-server-26.2"
URL = "https://piston-data.mojang.com/v1/objects/823e2250d24b3ddac457a60c92a6a941943fcd6a/server.jar"
SHA1 = "823e2250d24b3ddac457a60c92a6a941943fcd6a"
SIZE = 60894273
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
    report = {"minecraft": "26.2", "jarSource": URL, "jarSha1": SHA1,
              "jarSha256": hashlib.sha256(data).hexdigest(), "jarSize": len(data),
              "endpoint": "127.0.0.1:35565", "onlineMode": True, "enforceSecureProfile": True,
              "eulaAccepted": "eula=true" in eula.read_text().splitlines(), "serverStarted": False}
    (ROOT / "preparation.json").write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(report, indent=2))
    print(ROOT)


if __name__ == "__main__":
    main()
