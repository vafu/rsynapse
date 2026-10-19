#!/usr/bin/env python3
"""Install namespaced daemon artifacts/metadata and migrate owned legacy names.

No running service is stopped or restarted. --no-reload supports offline staging
and isolated installation tests. Short executable/unit aliases remain usable.
"""
import argparse
import os
from pathlib import Path
import shutil
import subprocess
import tempfile

REPO = Path(__file__).resolve().parents[1]
DAEMONS = {
    "workspace-dbus": ("rsynapse-workspace-dbus", "org.rsynapse.Workspace"),
    "steward": ("rsynapse-steward", "org.rsynapse.Steward"),
    "locusd": ("rsynapse-locusd", "org.rsynapse.Locus"),
    "projd": ("rsynapse-projd", "org.rsynapse.Proj"),
}


def atomic(path, content=None, source=None, link=None):
    path.parent.mkdir(parents=True, exist_ok=True)
    fd, temporary = tempfile.mkstemp(prefix="." + path.name + "-", dir=path.parent)
    temporary = Path(temporary)
    try:
        with os.fdopen(fd, "wb") as output:
            if source:
                with source.open("rb") as input:
                    shutil.copyfileobj(input, output)
            elif content is not None:
                output.write(content.encode())
            os.fchmod(output.fileno(), 0o755 if source else 0o644)
            output.flush()
            os.fsync(output.fileno())
        if link is not None:
            temporary.unlink()
            temporary.symlink_to(link)
        os.replace(temporary, path)
    finally:
        temporary.unlink(missing_ok=True)


def install(prefix, config, built=False, reload=True):
    binary = prefix / "bin"
    units = config / "systemd/user"
    services = prefix / "share/dbus-1/services"
    render = lambda path: path.read_text().replace("@LOCAL_BIN@", str(binary)).replace("@SCRIPT_DIR@", str(config / "scripts"))
    templates = REPO / "install/systemd/user"
    # Validate legacy ownership before replacing anything. User drop-ins remain
    # in place and systemd also reads them through the compatibility unit aliases.
    for legacy, (canonical, bus) in DAEMONS.items():
        name = canonical + ".service"
        text = render(templates / (name + ".in"))
        old_text = text.replace("/" + canonical, "/" + legacy).replace(f"Alias={legacy}.service\n", "")
        old = units / (legacy + ".service")
        if legacy != "steward" and old.is_symlink() and old.resolve() != units / name:
            raise RuntimeError(f"legacy unit points outside the owned canonical unit: {old}")
        if legacy != "steward" and old.exists() and not old.is_symlink():
            if old.read_text() != old_text:
                raise RuntimeError(f"legacy unit has local changes; preserve/migrate these first: {old}")
        current = units / name
        if current.exists() and not current.is_symlink():
            if current.read_text() not in (text, old_text):
                raise RuntimeError(f"canonical unit has local changes: {current}")

    if built:
        # Locus/workspace artifacts are installed by cargo install in local.sh.
        # Projd and Steward share the isolated public-registry build directory.
        for name in ("rsynapse-projd", "proj", "rsynapse-steward"):
            source = REPO / "steward/target/release" / name
            if not source.is_file():
                raise FileNotFoundError(source)
            atomic(binary / name, source=source)
    for legacy, (canonical, _) in DAEMONS.items():
        if not (binary / canonical).is_file():
            raise FileNotFoundError(binary / canonical)
        if (binary / canonical).is_symlink() and Path(os.path.abspath((binary / canonical).parent / os.readlink(binary / canonical))) == binary / legacy:
            raise RuntimeError(f"canonical binary still points to its legacy name; install the new build first: {canonical}")
        atomic(binary / legacy, link=canonical)

    for template in templates.glob("*.in"):
        atomic(units / template.name.removesuffix(".in"), content=render(template))
    for template in (REPO / "install/dbus-1/services").glob("*.in"):
        atomic(services / template.name.removesuffix(".in"), content=render(template))
    for legacy, (canonical, _) in DAEMONS.items():
        if legacy == "steward":
            continue  # Steward's unit name was already namespaced.
        old = units / (legacy + ".service")
        atomic(old, link=canonical + ".service")
        for wants in units.glob("*.target.wants"):
            previous = wants / old.name
            if previous.is_symlink() and previous.resolve() == units / (canonical + ".service"):
                atomic(wants / (canonical + ".service"), link="../" + canonical + ".service")
                previous.unlink()

    if reload:
        result = subprocess.run(["systemctl", "--user", "daemon-reload"], capture_output=True) if shutil.which("systemctl") else None
        if result is not None and result.returncode == 0:
            names = [name + ".service" for name, _ in DAEMONS.values()]
            names += ["rsynapse-shell.service", "rsynapse-notifications.service"]
            subprocess.run(["systemctl", "--user", "enable", *names], check=True)
            subprocess.run(["systemctl", "--user", "reset-failed", *names], capture_output=True)
        else:
            print("systemd user manager unavailable; metadata installed without reload/enable")
        if shutil.which("busctl"):
            subprocess.run(["busctl", "--user", "call", "org.freedesktop.DBus", "/org/freedesktop/DBus",
                "org.freedesktop.DBus", "ReloadConfig"], capture_output=True)
    print("Installed rsynapse-* daemon names, compatibility aliases and org.rsynapse.* activation metadata")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--prefix", type=Path, default=Path(os.environ.get("PREFIX", str(Path.home() / ".local"))))
    parser.add_argument("--config-home", type=Path, default=Path.home() / ".config")
    parser.add_argument("--install-built", action="store_true", help="install already-built Projd/Steward artifacts too")
    parser.add_argument("--no-reload", action="store_true", help="stage files only, without contacting systemd or D-Bus")
    args = parser.parse_args()
    install(args.prefix.expanduser().absolute(), args.config_home.expanduser().absolute(), args.install_built, not args.no_reload)
