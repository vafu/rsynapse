"""Offline installation/migration checks; never contacts systemd or D-Bus."""
import configparser
import importlib.util
import os
from pathlib import Path
import subprocess
import tempfile
import unittest

SCRIPT = Path(__file__).resolve().parents[1] / "session-services.py"
spec = importlib.util.spec_from_file_location("session_services", SCRIPT)
installer = importlib.util.module_from_spec(spec)
spec.loader.exec_module(installer)


class NamingMigration(unittest.TestCase):
    def fixture(self, root):
        prefix, config = root / "prefix", root / "config"
        binary, units = prefix / "bin", config / "systemd/user"
        binary.mkdir(parents=True)
        units.mkdir(parents=True)
        for legacy, (canonical, _) in installer.DAEMONS.items():
            command = binary / canonical
            command.write_text(f"#!/bin/sh\nprintf '%s\\n' {canonical}\n")
            command.chmod(0o755)
            (binary / legacy).write_text("old binary bytes\n")
            template = installer.REPO / "install/systemd/user" / (canonical + ".service.in")
            old = template.read_text().replace("@LOCAL_BIN@", str(binary))
            old = old.replace("/" + canonical, "/" + legacy).replace(f"Alias={legacy}.service\n", "")
            name = canonical if legacy == "steward" else legacy
            (units / (name + ".service")).write_text(old)
            target = "graphical-session.target" if legacy == "workspace-dbus" else "default.target"
            wants = units / (target + ".wants")
            wants.mkdir(exist_ok=True)
            (wants / (name + ".service")).symlink_to("../" + name + ".service")
        dropins = units / "locusd.service.d"
        dropins.mkdir()
        (dropins / "override.conf").write_text("[Service]\nEnvironment=LOCUS_RELATIONS_PATH=/custom/store.json\n")
        return prefix, config

    def test_names_aliases_activation_and_enabled_links_migrate_idempotently(self):
        with tempfile.TemporaryDirectory(prefix="rsynapse-names-") as temporary:
            prefix, config = self.fixture(Path(temporary))
            units, binary = config / "systemd/user", prefix / "bin"
            with (binary / "steward").open("rb") as old_inode:
                installer.install(prefix, config, reload=False)
                # Replacing a compatibility path preserves the old inode held
                # by running processes, instead of overwriting executable bytes.
                self.assertEqual(old_inode.read(), b"old binary bytes\n")
            installer.install(prefix, config, reload=False)
            for legacy, (canonical, bus) in installer.DAEMONS.items():
                self.assertEqual(os.readlink(binary / legacy), canonical)
                self.assertEqual(subprocess.check_output([str(binary / legacy)], text=True).strip(), canonical)
                activation = configparser.ConfigParser(interpolation=None)
                activation.read(prefix / "share/dbus-1/services" / (bus + ".service"))
                section = activation["D-BUS Service"]
                self.assertEqual(section["Name"], bus)
                self.assertEqual(section["Exec"].split()[0], str(binary / canonical))
                self.assertEqual(section["SystemdService"], canonical + ".service")
                unit = configparser.ConfigParser(interpolation=None)
                unit.read(units / (canonical + ".service"))
                self.assertEqual(unit["Service"]["BusName"], bus)
                self.assertEqual(unit["Service"]["ExecStart"].split()[0], str(binary / canonical))
                if legacy != "steward":
                    self.assertEqual(os.readlink(units / (legacy + ".service")), canonical + ".service")
                    target = unit["Install"]["WantedBy"]
                    self.assertTrue((units / (target + ".wants") / (canonical + ".service")).is_symlink())
                    self.assertFalse((units / (target + ".wants") / (legacy + ".service")).exists())
            self.assertIn("/custom/store.json", (units / "locusd.service.d/override.conf").read_text())

    def test_unknown_legacy_unit_is_preserved_before_any_metadata_changes(self):
        with tempfile.TemporaryDirectory(prefix="rsynapse-names-") as temporary:
            prefix, config = self.fixture(Path(temporary))
            legacy = config / "systemd/user/locusd.service"
            legacy.write_text("[Service]\nExecStart=/custom/locusd\n")
            with self.assertRaisesRegex(RuntimeError, "local changes"):
                installer.install(prefix, config, reload=False)
            self.assertEqual(legacy.read_text(), "[Service]\nExecStart=/custom/locusd\n")
            self.assertFalse((config / "systemd/user/rsynapse-workspace-dbus.service").exists())
            self.assertFalse((prefix / "share/dbus-1/services").exists())

    def test_reversing_old_steward_symlink_does_not_overwrite_legacy_inode(self):
        with tempfile.TemporaryDirectory(prefix="rsynapse-names-") as temporary:
            prefix, config = self.fixture(Path(temporary))
            binary = prefix / "bin"
            canonical = binary / "rsynapse-steward"
            artifact = Path(temporary) / "built-steward"
            artifact.write_bytes(canonical.read_bytes())
            canonical.unlink()
            canonical.symlink_to("steward")
            with (binary / "steward").open("rb") as old:
                installer.atomic(canonical, source=artifact)
                installer.install(prefix, config, reload=False)
                self.assertEqual(old.read(), b"old binary bytes\n")
            self.assertFalse(canonical.is_symlink())
            self.assertEqual((binary / "steward").resolve(), canonical)


if __name__ == "__main__":
    unittest.main()
