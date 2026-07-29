### Technical Overview

This solution adds a GitHub Pull Request template (`.github/PULL_REQUEST_TEMPLATE.md`) to standardise PR submissions according to the project's `CONTRIBUTING.md` guidelines.

#### Key Enhancements:
1. **Description & Summary Section**: Prompts the author to explain what changed, why the change was made, and any follow-up items.
2. **Testing Verification**: Provides dedicated fields for authors to describe how they verified their changes and what tests were run locally.
3. **PR Checklist**: Enforces the 7-step checklist from `CONTRIBUTING.md`:
   - Proper branch naming convention (e.g., `feat/`, `fix/`, `docs/`).
   - Feature/fix implementation with corresponding unit/integration tests.
   - Passing test suite executed locally.
   - Conventional Commits format used for PR title.
   - PR description populated with summary and testing details.
   - Rebased on top of the default branch with conflicts resolved.
   - Ready for reviewer assignment.

---

### GitHub PR Template File (`.github/PULL_REQUEST_TEMPLATE.md`)

```markdown
## Description

<!-- Provide a clear description of what changed, why, and any relevant context or follow-ups. -->

## Type of Change

- [ ] `feat`: A new feature
- [ ] `fix`: A bug fix
- [ ] `docs`: Documentation updates
- [ ] `chore`: Maintenance, CI/CD, or refactoring

## How Has This Been Tested?

<!-- Describe the tests that you ran to verify your changes. Include commands used and test environment details. -->

- [ ] Unit tests added/updated
- [ ] Tested locally (`pytest` or test runner)

## PR Checklist

- [ ] **Branch Naming**: My branch follows the naming convention (`feat/`, `fix/`, `docs/`, `chore/`).
- [ ] **Implementation & Tests**: Implemented changes along with corresponding unit/integration tests.
- [ ] **Local Testing**: Ran tests locally and verified all tests pass.
- [ ] **PR Title**: PR title follows [Conventional Commits](https://www.conventionalcommits.org/) (e.g., `docs(ci): add GitHub pull request template`).
- [ ] **Description**: Description includes what changed, why, how it was tested, and any follow-ups.
- [ ] **Rebase**: Rebased on top of `main` / latest target branch without merge conflicts.
- [ ] **Review**: Ready for maintainer review.
```

---

### Python Script to Add PR Template (`create_pr_template.py`)

Below is a Python script that automatically creates the `.github` directory (if it does not exist) and generates the `.github/PULL_REQUEST_TEMPLATE.md` file.

```python
import os
from pathlib import Path

PR_TEMPLATE_CONTENT = """## Description

<!-- Provide a clear description of what changed, why, and any relevant context or follow-ups. -->

## Type of Change

- [ ] `feat`: A new feature
- [ ] `fix`: A bug fix
- [ ] `docs`: Documentation updates
- [ ] `chore`: Maintenance, CI/CD, or refactoring

## How Has This Been Tested?

<!-- Describe the tests that you ran to verify your changes. Include commands used and test environment details. -->

- [ ] Unit tests added/updated
- [ ] Tested locally (`pytest` or test runner)

## PR Checklist

- [ ] **Branch Naming**: My branch follows the naming convention (`feat/`, `fix/`, `docs/`, `chore/`).
- [ ] **Implementation & Tests**: Implemented changes along with corresponding unit/integration tests.
- [ ] **Local Testing**: Ran tests locally and verified all tests pass.
- [ ] **PR Title**: PR title follows [Conventional Commits](https://www.conventionalcommits.org/) (e.g., `docs(ci): add GitHub pull request template`).
- [ ] **Description**: Description includes what changed, why, how it was tested, and any follow-ups.
- [ ] **Rebase**: Rebased on top of `main` / latest target branch without merge conflicts.
- [ ] **Review**: Ready for maintainer review.
"""


def create_pull_request_template(repo_root: str = ".") -> Path:
    """Creates .github/PULL_REQUEST_TEMPLATE.md within the specified repository root."""
    target_dir = Path(repo_root) / ".github"
    target_dir.mkdir(parents=True, exist_ok=True)

    template_path = target_dir / "PULL_REQUEST_TEMPLATE.md"
    template_path.write_text(PR_TEMPLATE_CONTENT, encoding="utf-8")

    print(f"Successfully created PR template at: {template_path.resolve()}")
    return template_path


if __name__ == "__main__":
    create_pull_request_template()
```