# Security Harness

Multi-agent application-security review for AI coding agents. It maps a codebase,
hunts vulnerabilities against 15 per-class knowledge bases, chains findings into
escalations, verifies real impact, and writes a report.

## Install

**Claude Code**

```
/plugin marketplace add dmdhrumilmistry/security-harness
/plugin install security-harness@security-harness
```

**Gemini CLI**

```bash
gemini extensions install https://github.com/dmdhrumilmistry/security-harness
```

**opencode, or any agentskills.io-compatible agent**

```bash
git clone https://github.com/dmdhrumilmistry/security-harness
cd security-harness
python scripts/sync-agent-skills.py --install agents
```

## Use

Ask in plain language. The `sh-router` skill interprets the request and dispatches.

```
security review this repo
audit src/api for IDOR
check for SQL injection in the payments service
pentest this codebase and give me SARIF
review PR 42
```

## Pull request review

`sh-pr-review` reviews one pull request instead of a whole codebase. Run it on your own
machine, on any PR you can read. It posts findings as inline comments on the PR and sets
a `security/pr-review` commit status that branch protection can enforce, and it always
asks before posting anything.

```
review https://github.com/acme/api/pull/128   # any repo, cloned to a temp dir
review PR 42                                  # the repo you are currently in
security review this PR                       # the PR for your current branch
```

A PR fails only for what it `introduced` or `aggravated`. Findings that predate it are
reported and never block. Triage runs first with no subagents, so a PR with no security
surface costs nothing and still reports a status. Comments carry a line-number-free
fingerprint, so a re-push adds only what is new.

It writes to the pull request and the commit status only. It opens no issues and creates
nothing in any external tracker.

## Pipeline

```
recon -> hunt (parallel, one per class) -> chain -> verify -> report
```

| Stage | Agent | What it produces |
|---|---|---|
| Recon | `sh-recon` | `recon.md`, `codebase-map.json`: stack, SBOM, known CVEs, attack surface |
| Hunt | `sh-hunter` (xN) | `findings.jsonl`, `attempts.md`: candidate findings, one agent per class |
| Chain | `sh-chainer` | `chains.md`: multi-step escalations built from individual findings |
| Verify | `sh-verifier` | `verified.jsonl`: exploitability verdict, payload, PoC, CVSS, confidence |
| Report | `sh-reporter` | `README.md`, `findings.json`, `results.sarif`, `report.html`, `report.pdf` |

## Knowledge bases

One skill per vulnerability class, each with sources, sinks, detection queries,
payloads, false-positive filters, and remediation:

`access-control` `auth` `crypto` `csrf` `deserialization` `file-upload`
`injection` `open-redirect` `path-traversal` `race-conditions` `secrets` `sqli`
`ssrf` `xss` `xxe`

Invoke one directly when you already know the class, for example
`sh-kb-sqli` or `sh-kb-ssrf`.

## Supporting tools

Graft for structural mapping, and syft/grype/trivy/osv-scanner for SBOM and CVE
data. All optional: the pipeline degrades to native search and manifest parsing
when none are installed. See `references/tooling-setup.md`.

## Scope

For code you are authorized to review. The harness reads and reasons about
source; it does not attack live systems. Findings ship with payloads and PoC
steps so a human can reproduce them in a test environment.

## License

MIT. See `LICENSE` at the repository root.
