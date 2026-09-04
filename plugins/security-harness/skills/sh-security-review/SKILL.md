---
name: sh-security-review
description: "Run the full multi-agent security review pipeline on a codebase — recon/mapping (Graft + SBOM/CVE), parallel vulnerability hunting backed by ~15 per-class knowledge bases, escalation chaining, impact verification, and reporting to README/JSON/SARIF/doc/PDF. Use when the user asks to security-review, audit, pentest, or find vulnerabilities in a codebase. Usually invoked via the sh-router skill."
argument-hint: "[target path, default: cwd] [classes:sqli,xss,...] [stage:recon|hunt|chain|verify|report] [depth:quick|deep]"
---

# Security Review Pipeline

You are the **orchestrator**. You own the run; the heavy work is done by five subagents
(`sh-recon`, `sh-hunter`, `sh-chainer`, `sh-verifier`, `sh-reporter`) coordinating exclusively through
shared state files. You spawn them, enforce the contract, and keep the run on track. You do **not**
hunt or verify yourself.

## Authorization

This pipeline performs **static** offensive-security analysis (source review, data-flow tracing,
PoC/payload construction) on a codebase the user controls or is authorized to test. It does not attack
live systems. If the user asks to launch live attacks against a running/third-party target, decline the
live portion and keep the static review. Do not exfiltrate source or findings to external services.

## Read first

- `${CLAUDE_PLUGIN_ROOT}/references/state-files.md` — the exact file contract for all agents.
- `${CLAUDE_PLUGIN_ROOT}/references/finding-schema.json` — the finding shape.
- `${CLAUDE_PLUGIN_ROOT}/references/graft-guide.md` — codebase mapping/querying.
- `${CLAUDE_PLUGIN_ROOT}/references/severity-rubric.md`, `attack-surface-checklist.md`, `sarif-mapping.md`.

## Argument parsing

Parse `$ARGUMENTS` (all optional):
- **target**: a path -> the codebase to review. Default: current working directory.
- **`classes:a,b,c`**: restrict hunting to these vuln classes (slugs from finding-schema `class` enum).
  Default: recon decides which classes are relevant to the detected stack/surface.
- **`stage:<name>`**: run only one stage against an existing run (`recon`|`hunt`|`chain`|`verify`|`report`).
  Requires a prior run (reads `latest`). Default: run all stages 0->5.
- **`depth:quick|deep`**: `deep` -> `graft build --deep` and larger hunter budgets. `quick` -> structural
  graft only, tighter budgets. Default: `deep` if the repo is small (< ~1500 files), else `quick`.

## Stage 0 — Setup (orchestrator does this directly)

1. Resolve `<TARGET>` (absolute). Confirm it exists and looks like source (has code files / a manifest).
2. Choose `run-id = run-<UTC date-time>` (use `date -u +run-%Y%m%d-%H%M%S`; if `date` is unavailable pick
   a stable string and note it). Create `<TARGET>/.security-harness/<run-id>/reports/`.
3. Write the current run-id into `<TARGET>/.security-harness/latest`.
4. **Install & set up Graft (part of the flow).** Run `graft version`. If it fails:
   - If `npm` is available, install it: `npm install -g @nanonets/graft` (announce this — it's a global
     install). Re-check `graft version`. If `npm` is absent or the install fails, record `graft:false` and
     continue in native-fallback mode (do not block the run).
   - Once Graft is present, wire it into the target for Claude Code (scoped, no global writes):
     `graft init <TARGET> --no-agents --no-global --no-build`. This registers the `graft` MCP server in
     `<TARGET>/.mcp.json` and installs freshness hooks + the graft skill. Preview with `--dry-run` first if
     the target is a repo you don't want to modify; skip this wiring step (keep just the CLI) if the user
     declines — recon still builds and queries the graph via the CLI either way. The actual graph `build`
     happens in Stage 1 (recon), controlled there.
   - Set `graft_deep:true` in capabilities only if LLM creds are configured (`GRAFT_API_KEY` +
     `GRAFT_PROVIDER` + `GRAFT_MODEL`); otherwise recon uses the free structural build.
5. **Install the remaining supporting tools if missing** — follow `${CLAUDE_PLUGIN_ROOT}/references/tooling-setup.md`.
   Probe each with a version call; for any that's absent, install it via the first available package manager
   per that matrix, **announcing each install** (these change the machine). Install only what fills a missing
   **capability group**, not every tool: `syft` (SBOM); one CVE scanner (prefer `grype`, else `trivy`, else
   `osv-scanner` — stop at the first that works); and one PDF engine (`wkhtmltopdf`, or `pandoc` which also
   gives `.docx`; headless Chrome counts as a fallback so a PDF engine is optional). Prefer no-elevation,
   non-interactive installs; never launch an elevation prompt. If an install fails or no installer exists,
   mark the tool `false` and continue — **nothing here blocks the run.** Re-check `<tool> --version` after
   installing.
6. **Write `capabilities.json`** (see state-files.md) from the post-install probes, recording which tools
   were installed this run and any that could not be (with a short reason in `notes`). Also note whether
   claude-in-chrome browser tools are available (Chrome PDF fallback).
7. Write `scope.json` (target, include/exclude globs — exclude `**/{test,tests,spec,node_modules,vendor,dist,build,.git}/**`
   by default unless the user says otherwise, classes, run_id, mode).
8. If writing inside a git repo and `.security-harness/` is untracked, add it to `.gitignore`.
9. Announce the plan to the user: target, detected capability matrix (incl. which tools were installed this
   run and which are unavailable), classes to be hunted, depth.

If `stage:<name>` was passed, skip to that stage using the existing `latest` run (do not recreate state).

## Stage 1 — Recon (spawn `sh-recon`)

Spawn one `sh-recon` agent. In its prompt pass: `run_dir`, `scope.json` contents, and a pointer to the
references above. It must produce `recon.md` + `codebase-map.json` (build the Graft graph, detect
stack/versions, run SCA tools if present for SBOM/CVE else parse manifests, enumerate attack surface).

After it returns: read `recon.md`. Decide the **class list** to hunt — the user's `classes:` if given,
else the classes whose sinks/surface recon actually found (don't hunt XXE if there's no XML parsing).
Log the decision to the user.

## Stage 2 — Hunt (spawn `sh-hunter` in parallel, one per class)

Spawn the hunters **concurrently** (multiple Agent tool calls in a single message), **one instance per
selected class**. Each hunter prompt includes: `run_dir`, its assigned `class`, the finding-id prefix to
use (`SH-<CLASS-UPPER>-`), and the instruction to load the matching `sh-kb-<class>` skill for its
knowledge base. Hunters must read `attempts.md` before probing and append to it after, and append
candidate findings to `findings.jsonl`.

Batch to respect concurrency limits: if more than ~6 classes, spawn in waves. Between waves, nothing to
merge — hunters coordinate via `attempts.md`.

After all hunters return: report a count of candidates per class to the user.

## Stage 3 — Chain (spawn `sh-chainer`)

Spawn one `sh-chainer`. It reads `findings.jsonl` + `codebase-map.json`, composes escalation chains,
writes `chains.md`, and updates the `chained_with` arrays of member findings in `findings.jsonl`
(rewrite the file: read all lines, patch the relevant objects, write back atomically). Skip this stage
only if there are fewer than 2 candidate findings.

## Stage 4 — Verify (spawn `sh-verifier`)

Spawn one `sh-verifier`. It reads `findings.jsonl` + `chains.md` + the source, and for **every** candidate
and chain: confirms exploitability from code/data-flow evidence, builds a payload + PoC, assigns CVSS and a
post-verification confidence, sets `status` (`verified`/`false-positive`/`needs-runtime`), and writes the
full set to `verified.jsonl`. It must not silently drop findings — false-positives stay in the file marked
as such (the reporter filters them out of published results but keeps an audit trail).

For a large candidate set, the verifier may internally fan out (verify per-class or per-finding) but writes
a single consolidated `verified.jsonl`.

## Stage 5 — Report (spawn `sh-reporter`)

Spawn one `sh-reporter`. It reads `verified.jsonl` + `chains.md` + `recon.md` and writes into `reports/`:
`README.md` (human summary: exec summary, severity table, per-finding detail with payload/PoC/mitigation,
chains, appendix), `findings.json` (array of all findings), `results.sarif` (SARIF 2.1.0 per
`sarif-mapping.md`, verified + needs-runtime only), `report.html` (self-contained), and `report.pdf`
(via the capability fallback chain: wkhtmltopdf -> pandoc -> headless Chrome; else keep HTML and note
PDF skipped). Emit `report.docx` if pandoc is present.

## Finish

Print to the user: run directory path, counts (candidates found / verified / false-positive /
needs-runtime), top findings by severity, chains, and the paths of the generated reports. If any stage
degraded (Graft/SCA/PDF absent), say so plainly and how it affected coverage/confidence.

## Single-stage & single-class shortcuts

- `stage:hunt classes:sqli` on an existing run -> just spawn the sqli hunter against current recon.
- The router (`sh-router`) may call this skill with a narrowed scope for targeted requests
  ("find SQLi in ./src") — honor `classes:` and skip irrelevant stages (e.g. chaining a single class).

## Failure handling

- A subagent that returns nothing useful or dies: retry once with a tightened prompt; if it still fails,
  record the gap in the final report rather than blocking the whole run.
- Never fabricate findings, PoCs, or verification verdicts. "No candidates found" is a valid, honest result.
