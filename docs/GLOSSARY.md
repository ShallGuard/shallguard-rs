# ShallGuard glossary

This page defines the terms that the ShallGuard documents use. Each term has
one meaning. The documents use the same word for the same thing.

## General terms

| Term | Meaning |
|---|---|
| Rust | The programming language that ShallGuard analyzes. |
| Cargo | The build tool and package manager for Rust. |
| Crate | One Rust package. A crate is a library or an executable. |
| Cargo workspace | A group of crates that Cargo builds together from one repository. |
| Repository | A Git directory that holds the source code and the documents of one project. |
| Merge request (MR) | A proposed change to a repository. A reviewer examines the change before it goes into the main branch. GitHub calls this a pull request. |
| Continuous integration (CI) | An automated pipeline that builds and tests each proposed change. |
| Gate | A CI step that must pass before a change can merge. |
| Coding agent | A program that uses a large language model to write or change code. Examples are Claude Code, Codex, and Copilot. |
| Large language model (LLM) | A program that produces text from a prompt. ShallGuard uses an LLM only for advisory review. |
| Provider | The command-line program that gives ShallGuard access to an LLM. The supported providers are Codex, Claude, and Copilot. |

## ShallGuard terms

| Term | Meaning |
|---|---|
| Requirement | One numbered statement of what the software must do. It has the form `REQ-<AREA>-<NNN>` and uses the word SHALL. It lives in a Markdown document that `shallguard.toml` selects. |
| Requirement document | A Markdown file that holds requirements. |
| Area | A group of requirements with one capability. The area name is the middle part of the requirement ID, for example `CLI` in `REQ-CLI-001`. |
| RFC 2119 | An internet standard that defines the words SHALL, SHALL NOT, and MAY for requirement statements. |
| Anchor | A mark in Rust code that names a requirement ID. ShallGuard finds anchors in the syntax of the code. A requirement ID in a comment is not an anchor. |
| Enforcement anchor | The attribute `#[shallguard::enforces]` or the macro `shallguard::enforces_here!`. It marks the code that makes a requirement true. |
| Enforcement site | The code item or block that carries an enforcement anchor. |
| Verification anchor | The attribute `#[shallguard::verifies]` on a Rust test. It marks a test that gives evidence for a requirement. |
| Verification test | A test that carries a verification anchor. |
| Evidence | The proof that a requirement is true. Each requirement names its evidence class on its *Verified:* line. |
| Evidence class | One of four marks: ✅ an anchored automated test, 🔬 an end-to-end or production validation, 👁 a code review only, ⏳ pending. |
| Citation | The file and the symbol that a requirement names on its *Enforced:* or *Verified:* line. |
| Traceability | The link between a requirement, its enforcement anchors, and its verification anchors. |
| Check | The command `cargo shallguard check`. It examines the traceability of every requirement and fails when a link is broken. |
| Gap | A requirement without an enforcement anchor, or a requirement without automated evidence. |
| Baseline | A committed file that lists the gaps that existed when a repository adopted ShallGuard. The baseline can only become smaller. |
| Ratchet | The rule that the number of gaps can only go down. The check rejects each new gap. |
| Hard area | An area with a policy that does not accept any gap in the baseline. |
| Vacuous test | A test that cannot fail. ShallGuard rejects a vacuous test as evidence. |
| Oracle | The part of a test that decides if the test passes or fails. A test can declare an oracle outside its body with the `oracle` option. |
| Impact | The relation between a Git change and a requirement. The impact can be direct, transitive, or structural. |
| Coverage | Execution evidence from the LLVM tools. It shows if a verification test reached an enforcement site. Coverage does not prove that the code is correct. |
| Capsule | A bounded, reproducible bundle that holds one requirement, its code, its changes, and its evidence for a review. |
| Semantic review | An advisory judgment about a capsule. A person or an LLM gives the judgment. It is not part of the check. |
| Verdict | The result of a semantic review for one requirement. A verdict is advisory. |
| Artifact | A machine-readable file that a ShallGuard command writes. Each artifact has a schema version. |
