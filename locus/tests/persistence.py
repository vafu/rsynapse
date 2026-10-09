"""Private-bus CLI/contract/restart checks; never contacts the live session bus.

Usage: python3 locus/tests/persistence.py TARGET_DIR/debug
Build both binaries first with cargo build --bins.
"""
import json
import os
from pathlib import Path
import select
import signal
import subprocess
import sys
import tempfile
import xml.etree.ElementTree as ET


def key(kind, identifier):
    return {"type": "stable-key", "kind": kind, "id": identifier}


with tempfile.TemporaryDirectory(prefix="rsynapse-locus-test-") as directory:
    binaries = Path(sys.argv[1]).resolve()
    bus = subprocess.Popen(
        ["dbus-daemon", "--session", "--nofork", "--print-address=1"],
        stdout=subprocess.PIPE, text=True,
    )
    service = None
    try:
        env = dict(os.environ, DBUS_SESSION_BUS_ADDRESS=bus.stdout.readline().strip(),
                   LOCUS_RELATIONS_PATH=str(Path(directory) / "relations.json"),
                   RUST_LOG="locusd=info")
        path = Path(env["LOCUS_RELATIONS_PATH"])
        subject = key("org.rsynapse.niri.window.id", "42")
        target = key("arbitrary.target", "x")
        endpoint_args = [json.dumps(subject), "test.relation", json.dumps(target)]

        def start():
            process = subprocess.Popen([str(binaries / "locusd")], env=env,
                                       stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
                                       text=True)
            ready, _, _ = select.select([process.stdout], [], [], 10)
            assert ready, "service did not become ready"
            line = process.stdout.readline()
            assert "owning org.rsynapse.Locus" in line, line
            return process

        def stop(process):
            process.send_signal(signal.SIGINT)
            assert process.wait(timeout=10) == 0

        def cli(*args, success=True):
            result = subprocess.run([str(binaries / "locus"), *args], env=env,
                                    capture_output=True, text=True, timeout=10)
            if not success:
                assert result.returncode != 0, result.stdout
                return
            assert result.returncode == 0, result.stderr
            return json.loads(result.stdout)

        # Both help paths must work without a bus and identify the binary role.
        offline_env = dict(env, DBUS_SESSION_BUS_ADDRESS="unix:path=/nonexistent/locus-test-bus")
        for binary, expected_help in [("locus", "CLI for locusd"), ("locusd", "relation daemon")]:
            help_result = subprocess.run([str(binaries / binary), "--help"], env=offline_env,
                                         capture_output=True, text=True, timeout=10)
            assert help_result.returncode == 0, help_result.stderr
            assert expected_help in help_result.stdout, help_result.stdout
        invalid_daemon = subprocess.run([str(binaries / "locusd"), "--schema", "unused.yaml"],
                                        env=offline_env, capture_output=True, text=True, timeout=10)
        assert invalid_daemon.returncode != 0
        assert "takes no arguments" in invalid_daemon.stderr

        service = start()
        xml = subprocess.check_output([
            "busctl", "--user", "--xml-interface", "introspect", "org.rsynapse.Locus",
            "/org/rsynapse/Locus", "org.rsynapse.Locus.Relations1",
        ], env=env, text=True)
        interface = ET.fromstring(xml).find("interface[@name='org.rsynapse.Locus.Relations1']")
        state_signature = "((a{ss}sa{ss}a{ss}tt)b)"
        expected = {
            "SetWithPersistence": ("a{ss}sa{ss}a{ss}b", state_signature),
            "SetOneWithPersistence": ("a{ss}sa{ss}a{ss}b", state_signature),
            "SetPersistence": ("a{ss}sa{ss}b", state_signature),
            "ListWithPersistence": ("s", "a" + state_signature),
            "Set": ("a{ss}sa{ss}a{ss}", "(a{ss}sa{ss}a{ss}tt)"),
        }
        for name, signatures in expected.items():
            method = interface.find(f"method[@name='{name}']")
            actual = tuple("".join(arg.attrib["type"] for arg in method.findall("arg")
                                   if arg.attrib.get("direction", "in") == direction)
                           for direction in ["in", "out"])
            assert actual == signatures, (name, actual, signatures)

        state = cli("set", *endpoint_args, '{"source":"test"}')
        assert state["persist"] is False
        assert not path.exists()
        assert cli("list")[0] == state
        stop(service)
        service = start()
        assert cli("list") == []

        state = cli("set-one", *endpoint_args, "{}", "--persist", "true")
        assert state["persist"] is True
        assert json.loads(path.read_text())[0]["persist"] is True
        stop(service)
        service = start()
        assert cli("list") == [state]
        disabled = cli("persist", *endpoint_args, "false")
        assert disabled["record"] == state["record"]
        assert disabled["persist"] is False
        assert cli("list") == [disabled]
        assert json.loads(path.read_text()) == []
        stop(service)
        service = start()
        assert cli("list") == []
        cli("persist", *endpoint_args, "true", success=False)
        stop(service)
        service = None

        # Migration: a legacy disk record, including a window endpoint, is preserved.
        path.write_text(json.dumps([state["record"]]))
        service = start()
        assert cli("list") == [state]
        cli("persist", *endpoint_args, "false")
        assert json.loads(path.read_text()) == []
        stop(service)
        service = start()
        assert cli("list") == []
        print("Private-bus persistence contract, CLI, restart and migration checks passed")
    finally:
        if service is not None and service.poll() is None:
            service.send_signal(signal.SIGINT)
            service.wait(timeout=10)
        bus.terminate()
        bus.wait(timeout=10)
