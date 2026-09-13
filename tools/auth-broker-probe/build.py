"""Build a reflection-only Fabric probe against an already installed loader."""
import argparse
from pathlib import Path
import shutil
import subprocess
import zipfile

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
                str(root / "src/me/aomona/probe/TokenReadProbe.java")], check=True)
jar = build / "mona-token-read-probe.jar"
with zipfile.ZipFile(jar, "w", zipfile.ZIP_DEFLATED) as archive:
    archive.write(root / "fabric.mod.json", "fabric.mod.json")
    for file in sorted(classes.rglob("*.class")):
        archive.write(file, file.relative_to(classes).as_posix())
print(jar)
