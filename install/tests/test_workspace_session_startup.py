"""Exercise the installed unit's readiness gate on isolated user-manager units."""

import os
from pathlib import Path
import shlex
import socket
import subprocess
import sys
import tempfile
import uuid


TEMPLATE = Path(__file__).resolve().parents[1] / "systemd/user/rsynapse-workspace-dbus.service.in"


def systemctl(*args, check=True):
    return subprocess.run(
        ["systemctl", "--user", *args], check=check, capture_output=True, text=True,
        timeout=20,
    )


def worker(mode, control, environment):
    with socket.socket(socket.AF_UNIX) as stream:
        stream.settimeout(15)
        stream.connect(control)
        if mode == "projection":
            assert os.environ.get("RSYNAPSE_STARTUP_READY") == "published", (
                "projection inherited its environment before session publication"
            )
        stream.sendall(mode.encode())
        if mode == "ready":
            assert stream.recv(16) == b"publish"
            Path(environment).write_text("RSYNAPSE_STARTUP_READY=published\n")


def exercise():
    # Only these unique fixture units are linked/stopped/removed. EnvironmentFile
    # models the manager's environment at exec without mutating session variables.
    prefix = "rsynapse-startup-test-" + uuid.uuid4().hex
    session = prefix + ".target"
    pre = prefix + "-pre.target"
    ready = prefix + "-ready.service"
    projection = prefix + "-projection.service"
    names = [projection, session, ready, pre]
    script = shlex.quote(str(Path(__file__).resolve()))
    python = shlex.quote(sys.executable)

    with tempfile.TemporaryDirectory(prefix="rsynapse-startup-") as temporary:
        directory = Path(temporary)
        control = directory / "control"
        environment = directory / "environment"
        command = f"{python} {script} --worker"
        (directory / pre).write_text("[Unit]\nDescription=Startup fixture pre-session\n")
        (directory / session).write_text(
            "[Unit]\nDescription=Startup fixture session\n"
            f"Wants={ready} {projection}\nAfter={ready}\n"
        )
        (directory / ready).write_text(
            "[Unit]\nDescription=Startup fixture environment publisher\n"
            f"After={pre}\nBefore={session}\n[Service]\nType=oneshot\n"
            f"ExecStart={command} ready {control} {environment}\n"
        )
        unit = TEMPLATE.read_text().replace("graphical-session-pre.target", pre)
        # A fixture must never acquire/remove the real compatibility unit alias.
        unit = unit.replace("Alias=workspace-dbus.service\n", "")
        unit = unit.replace("graphical-session.target", session)
        unit = unit.replace("@LOCAL_BIN@", str(Path.home() / ".local/bin"))
        unit = unit.replace("org.rsynapse.Workspace", "org.rsynapse.StartupTest.t" + uuid.uuid4().hex)
        # Keep the template's actual dependency graph; replace only the payload.
        unit += (
            "\n[Service]\nType=oneshot\nExecStart=\nRestart=no\n"
            "RemainAfterExit=yes\nEnvironment=RSYNAPSE_STARTUP_READY=\n"
            f"EnvironmentFile=-{environment}\n"
            f"ExecStart={command} projection {control} {environment}\n"
        )
        (directory / projection).write_text(unit)
        with socket.socket(socket.AF_UNIX) as listener:
            listener.bind(str(control))
            listener.listen(2)
            listener.settimeout(15)
            try:
                systemctl("link", "--runtime", *[str(directory / n) for n in names])
                # A client may activate the projection before a session even
                # exists. After= alone would not prevent exec in this case.
                early = systemctl("start", projection, check=False)
                assert early.returncode != 0, "premature activation was allowed"
                state = systemctl("show", projection, "--property=ExecMainStartTimestampMonotonic", "--value")
                assert state.stdout.strip() == "0", "premature activation executed the projection"
                state = systemctl("show", session, "--property=ActiveState", "--value")
                assert state.stdout.strip() == "inactive", "activation pulled in a graphical session"
                systemctl("reset-failed", projection, check=False)

                # Simulate a compositor taking arbitrary time to publish its
                # socket environment. Synchronize with IPC, never a delay.
                systemctl("start", "--no-block", session)
                publisher, _ = listener.accept()
                with publisher:
                    publisher.settimeout(15)
                    assert publisher.recv(16) == b"ready", "projection ran before readiness"
                    state = systemctl("show", projection, "--property=ActiveState", "--value")
                    assert state.stdout.strip() == "inactive", state.stdout
                    publisher.sendall(b"publish")
                consumer, _ = listener.accept()
                with consumer:
                    consumer.settimeout(15)
                    assert consumer.recv(16) == b"projection"
                systemctl("start", projection)
                state = systemctl("show", projection, "--property=ActiveState", "--value")
                assert state.stdout.strip() == "active", state.stdout
                print("PASS: early activation rejected; session startup inherits published environment")
            finally:
                systemctl("stop", *names, check=False)
                systemctl("disable", "--runtime", *names, check=False)
                systemctl("reset-failed", *names, check=False)


if __name__ == "__main__":
    if len(sys.argv) > 1 and sys.argv[1] == "--worker":
        worker(*sys.argv[2:])
    else:
        exercise()
