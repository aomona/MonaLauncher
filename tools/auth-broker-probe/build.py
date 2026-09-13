"""Build a reflection-only Fabric probe against an already installed loader."""
import argparse
from pathlib import Path
import shutil
import subprocess
import zipfile
import platform
import re

parser = argparse.ArgumentParser()
parser.add_argument("--loader", type=Path, required=True)
parser.add_argument("--javac", default="javac")
args = parser.parse_args()
root = Path(__file__).resolve().parent
build = root / "build"
classes = build / "classes"
if classes.exists():
    shutil.rmtree(classes)
classes.mkdir(parents=True)
subprocess.run([args.javac, "--release", "21", "-cp", str(args.loader), "-d", str(classes),
                *map(str, sorted((root / "src/me/aomona/probe").glob("*.java")))], check=True)
system = platform.system()
if system not in ("Darwin", "Linux"):
    raise SystemExit("Native memory probe supports macOS and Linux only")
settings = subprocess.run([args.javac, "-J-XshowSettings:properties", "-version"], capture_output=True, text=True, check=True)
java_home = Path(re.search(r"java.home = (.+)", settings.stderr).group(1).strip())
native_name = "libmona-memory-probe." + ("dylib" if system == "Darwin" else "so")
native = build / native_name
subprocess.run(["cc", "-shared", "-fPIC", "-Wall", "-Wextra", "-Werror", "-I" + str(java_home / "include"),
                "-I" + str(java_home / "include" / ("darwin" if system == "Darwin" else "linux")),
                str(root / "native_memory.c"), "-o", str(native)], check=True)
subprocess.run([str(java_home / "bin/java"), "-cp", str(classes), "me.aomona.probe.NativeMemoryProbe", str(native)], check=True)
jar = build / "mona-token-read-probe.jar"
with zipfile.ZipFile(jar, "w", zipfile.ZIP_DEFLATED) as archive:
    archive.write(root / "fabric.mod.json", "fabric.mod.json")
    archive.write(native, "native/" + native_name)
    for file in sorted(classes.rglob("*.class")):
        archive.write(file, file.relative_to(classes).as_posix())
print(jar)
