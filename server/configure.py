"""Prepares the Anki data directory before Anki starts.

Runs on every start of the container, so a changed setting or a rebuilt
image takes effect at the next start:

1. creates the profile ANKI_PROFILE without Anki's first-run dialogs;
2. installs the add-ons shipped in the image into /data/addons21,
   replacing their code and keeping their user files;
3. writes both add-ons' settings: AnkiConnect on 0.0.0.0:8765 and the MCP
   server on 0.0.0.0:3141 at /mcp/, with no API key, and the MCP server's
   allowed host names from ANKI_SERVER_HOSTS and ANKI_TLS_NAME.
"""

import json
import os
import shutil
from pathlib import Path

from aqt.profiles import ProfileManager

DATA = Path("/data")
SHIPPED = Path("/opt/addons")
ADDONS = DATA / "addons21"

ANKICONNECT = "2055492159"
ANKIMCP = "anki_mcp_server"
ANKICONNECT_PORT = 8765
MCP_PORT = 3141
MCP_PATH = "mcp"


def profile_name() -> str:
    name = os.environ.get("ANKI_PROFILE", "").strip()
    return name or "User 1"


def create_profile(name: str) -> None:
    pm = ProfileManager(DATA)
    pm.setupMeta()
    # Without these Anki asks for a language and a profile on the first run.
    pm.meta["defaultLang"] = pm.meta.get("defaultLang") or "en_US"
    pm.meta["firstRun"] = False
    # Not pm.profiles(): with no profile yet it creates a default one, which
    # needs Anki's translations, not loaded outside the app.
    existing = pm.db.list("select name from profiles where name != '_global'")
    if name not in existing:
        pm.create(name)
    pm.load(name)
    pm.set_last_loaded_profile_name(name)
    pm.save()
    print(f"profile: {name}")


def install_addon(package: str) -> Path:
    """Replaces the add-on's code with the shipped one and keeps user_files."""
    source = SHIPPED / package
    target = ADDONS / package
    target.mkdir(parents=True, exist_ok=True)
    for entry in target.iterdir():
        if entry.name in ("user_files", "meta.json"):
            continue
        if entry.is_dir():
            shutil.rmtree(entry)
        else:
            entry.unlink()
    for entry in source.iterdir():
        if entry.name == "user_files" and (target / "user_files").exists():
            continue
        if entry.is_dir():
            shutil.copytree(entry, target / entry.name)
        else:
            shutil.copy2(entry, target / entry.name)
    return target


def write_config(addon_dir: Path, config: dict) -> None:
    """Anki merges meta.json's "config" over the add-on's config.json."""
    meta_path = addon_dir / "meta.json"
    meta = {}
    if meta_path.exists():
        try:
            meta = json.loads(meta_path.read_text())
        except ValueError:
            meta = {}
    meta["config"] = config
    meta["disabled"] = False
    # The image pins the version: no update offers from AnkiWeb.
    meta["update_enabled"] = False
    meta_path.write_text(json.dumps(meta, indent=2) + "\n")


def allowed_hosts() -> list[str]:
    names = []
    for variable in ("ANKI_SERVER_HOSTS", "ANKI_TLS_NAME"):
        for part in os.environ.get(variable, "").split(","):
            part = part.strip()
            if part and part not in names:
                names.append(part)
    hosts = []
    for name in names:
        # A client sends the port in Host unless it is the scheme's default
        # (a TLS name behind the proxy on 443 arrives without one).
        hosts += [name, f"{name}:{MCP_PORT}"]
    return hosts


def main() -> None:
    create_profile(profile_name())

    ankiconnect = install_addon(ANKICONNECT)
    write_config(
        ankiconnect,
        {
            "apiKey": None,
            "apiLogPath": None,
            "webBindAddress": "0.0.0.0",
            "webBindPort": ANKICONNECT_PORT,
            "webCorsOriginList": ["http://localhost"],
            "ignoreOriginList": [],
        },
    )

    ankimcp = install_addon(ANKIMCP)
    hosts = allowed_hosts()
    write_config(
        ankimcp,
        {
            "http_enabled": True,
            "http_host": "0.0.0.0",
            "http_port": MCP_PORT,
            "http_path": MCP_PATH,
            "http_api_key": "",
            "http_allowed_hosts": hosts,
        },
    )
    print(f"AnkiConnect: 0.0.0.0:{ANKICONNECT_PORT}, no API key")
    print(f"MCP: 0.0.0.0:{MCP_PORT}/{MCP_PATH}/, no API key, hosts: {', '.join(hosts) or 'localhost only'}")


if __name__ == "__main__":
    main()
