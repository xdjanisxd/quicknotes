import contextlib
import io
import json
import os
from pathlib import Path
import sys
import tempfile
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import install_linux


REPOSITORY = Path(__file__).resolve().parents[2]


class LinuxInstallerTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.root = Path(self.temporary.name)
        self.home = self.root / "home"
        self.data = self.root / "data"
        self.home.mkdir()
        self.binary = self.root / "fake-quicknote"
        marker = self.root / "binary-was-run"
        self.binary.write_text(
            "#!/bin/sh\n"
            f"if [ \"$1\" = \"--quicknote-check-build\" ]; then touch {marker}; "
            "echo production; exit 0; fi\n"
            f"touch {marker}\n"
        )
        self.binary.chmod(0o755)
        self.marker = marker

    def tearDown(self):
        self.temporary.cleanup()

    def call_install(self, action, **kwargs):
        return install_linux.install(
            action,
            home=self.home,
            data=self.data,
            repository_root=REPOSITORY,
            **kwargs,
        )

    def test_dry_run_does_not_execute_binary_or_create_destinations(self):
        output = io.StringIO()
        with contextlib.redirect_stdout(output):
            self.call_install("install", binary=self.binary, dry_run=True)
        self.assertIn("Would validate", output.getvalue())
        self.assertFalse(self.marker.exists())
        self.assertFalse((self.home / ".local").exists())
        self.assertFalse(self.data.exists())

    def test_install_migrates_exact_bootstrap_and_uninstall_restores_it(self):
        desktop = self.data / "applications" / f"{install_linux.APP_ID}.desktop"
        checkout_binary = REPOSITORY / "src-tauri/target/release/quicknote"
        bootstrap = (
            "[Desktop Entry]\n"
            "Type=Application\n"
            "Name=Quicknote\n"
            "Comment=Fast local notes\n"
            f"Exec={install_linux.exec_value(checkout_binary)}\n"
            "Terminal=false\n"
            "Categories=Utility;\n"
            f"StartupWMClass={install_linux.APP_ID}\n"
        ).encode()
        desktop.parent.mkdir(parents=True)
        desktop.write_bytes(bootstrap)
        note = self.home / ".config/app.quicknote.local/notes-state.json"
        note.parent.mkdir(parents=True)
        note.write_text("user settings must survive")

        self.call_install("install", binary=self.binary)

        installed_binary = self.home / ".local/lib/quicknote/quicknote"
        launcher = self.home / ".local/bin/quicknote"
        self.assertEqual(installed_binary.read_bytes(), self.binary.read_bytes())
        self.assertTrue(os.access(launcher, os.X_OK))
        self.assertIn(f"Icon={install_linux.APP_ID}", desktop.read_text())
        self.assertIn(f"TryExec={installed_binary}\n", desktop.read_text())
        self.assertNotIn(f'TryExec="{installed_binary}"', desktop.read_text())
        self.assertNotIn(str(REPOSITORY), desktop.read_text())
        self.assertTrue((self.data / f"icons/hicolor/16x16/apps/{install_linux.APP_ID}.png").is_file())
        self.assertTrue(self.marker.exists(), "install validates the production build probe")

        self.call_install("uninstall")

        self.assertEqual(desktop.read_bytes(), bootstrap)
        self.assertEqual(note.read_text(), "user settings must survive")
        self.assertFalse(installed_binary.exists())
        self.assertFalse(launcher.exists())

    def test_install_upgrades_previous_owned_quoted_tryexec_launcher(self):
        self.call_install("install", binary=self.binary)
        desktop = self.data / "applications" / f"{install_linux.APP_ID}.desktop"
        old_entry = install_linux.previous_installed_desktop_entry(
            self.home / ".local/lib/quicknote/quicknote"
        )
        desktop.write_bytes(old_entry)
        record_path = self.data / install_linux.RECORD_RELATIVE
        record = json.loads(record_path.read_text())
        record["files"][str(desktop)] = install_linux.sha256(old_entry)
        record_path.write_bytes(install_linux.record_bytes(record))

        self.call_install("install", binary=self.binary, skip_build=False)

        contents = desktop.read_text()
        self.assertIn(
            f"TryExec={self.home / '.local/lib/quicknote/quicknote'}\n", contents
        )
        self.assertNotIn(
            f'TryExec="{self.home / ".local/lib/quicknote/quicknote"}"', contents
        )

    def test_force_saves_and_restores_unrelated_same_id_launcher(self):
        desktop = self.data / "applications" / f"{install_linux.APP_ID}.desktop"
        desktop.parent.mkdir(parents=True)
        original = b"[Desktop Entry]\nName=User custom launcher\nExec=other-app\n"
        desktop.write_bytes(original)

        with self.assertRaisesRegex(ValueError, "unrelated desktop entry"):
            self.call_install("install", binary=self.binary)

        self.call_install("install", binary=self.binary, force=True)
        self.assertIn(f"Icon={install_linux.APP_ID}", desktop.read_text())
        self.call_install("uninstall")
        self.assertEqual(desktop.read_bytes(), original)

    def test_uninstall_refuses_modified_installed_file(self):
        self.call_install("install", binary=self.binary)
        wrapper = self.home / ".local/bin/quicknote"
        wrapper.write_text("user-modified")

        with self.assertRaisesRegex(ValueError, "modified installed file"):
            self.call_install("uninstall")
        self.assertEqual(wrapper.read_text(), "user-modified")


if __name__ == "__main__":
    unittest.main()
