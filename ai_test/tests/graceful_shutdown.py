"""Real SIGINT/SIGTERM tests, each in its own temporary PostgreSQL database."""

import os
from pathlib import Path
import re
import signal
import struct
import subprocess
import time
from urllib.parse import urlsplit, urlunsplit


ROOT = Path(os.environ["ROOT"])
ARTIFACTS = Path(os.environ["ARTIFACTS"])
ARTIFACTS.mkdir(parents=True, exist_ok=True)
APP = os.environ["APP_BIN"]
BASE_ENV = os.environ.copy()


def psql(sql, database):
    env = dict(BASE_ENV, PGDATABASE=database)
    result = subprocess.run(
        ["psql", "-X", "-v", "ON_ERROR_STOP=1", "-At", "-c", sql],
        env=env, capture_output=True, text=True, check=True,
    )
    return result.stdout.strip()


def varint(number):
    data = bytearray()
    while number > 127:
        data.append((number & 127) | 128)
        number >>= 7
    data.append(number)
    return bytes(data)


def slot_capture(path, count):
    with path.open("wb") as capture:
        for number in range(900100000, 900100000 + count):
            slot = b"\x08" + varint(number) + b"\x10" + varint(number - 1) + b"\x18\x02"
            update = b"\x1a" + varint(len(slot)) + slot
            capture.write(struct.pack("<I", len(update)) + update)


def wait_for_log(process, path, text):
    deadline = time.monotonic() + 30
    while time.monotonic() < deadline:
        if text in path.read_text():
            return
        if process.poll() is not None:
            raise AssertionError(f"process exited before readiness: {path.read_text()}")
        time.sleep(0.01)
    raise AssertionError(f"process never reached readiness: {path}")


def check_signal(label, signum, count):
    database = f"shutdown_{os.getpid()}_{label}"
    assert re.fullmatch(r"shutdown_\d+_[a-z]+", database)
    psql(f'CREATE DATABASE "{database}"', "postgres")
    process = None
    try:
        capture = ARTIFACTS / f"shutdown-{label}.bin"
        slot_capture(capture, count)
        url = urlsplit(BASE_ENV["DATABASE_URL"])
        env = dict(BASE_ENV, DATABASE_URL=urlunsplit(url._replace(path="/" + database)),
                   REPLAY=str(capture), REPLAY_LOOP="1")
        env.pop("REPLAY_DLQ", None)
        env.pop("PRINT_EVENTS", None)
        log = ARTIFACTS / f"shutdown-{label}.log"
        with log.open("w") as output:
            process = subprocess.Popen([APP], env=env, stdout=output, stderr=subprocess.STDOUT)
            # A full pass ensures there are accepted updates to verify, even if
            # the first startup signal arrives unusually quickly on this host.
            wait_for_log(process, log, "REPLAY pass 1 done:")
            process.send_signal(signum)
            assert process.wait(timeout=30) == 0, log.read_text()
        text = log.read_text()
        assert "Shutdown complete:" in text, text
        assert "panicked" not in text and "Indexer failed:" not in text, text
        match = re.search(r"REPLAY stopped: (\d+) updates accepted", text)
        assert match, text
        accepted = int(match.group(1))
        assert accepted >= count
        expected = min(accepted, count)  # Looping replays must remain idempotent.
        saved = int(psql("SELECT count(*) FROM slots", database))
        assert saved == expected, f"{label}: accepted={accepted}, saved={saved}, expected={expected}"
        assert psql("SELECT count(*) FROM dead_letter_batches", database) == "0"
        print(f"PASS: {label}: exit 0, all {expected} unique accepted slots persisted", flush=True)
    finally:
        if process is not None and process.poll() is None:
            process.kill()
            process.wait(timeout=5)
        psql(f'DROP DATABASE "{database}" WITH (FORCE)', "postgres")


def check_startup_signal():
    env = dict(BASE_ENV, DATABASE_URL="postgres://unused:unused@127.0.0.1:1/unused")
    env.pop("REPLAY_DLQ", None)
    log = ARTIFACTS / "shutdown-startup.log"
    with log.open("w") as output:
        process = subprocess.Popen([APP], env=env, stdout=output, stderr=subprocess.STDOUT)
        try:
            time.sleep(0.2)
            assert process.poll() is None, "expected startup to be waiting for the unavailable database"
            process.send_signal(signal.SIGTERM)
            assert process.wait(timeout=5) == 0, log.read_text()
        finally:
            if process.poll() is None:
                process.kill()
                process.wait(timeout=5)
    assert "Shutdown requested during startup" in log.read_text()
    print("PASS: SIGTERM interrupts database startup cleanly", flush=True)


check_signal("interrupt", signal.SIGINT, 50000)
check_signal("terminate", signal.SIGTERM, 50000)
check_signal("empty", signal.SIGINT, 0)
check_startup_signal()
