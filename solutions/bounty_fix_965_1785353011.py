### Technical Overview

The minimum Rust version documented across `README.md` and `CONTRIBUTING.md` (`1.75+`) was out of sync with the pinned Rust toolchain image in the root `Dockerfile` (`rust:1.88-slim`). 

To resolve this inconsistency and maintain documentation accuracy:
1. **`README.md`**: Updated the top badge row Rust version badge from `1.75+` (`1.75%2B`) to `1.88+` (`1.88%2B`).
2. **`CONTRIBUTING.md`**: Updated the Rust prerequisite table entry from `stable, 1.75+` to `stable, 1.88+`.

---

### Code Solution

Below is a Python automation script that programmatically reconciles `README.md` and `CONTRIBUTING.md` with the pinned `rust:1.88-slim` Dockerfile version.

```python
#!/usr/bin/env python3
import re
from pathlib import Path


def reconcile_rust_version(repo_root: Path, target_version: str = "1.88"):
    """
    Reconcile README.md and CONTRIBUTING.md Rust versions with Dockerfile.
    """
    readme_path = repo_root / "README.md"
    contributing_path = repo_root / "CONTRIBUTING.md"

    # 1. Update README.md badge
    if readme_path.exists():
        content = readme_path.read_text(encoding="utf-8")
        # Replace badge text patterns (e.g. 1.75+ or encoded 1.75%2B)
        updated_content = re.sub(
            r"(rust-|Rust%20Version-|Rust-)(1\.\d+)(%2B|\+)",
            rf"\g<1>{target_version}\g<3>",
            content,
        )
        if content != updated_content:
            readme_path.write_text(updated_content, encoding="utf-8")
            print(f"Updated {readme_path.name} Rust badge to {target_version}+")

    # 2. Update CONTRIBUTING.md prerequisites table
    if contributing_path.exists():
        content = contributing_path.read_text(encoding="utf-8")
        updated_content = re.sub(
            r"(Rust\s*\|\s*stable,\s*)1\.\d+\+",
            rf"\g<1>{target_version}+",
            content,
        )
        if content != updated_content:
            contributing_path.write_text(updated_content, encoding="utf-8")
            print(f"Updated {contributing_path.name} Rust version to {target_version}+")


if __name__ == "__main__":
    reconcile_rust_version(Path("."))
```

---

### Git Patch

```diff
diff --git a/README.md b/README.md
index 1111111..2222222 100644
--- a/README.md
+++ b/README.md
@@ -1 +1 @@
-[![Minimum Rust Version](https://img.shields.io/badge/Minimum%20Rust%20Version-1.75%2B-orange.svg)](https://www.rust-lang.org)
+[![Minimum Rust Version](https://img.shields.io/badge/Minimum%20Rust%20Version-1.88%2B-orange.svg)](https://www.rust-lang.org)

diff --git a/CONTRIBUTING.md b/CONTRIBUTING.md
index 3333333..4444444 100644
--- a/CONTRIBUTING.md
+++ b/CONTRIBUTING.md
@@ -11 +11 @@
-| Rust | stable, 1.75+ | Compiler and package manager |
+| Rust | stable, 1.88+ | Compiler and package manager |
```