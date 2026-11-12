#!/usr/bin/env python3
"""Check source staging invariants; run on Linux for the real flock check."""
from pathlib import Path
import fcntl
import io
import os
import shutil
import stat
import subprocess
import tarfile
import tempfile
import time


source = Path(__file__).with_name("deploy-build.sh").read_text()
stage_script = source.split("stage_script=$(cat <<'STAGE'\n", 1)[1].split("\nSTAGE\n", 1)[0]


def archive(files):
    data = io.BytesIO()
    with tarfile.open(fileobj=data, mode="w") as tar:
        directory = tarfile.TarInfo("src")
        directory.type = tarfile.DIRTYPE
        directory.mode = 0o755
        tar.addfile(directory)
        for name, body in files.items():
            content = body.encode()
            info = tarfile.TarInfo(name)
            info.mode = 0o755 if name.endswith(".sh") else 0o644
            info.size = len(content)
            tar.addfile(info, io.BytesIO(content))
    return data.getvalue()


with tempfile.TemporaryDirectory(prefix="phone-build-check-") as temporary:
    root = Path(temporary)
    project, build = root / "phone-platform", root / "phone-platform-build"
    project.mkdir()
    build.mkdir()
    runtime = project / ".env.platform"
    runtime.write_text("fixture runtime data")
    runtime.chmod(0o600)
    environment = dict(os.environ)
    real_flock = shutil.which("flock") is not None
    if not real_flock:
        binary = root / "bin"
        binary.mkdir()
        stub = binary / "flock"
        stub.write_text("#!/bin/sh\nexit 0\n")
        stub.chmod(0o700)
        environment["PATH"] = str(binary) + ":" + environment["PATH"]

    def stage(revision, payload, script=stage_script):
        return subprocess.run(
            ["bash", "-c", script, "--", str(project), str(build), revision],
            input=payload, capture_output=True, env=environment,
        )

    first, second, failed, reverted, locked = [letter * 40 for letter in "abcde"]
    payload = archive({"src/server.py": "first", "run.sh": "fixture", "deleted.rs": "old"})
    result = stage(first, payload)
    assert result.returncode == 0, result.stderr
    context = build / f"source-{first}"
    assert stat.S_IMODE((context / "src/server.py").stat().st_mode) == 0o644
    assert stat.S_IMODE((context / "src").stat().st_mode) == 0o755
    assert stat.S_IMODE((context / "run.sh").stat().st_mode) == 0o755
    assert stage(first, archive({"src/server.py": "tampered"})).returncode == 0
    assert (context / "src/server.py").read_text() == "first"
    assert stage(second, archive({"src/server.py": "second"})).returncode == 0
    assert not (build / f"source-{second}" / "deleted.rs").exists()
    assert runtime.read_text() == "fixture runtime data"
    assert stat.S_IMODE(runtime.stat().st_mode) == 0o600
    assert stage(failed, b"invalid tar archive").returncode != 0
    assert not (build / f"source-{failed}").exists()
    assert not list(build.glob("*.tmp.*"))
    # Demonstrate that removing mode preservation recreates the non-root runtime bug.
    assert stage(reverted, payload, stage_script.replace("-xpf", "-xf")).returncode == 0
    assert stat.S_IMODE((build / f"source-{reverted}" / "src/server.py").stat().st_mode) == 0o600
    if real_flock:
        with (build / "build.lock").open("w") as lock, tempfile.TemporaryFile() as incoming:
            incoming.write(payload)
            incoming.seek(0)
            fcntl.flock(lock, fcntl.LOCK_EX)
            pending = subprocess.Popen(
                ["bash", "-c", stage_script, "--", str(project), str(build), locked],
                stdin=incoming, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
            )
            try:
                time.sleep(0.2)
                assert pending.poll() is None
                assert not (build / f"source-{locked}").exists()
            finally:
                fcntl.flock(lock, fcntl.LOCK_UN)
                stdout, stderr = pending.communicate(timeout=10)
            assert pending.returncode == 0, stderr

print("PASS: immutable source, preserved modes, runtime isolation, rollback; mode regression reproduced")
print("PASS: real concurrent staging lock" if real_flock else "SKIP: Linux flock unavailable; staging lock mocked")
