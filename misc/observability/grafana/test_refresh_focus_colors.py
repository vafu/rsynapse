"""Offline rendering checks with synthetic Graphite inventory."""
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parent
SCRIPT = ROOT / "refresh-focus-colors.py"
spec = importlib.util.spec_from_file_location("focus_colors", SCRIPT)
colors = importlib.util.module_from_spec(spec)
spec.loader.exec_module(colors)


class DashboardRendering(unittest.TestCase):
    def test_generic_source_contains_no_discovered_inventory(self):
        before = {path: path.read_bytes() for path in (ROOT / "dashboards").glob("*.json")}
        result = subprocess.run([sys.executable, str(SCRIPT), "--generic", "--check",
                                 "--output-dir", str(ROOT / "dashboards")], capture_output=True, text=True)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(before, {path: path.read_bytes() for path in before})
        focus = json.loads((ROOT / "dashboards/rsynapse-focus.json").read_text())
        panels = {panel["id"]: panel for panel in focus["panels"]}
        for panel_id in (1, 7):
            self.assertEqual(set(panels[panel_id]["fieldConfig"]["defaults"]["mappings"][0]["options"]), {"(none)"})
        for panel_id in (2, 8, 16, 18, 19, 20):
            self.assertFalse(panels[panel_id]["fieldConfig"]["overrides"])

    def test_runtime_inventory_and_stable_colors_are_kept_outside_source(self):
        before = {path: path.read_bytes() for path in (ROOT / "dashboards").glob("*.json")}
        with tempfile.TemporaryDirectory(prefix="rsynapse-dashboard-render-") as temporary:
            root = Path(temporary)
            output = root / "rendered"
            args = [str(SCRIPT), "--output-dir", str(output)]
            env = dict(os.environ, XDG_STATE_HOME=str(root / "state"))
            with patch.dict(os.environ, env, clear=True), patch.object(sys, "argv", args), \
                    patch.object(colors, "find_names", return_value={"sample-project"}):
                colors.main()
            registry = root / "state/rsynapse/grafana/focus-colors.json"
            previous = json.loads(registry.read_text())
            self.assertIn("sample-project", previous["contexts"])
            self.assertEqual(len(list(output.glob("*.json"))), len(before))
            self.assertIn("sample-project", (output / "rsynapse-focus.json").read_text())
            with patch.dict(os.environ, env, clear=True), patch.object(sys, "argv", args), \
                    patch.object(colors, "find_names", return_value={"sample-project", "another-project"}):
                colors.main()
            self.assertEqual(json.loads(registry.read_text())["contexts"]["sample-project"],
                             previous["contexts"]["sample-project"])
        self.assertEqual(before, {path: path.read_bytes() for path in before})

    def test_runtime_rendering_cannot_overwrite_source(self):
        result = subprocess.run([sys.executable, str(SCRIPT), "--output-dir", str(ROOT / "dashboards")],
                                capture_output=True, text=True)
        self.assertEqual(result.returncode, 2)
        self.assertIn("outside the tracked dashboard directory", result.stderr)


if __name__ == "__main__":
    unittest.main()
