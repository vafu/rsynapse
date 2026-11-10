"""Build wrappers preserve the caller's Cargo environment; no Rust builds."""
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

REPO = Path(__file__).resolve().parents[2]


class BuildEnvironment(unittest.TestCase):
    def test_wrappers_forward_arguments_and_preserve_environment_and_cwd(self):
        with tempfile.TemporaryDirectory(prefix="rsynapse-build-paths-") as temporary:
            root = Path(temporary)
            cargo = root / "cargo"
            cargo.write_text(
                f"#!{sys.executable}\n"
                "import json, os, sys\n"
                "print(json.dumps({'args': sys.argv[1:], 'cwd': os.getcwd(), "
                "'home': os.environ.get('CARGO_HOME'), "
                "'target': os.environ.get('CARGO_TARGET_DIR')}))\n"
            )
            cargo.chmod(0o755)
            for project in ("misc/steward", "dbus/projd", "misc/dashboard-api"):
                for overrides in ({}, {"CARGO_HOME": str(root / "custom cache"),
                                       "CARGO_TARGET_DIR": "relative target"}):
                    with self.subTest(project=project, overrides=overrides):
                        env = dict(os.environ, PATH=str(root) + os.pathsep + os.environ["PATH"])
                        env.pop("CARGO_HOME", None)
                        env.pop("CARGO_TARGET_DIR", None)
                        env.update(overrides)
                        result = json.loads(subprocess.check_output(
                            ["bash", str(REPO / project / "build.sh"), "--locked", "--offline"],
                            cwd=root, env=env, text=True,
                        ))
                        self.assertEqual(result, {
                            "args": ["build", "--release", "--manifest-path",
                                     str(REPO / project / "Cargo.toml"), "--locked", "--offline"],
                            "cwd": str(root), "home": overrides.get("CARGO_HOME"),
                            "target": overrides.get("CARGO_TARGET_DIR"),
                        })


if __name__ == "__main__":
    unittest.main()
