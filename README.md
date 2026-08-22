# Security Harness

A multi-agent **application-security review harness** for Claude Code (and, later, other AI agents). One
router skill dispatches to a full offensive-security pipeline that **maps** a codebase, **hunts**
vulnerabilities with a per-class knowledge base, **chains** findings into escalations, **verifies** real
impact, and **reports** to README / JSON / SARIF / doc / PDF.

> Scope: this harness performs **static** analysis (source review, data-flow tracing, PoC/payload
> construction) on code you own or are authorized to test. It does not attack live third-party systems.

## What's inside

```
security-harness/                         # a plugin marketplace
└── plugins/security-harness/
    ├── skills/
    │   ├── sh-router            # single entry point — routes any appsec request
    │   ├── sh-security-review   # the pipeline orchestrator (Stages 0–5)
    │   └── sh-kb-*  (15)        # per-vuln-class knowledge bases
    ├── agents/
    │   ├── sh-recon             # map: Graft graph + stack/SBOM/CVE + attack surface
    │   ├── sh-hunter            # find: source→sink hunting, one per class (parallel)
    │   ├── sh-chainer           # escalate: combine findings into attack chains
    │   ├── sh-verifier          # confirm: offensive + seceng + dev verification + PoC
    │   └── sh-reporter          # deliver: README/JSON/SARIF/HTML/PDF/doc
    └── references/              # shared contracts (finding schema, SARIF map, state files, rubrics)
```

### Vulnerability classes covered (the `sh-kb-*` skills)

access-control (IDOR/BOLA/priv-esc) · sqli · xss · ssrf · injection (cmd/code/SSTI/LDAP) · auth (session/JWT)
· deserialization · path-traversal (LFI/RFI) · secrets · csrf · xxe · open-redirect · crypto · race-conditions
· file-upload. Dependency CVEs/SBOM are handled by the recon stage.

## Install

```
/plugin marketplace add D:\work\orca\security-harness
/plugin install security-harness
```

**Graft is installed and set up automatically by the pipeline.** Stage 0 runs `npm install -g
@nanonets/graft` if it's missing (requires Node/npm), then `graft init <target> --no-agents --no-global`
to register the Graft MCP server and freshness hooks for the target repo. The graph itself (`<target>/graft/`,
auto-gitignored) is built during recon. To pre-install manually: `npm install -g @nanonets/graft`. Graft's
structural build is free and needs no API key; the optional `--deep` LLM pass uses `GRAFT_API_KEY` /
`GRAFT_PROVIDER` / `GRAFT_MODEL` when set.

**Other companion tools** (all optional — the pipeline degrades gracefully if absent):

- SBOM: [`syft`](https://github.com/anchore/syft) · CVEs: [`grype`](https://github.com/anchore/grype),
  [`trivy`](https://github.com/aquasecurity/trivy), or [`osv-scanner`](https://github.com/google/osv-scanner)
- Reports: `wkhtmltopdf` or `pandoc` (for PDF/DOCX); otherwise you get `report.html`.

## Usage

Invoke the router with a natural-language request:

```
/sh-router full security review of ./api
/sh-router find SQLi and IDOR in src/
/sh-router just map this codebase        # recon only
```

Or call the pipeline directly:

```
/sh-security-review . classes:sqli,access-control,ssrf depth:deep
/sh-security-review . stage:report       # regenerate reports for the latest run
```

### Output

Everything lands under `<target>/.security-harness/<run-id>/`:

- `recon.md`, `codebase-map.json` — the map (stack, SBOM, CVEs, attack surface).
- `findings.jsonl` → `chains.md` → `verified.jsonl` — the working state (see `references/state-files.md`).
- `reports/` — `README.md`, `findings.json`, `results.sarif`, `report.html`, `report.pdf` (+ `report.docx`).

Each published finding carries a payload, a PoC, the verification verdict, CWE/OWASP ids, CVSS, and a
code-level mitigation.

## How it works

1. **Setup** — probe available tools, define scope, create the run directory.
2. **Recon** (`sh-recon`) — build the Graft graph; detect stack/versions; SBOM + CVEs; enumerate entry
   points, trust boundaries, and dangerous sinks.
3. **Hunt** (`sh-hunter` ×N, parallel) — one hunter per relevant class loads its `sh-kb-*` knowledge base,
   traces attacker input from source to sink, and records candidates. A shared **attempts ledger** stops
   agents from repeating each other's probes.
4. **Chain** (`sh-chainer`) — compose findings into higher-severity attack paths.
5. **Verify** (`sh-verifier`) — refute first, then confirm exploitability from evidence, build PoCs, assign
   CVSS, and cut false positives.
6. **Report** (`sh-reporter`) — produce the deliverables.

Subagents share nothing but files; the contract is in `plugins/security-harness/references/state-files.md`.

## Extending

Add a new vulnerability class by creating `skills/sh-kb-<class>/SKILL.md` following the shared template
(When to hunt · Sources & sinks · Detection recipe · Payloads/PoC · False-positive filters · CWE/OWASP ·
Chaining hints · Mitigation), then add its slug to the `class` enum in `references/finding-schema.json`
and the routing table in `skills/sh-router/SKILL.md`.

## Roadmap

- Codex / Cursor mirror wiring (`.codex-plugin` / `.cursor-plugin`).
- Optional live-fetch augmentation of knowledge bases (PortSwigger/OWASP/CWE) on top of curated references.
- Optional DAST bridge for runtime confirmation of `needs-runtime` findings.

## License

MIT
