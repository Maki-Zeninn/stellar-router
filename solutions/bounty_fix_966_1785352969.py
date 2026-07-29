### Technical Overview

The issue in `/workspaces/stellar-router/integration-tests/tests/README.md` is that the **Quick Tests** section omits two existing test cases and contains a truncated description line (`- test_account_generation_and_funding - Tes`).

To fix this:
1. The script inspects the test directory (`/workspaces/stellar-router/integration-tests/tests/`) for source files (`.rs`, `.py`, `.js`, `.ts`).
2. It parses all `test_*` function definitions and extracts doc comments or generates clean descriptions for each test case.
3. It fixes the incomplete description for `test_account_generation_and_funding`.
4. It updates the `### Quick Tests` section in `README.md` to include all discovered quick tests formatted consistently as `- \`test_name\` - Description`.

---

### Python Solution

```python
import os
import re
from pathlib import Path


def fix_readme_quick_tests():
    # 1. Locate integration-tests/tests/README.md
    readme_path = None
    candidates = [
        Path("/workspaces/stellar-router/integration-tests/tests/README.md"),
        Path("integration-tests/tests/README.md"),
        Path("tests/README.md"),
        Path("README.md"),
    ]

    for cand in candidates:
        if cand.exists() and "integration-tests" in str(cand.absolute()):
            readme_path = cand
            break

    if not readme_path:
        for root, dirs, files in os.walk("."):
            if "README.md" in files and "integration-tests" in root:
                readme_path = Path(root) / "README.md"
                break

    if not readme_path or not readme_path.exists():
        print("Error: Could not find README.md in integration-tests/tests/")
        return

    tests_dir = readme_path.parent

    # Known fallback descriptions if doc comments are absent
    known_descriptions = {
        "test_stellar_cli_available": "Verify CLI installation",
        "test_wasm_contracts_built": "Verify WASM files exist",
        "test_account_generation_and_funding": "Test account generation and funding",
    }

    discovered_tests = {}

    # 2. Scan source files in tests_dir to discover all test functions
    for file_path in tests_dir.rglob("*"):
        if file_path.suffix in [".rs", ".py", ".ts", ".js"]:
            try:
                lines = file_path.read_text(encoding="utf-8").splitlines()
                for i, line in enumerate(lines):
                    # Match Rust/Python/JS test functions
                    match = re.search(
                        r"(?:fn|def)\s+(test_[a_zA-Z0-9_]+)\s*\(", line
                    )
                    if not match:
                        match = re.search(
                            r"(?:test|it)\s*\(\s*['\"](test_[a_zA-Z0-9_]+)['\"]",
                            line,
                        )

                    if match:
                        test_name = match.group(1)
                        if test_name not in discovered_tests:
                            # Extract doc comments preceding the test definition
                            doc_comment = ""
                            j = i - 1
                            while j >= 0 and j >= i - 10:
                                prev_line = lines[j].strip()
                                if prev_line.startswith(
                                    "///"
                                ) or prev_line.startswith("//"):
                                    cleaned = re.sub(
                                        r"^(///|//)\s*", "", prev_line
                                    ).strip()
                                    if cleaned and not cleaned.startswith("#"):
                                        doc_comment = cleaned
                                        break
                                elif (
                                    prev_line.startswith("#[")
                                    or prev_line == ""
                                ):
                                    j -= 1
                                    continue
                                else:
                                    break

                            if doc_comment:
                                description = doc_comment
                            elif test_name in known_descriptions:
                                description = known_descriptions[test_name]
                            else:
                                words = test_name.replace("test_", "").split(
                                    "_"
                                )
                                description = " ".join(words).capitalize()

                            discovered_tests[test_name] = description
            except Exception as e:
                print(f"Warning: Failed to read {file_path}: {e}")

    # Fallback to known default list if source parsing yields no results
    if not discovered_tests:
        discovered_tests = {
            "test_stellar_cli_available": "Verify CLI installation",
            "test_wasm_contracts_built": "Verify WASM files exist",
            "test_account_generation_and_funding": "Test account generation and funding",
        }

    # 3. Format updated Quick Tests section
    quick_tests_lines = ["### Quick Tests", ""]
    for name, desc in discovered_tests.items():
        quick_tests_lines.append(f"- `{name}` - {desc}")

    quick_tests_block = "\n".join(quick_tests_lines)

    # 4. Read and update README.md content
    readme_content = readme_path.read_text(encoding="utf-8")
    pattern = r"### Quick Tests\n.*?(?=\n### |\n## |\Z)"

    if re.search(pattern, readme_content, flags=re.DOTALL):
        updated_content = re.sub(
            pattern, quick_tests_block, readme_content, flags=re.DOTALL
        )
    else:
        # Fallback string replacement
        lines = readme_content.splitlines()
        new_lines = []
        in_quick_tests = False
        for line in lines:
            if line.strip() == "### Quick Tests":
                in_quick_tests = True
                new_lines.extend(quick_tests_lines)
            elif in_quick_tests:
                if line.startswith("#") or (
                    line.strip() and not line.strip().startswith("-")
                ):
                    in_quick_tests = False
                    new_lines.append(line)
            else:
                new_lines.append(line)
        updated_content = "\n".join(new_lines) + "\n"

    readme_path.write_text(updated_content, encoding="utf-8")
    print(f"Successfully updated {readme_path}")


if __name__ == "__main__":
    fix_readme_quick_tests()
```