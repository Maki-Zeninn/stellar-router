### Technical Overview

#### Problem
In `.github/workflows/metrics-exporter.yml`, the `branches` key under `push` and `pull_request` event triggers uses array syntax with inner spaces (`[ main, develop ]`). Across all other GitHub Actions workflow files in `.github/workflows/` (such as `ci.yml`, `wasm-size-check.yml`, etc.), array items are formatted compactly without inner leading or trailing spaces (`[main]` / `[main, develop]`). This inconsistency violates repo-wide YAML formatting conventions.

#### Solution
Normalize the `branches` list definitions in `.github/workflows/metrics-exporter.yml` by removing leading/trailing spaces inside the brackets, changing `[ main, develop ]` to `[main, develop]`.

---

### Code Fix (Git Diff)

```diff
--- a/.github/workflows/metrics-exporter.yml
+++ b/.github/workflows/metrics-exporter.yml
@@ -2,10 +2,10 @@ name: Metrics Exporter
 
 on:
   push:
-    branches: [ main, develop ]
+    branches: [main, develop]
   pull_request:
-    branches: [ main, develop ]
+    branches: [main, develop]
 
 jobs:
```

---

### Python Script (Automated Fixer)

If you need a programmatic script to apply this fix across workflow files automatically:

```python
from pathlib import Path


def normalize_workflow_branches(workflow_path: Path) -> bool:

    """Normalizes array bracket spacing in workflow branch definitions."""
    if not workflow_path.exists():
        print(f"File not found: {workflow_path}")
        return False

    content = workflow_path.read_text(encoding="utf-8")

    # Replace array formatting with inner spaces [ main, develop ] -> [main, develop]
    updated_content = content.replace("[ main, develop ]", "[main, develop]")

    if content != updated_content:
        workflow_path.write_text(updated_content, encoding="utf-8")
        print(f"Successfully normalized branch formatting in {workflow_path}")
        return True

    print(f"No changes required for {workflow_path}")
    return False


if __name__ == "__main__":
    target_file = Path(".github/workflows/metrics-exporter.yml")
    normalize_workflow_branches(target_file)
```