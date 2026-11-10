"""Replaces the profile's collection with a .colpkg, as Anki's File > Import
does for a collection package. Anki must not be running (server/restore.sh
stops it first).

  python /opt/slipcard/restore.py <file.colpkg>

The current collection is copied to the profile's backups folder first,
as before-restore-<time>.anki2.
"""

import shutil
import sys
import time
from pathlib import Path

from anki._backend import RustBackend

from configure import DATA, create_profile, profile_name


def main() -> None:
    package = Path(sys.argv[1])
    if not package.is_file():
        sys.exit(f"not a file: {package}")
    name = profile_name()
    create_profile(name)
    folder = DATA / name
    collection = folder / "collection.anki2"
    if collection.exists():
        backups = folder / "backups"
        backups.mkdir(exist_ok=True)
        saved = backups / f"before-restore-{time.strftime('%Y-%m-%d-%H.%M.%S')}.anki2"
        shutil.copy2(collection, saved)
        print(f"current collection saved as {saved.relative_to(DATA)}")
    RustBackend().import_collection_package(
        col_path=str(collection),
        backup_path=str(package),
        media_folder=str(folder / "collection.media"),
        media_db=str(folder / "collection.media.db2"),
    )
    print(f"restored {package.name} into profile {name}")


if __name__ == "__main__":
    main()
