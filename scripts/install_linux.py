#!/usr/bin/env python3
"""Install Quicknote for the current Linux user (Python standard library only)."""

import argparse
import base64
import hashlib
import json
import os
from pathlib import Path
import shlex
import shutil
import subprocess
import sys
import tempfile

APP_ID = "app.quicknote.local"
APP_NAME = "Quicknote"
PNG_ASSETS = {
    16: "16x16.png",
    24: "24x24.png",
    32: "32x32.png",
    64: "64x64.png",
    128: "128x128.png",
    256: "128x128@2x.png",
    512: "icon.png",
}
RECORD_RELATIVE = Path("quicknote/desktop-install.json")


def exec_value(path):
    value = str(path)
    if any(character in value for character in "=%\n\r"):
        raise ValueError("installation paths cannot contain '=', '%', or newlines")
    escaped = "".join("\\" + char if char in '\\"`$' else char for char in value)
    return '"' + escaped.replace("\\", "\\\\").replace("%", "%%") + '"'


def desktop_string_value(value):
    """Escape a desktop-entry string value (used by path-valued TryExec)."""
    return (
        str(value)
        .replace("\\", "\\\\")
        .replace("\n", "\\n")
        .replace("\r", "\\r")
        .replace("\t", "\\t")
        .replace(" ", "\\s")
    )


def destinations(home, data):
    binary = home / ".local/lib/quicknote/quicknote"
    wrapper = home / ".local/bin/quicknote"
    desktop = data / "applications" / f"{APP_ID}.desktop"
    icons = [
        data / "icons/hicolor" / f"{size}x{size}/apps" / f"{APP_ID}.png"
        for size in PNG_ASSETS
    ]
    return [binary, wrapper, desktop, *icons]


def sha256(contents):
    return hashlib.sha256(contents).hexdigest()


def file_digest(path):
    if path.is_symlink() or not path.is_file():
        return None
    with path.open("rb") as source:
        return hashlib.file_digest(source, "sha256").hexdigest()


def atomic_write(path, contents, mode):
    path.parent.mkdir(parents=True, exist_ok=True)
    descriptor, temporary = tempfile.mkstemp(prefix=".quicknote-", dir=path.parent)
    try:
        with os.fdopen(descriptor, "wb") as output:
            output.write(contents)
            os.fchmod(output.fileno(), mode)
        os.replace(temporary, path)
    finally:
        if os.path.exists(temporary):
            os.unlink(temporary)


def read_record(path, allowed_paths, desktop_path):
    if path.is_symlink():
        raise ValueError(f"refusing symlinked installation record: {path}")
    if not path.exists():
        return {"files": {}, "replaced_desktop": None}
    try:
        record = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        raise ValueError(f"invalid installation record {path}: {error}") from error
    if not isinstance(record, dict) or set(record) != {"files", "replaced_desktop"}:
        raise ValueError("invalid Quicknote installation record")
    files = record["files"]
    if not isinstance(files, dict) or set(files) - allowed_paths:
        raise ValueError("installation paths changed; uninstall with the original HOME/XDG_DATA_HOME")
    for value in files.values():
        if not valid_digest(value):
            raise ValueError("invalid file hash in Quicknote installation record")
    replaced = record["replaced_desktop"]
    if replaced is not None:
        if not isinstance(replaced, str) or str(desktop_path) not in files:
            raise ValueError("invalid saved desktop entry in Quicknote installation record")
        try:
            base64.b64decode(replaced, validate=True)
        except (ValueError, base64.binascii.Error) as error:
            raise ValueError("invalid saved desktop entry in Quicknote installation record") from error
    return record


def valid_digest(value):
    return (
        isinstance(value, str)
        and len(value) == 64
        and all(character in "0123456789abcdef" for character in value)
    )


def record_bytes(record):
    return (json.dumps(record, indent=2, sort_keys=True) + "\n").encode()


def desktop_entry(executable):
    quoted = exec_value(executable)
    return (
        "[Desktop Entry]\n"
        "Type=Application\n"
        f"Name={APP_NAME}\n"
        "Comment=Fast local notes\n"
        f"Exec={quoted}\n"
        f"TryExec={desktop_string_value(executable)}\n"
        f"Icon={APP_ID}\n"
        "Terminal=false\n"
        "Categories=Utility;\n"
        "StartupNotify=true\n"
        f"StartupWMClass={APP_ID}\n"
    ).encode()


def previous_installed_desktop_entry(executable):
    """Exact launcher format written before TryExec was fixed to a plain path."""
    return desktop_entry(executable).replace(
        f"TryExec={desktop_string_value(executable)}\n".encode(),
        f"TryExec={exec_value(executable)}\n".encode(),
    )


def is_previous_bootstrap(contents, repository_root):
    try:
        lines = contents.decode("utf-8").splitlines()
    except UnicodeDecodeError:
        return False
    values = {}
    for line in lines:
        if not line or line.startswith("#") or line == "[Desktop Entry]":
            continue
        key, separator, value = line.partition("=")
        if not separator or key in values:
            return False
        values[key] = value
    expected = {
        "Type": "Application",
        "Name": APP_NAME,
        "Comment": "Fast local notes",
        "Terminal": "false",
        "Categories": "Utility;",
        "StartupWMClass": APP_ID,
    }
    if any(values.get(key) != value for key, value in expected.items()):
        return False
    if set(values) != set(expected) | {"Exec"}:
        return False
    for mode in ("debug", "release"):
        allowed = repository_root / "src-tauri/target" / mode / "quicknote"
        if values["Exec"] == exec_value(allowed):
            return True
    return False


def refresh_desktop(data):
    commands = [
        ("update-desktop-database", str(data / "applications")),
        ("gtk-update-icon-cache", "-f", "-t", str(data / "icons/hicolor")),
    ]
    if "KDE" in os.environ.get("XDG_CURRENT_DESKTOP", "").split(":"):
        commands.append(("kbuildsycoca6", "--noincremental"))
    for command in commands:
        if not shutil.which(command[0]):
            continue
        try:
            result = subprocess.run(command, capture_output=True, text=True, timeout=30)
        except (OSError, subprocess.TimeoutExpired):
            result = None
        if result is None or result.returncode:
            print(f"Warning: {command[0]} failed; launcher refresh may require logout/login",
                  file=sys.stderr)


def production_binary(binary):
    if binary.is_symlink() or not binary.is_file() or not os.access(binary, os.X_OK):
        raise ValueError(f"missing executable production binary: {binary}")
    try:
        result = subprocess.run(
            [str(binary), "--quicknote-check-build"],
            capture_output=True,
            text=True,
            timeout=5,
            check=False,
        )
    except (OSError, subprocess.TimeoutExpired) as error:
        raise ValueError(f"could not verify Quicknote build mode at {binary}: {error}") from error
    if result.returncode or result.stdout.strip() != "production":
        detail = result.stderr.strip() or result.stdout.strip() or f"exit {result.returncode}"
        raise ValueError(f"{binary} is not a production Tauri build ({detail})")


def bootstrap_or_force_backup(path, repository_root, force, installed_binary):
    contents = path.read_bytes()
    if (is_previous_bootstrap(contents, repository_root)
            or contents == previous_installed_desktop_entry(installed_binary)):
        return contents
    if force:
        return contents
    raise ValueError(
        f"refusing to replace unrelated desktop entry: {path}; "
        "inspect it and retry with --force to save and restore it on uninstall"
    )


def install(action, *, binary=None, skip_build=False, dry_run=False, force=False,
            home=None, data=None, repository_root=None):
    if sys.platform != "linux":
        raise ValueError("this installer supports Linux only")
    home = Path(home) if home is not None else Path(os.environ.get("HOME", ""))
    if not home.is_absolute() or not home.is_dir():
        raise ValueError("HOME must be an existing absolute directory")
    if data is None:
        configured_data = os.environ.get("XDG_DATA_HOME", "")
        data = Path(configured_data) if configured_data else Path()
        if not data.is_absolute():
            data = home / ".local/share"
    else:
        data = Path(data)
    if not data.is_absolute():
        raise ValueError("XDG_DATA_HOME must be absolute")
    root = Path(repository_root) if repository_root else Path(__file__).resolve().parent.parent
    paths = destinations(home, data)
    path_names = {str(path) for path in paths}
    record_path = data / RECORD_RELATIVE
    desktop_path = paths[2]
    record = read_record(record_path, path_names, desktop_path)

    if action == "uninstall":
        for path in paths:
            if path.exists() or path.is_symlink():
                if path.is_symlink() or str(path) not in record["files"]:
                    raise ValueError(f"refusing to remove unowned or symlinked file: {path}")
                if file_digest(path) != record["files"][str(path)]:
                    raise ValueError(f"refusing to remove modified installed file: {path}")
        if dry_run:
            print("Would uninstall Quicknote files:")
            for path in paths:
                if str(path) in record["files"]:
                    print(f"  {path}")
            if record["replaced_desktop"]:
                print(f"Would restore the previous desktop entry: {desktop_path}")
            return
        for path in reversed(paths):
            if str(path) in record["files"] and path.exists():
                path.unlink()
        if record["replaced_desktop"]:
            atomic_write(desktop_path, base64.b64decode(record["replaced_desktop"]), 0o644)
        if record_path.exists():
            record_path.unlink()
        refresh_desktop(data)
        print("Uninstalled Quicknote. Notes, settings, and shortcut preferences were preserved.")
        return

    if binary and skip_build:
        raise ValueError("use either --binary or --skip-build, not both")
    source_binary = Path(binary).expanduser() if binary else root / "src-tauri/target/release/quicknote"
    if not source_binary.is_absolute():
        source_binary = Path.cwd() / source_binary
    if not dry_run and not binary and not skip_build:
        subprocess.run(["npm", "run", "build:desktop"], cwd=root, check=True)
    if dry_run:
        if not binary and not skip_build:
            print("Would run `npm run build:desktop` to create the production app.")
        else:
            print("Would validate the selected production binary before installing.")
        print("Would install Quicknote to:")
        for path in paths:
            print(f"  {path}")
        return
    production_binary(source_binary)

    binary_contents = source_binary.read_bytes()
    wrapper_contents = (
        "#!/bin/sh\nexec " + shlex.quote(str(paths[0])) + " \"$@\"\n"
    ).encode()
    desktop_contents = desktop_entry(paths[0])
    icon_directory = root / "src-tauri/icons"
    contents = [binary_contents, wrapper_contents, desktop_contents]
    for asset in PNG_ASSETS.values():
        icon_path = icon_directory / asset
        if not icon_path.is_file():
            raise ValueError(f"missing application icon: {icon_path}")
        contents.append(icon_path.read_bytes())

    replaced_desktop = record["replaced_desktop"]
    for index, path in enumerate(paths):
        if not (path.exists() or path.is_symlink()):
            continue
        if path.is_symlink():
            raise ValueError(f"refusing symlinked installation destination: {path}")
        if str(path) in record["files"]:
            if file_digest(path) != record["files"][str(path)]:
                if index == 2 and path.read_bytes() == previous_installed_desktop_entry(paths[0]):
                    continue
                raise ValueError(f"refusing to overwrite modified installed file: {path}")
            continue
        if index == 2 and replaced_desktop is None:
            replaced_desktop = base64.b64encode(
                bootstrap_or_force_backup(path, root, force, paths[0])
            ).decode("ascii")
            continue
        raise ValueError(f"refusing unrelated or modified file: {path}")

    record["replaced_desktop"] = replaced_desktop
    atomic_write(record_path, record_bytes(record), 0o600)
    for index, (path, content) in enumerate(zip(paths, contents)):
        atomic_write(path, content, 0o755 if index < 2 else 0o644)
        record["files"][str(path)] = sha256(content)
        atomic_write(record_path, record_bytes(record), 0o600)
    refresh_desktop(data)
    print(f"Installed Quicknote: {paths[0]}\nLauncher: {paths[2]}")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("action", choices=("install", "uninstall"))
    parser.add_argument("--binary", help="production Quicknote executable to install")
    parser.add_argument("--skip-build", action="store_true",
                        help="use the existing target/release/quicknote artifact")
    parser.add_argument("--dry-run", action="store_true", help="show paths without writing or building")
    parser.add_argument("--force", action="store_true",
                        help="replace a non-bootstrap same-ID desktop entry, saving it for uninstall")
    args = parser.parse_args()
    if args.action == "uninstall" and (args.binary or args.skip_build or args.force):
        parser.error("--binary, --skip-build, and --force apply only to install")
    try:
        install(args.action, binary=args.binary, skip_build=args.skip_build,
                dry_run=args.dry_run, force=args.force)
    except (OSError, ValueError, subprocess.CalledProcessError) as error:
        parser.exit(1, f"Error: {error}\n")


if __name__ == "__main__":
    main()
