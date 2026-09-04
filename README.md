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

In Claude Code, add this repo as a plugin marketplace and install the plugin:

```
/plugin marketplace add dmdhrumilmistry/security-harness
/plugin install security-harness
```

`/plugin marketplace add` accepts any of: a GitHub `owner/repo` (as above), a full git URL
(`https://github.com/dmdhrumilmistry/security-harness.git`), or a local path to a clone
(e.g. `/plugin marketplace add ./security-harness` from the directory containing your checkout).
Then run `/plugin install security-harness` and reload when prompted.

**Graft is installed and set up automatically by the pipeline.** Stage 0 runs `npm install -g
@nanonets/graft` if it's missing (requires Node/npm), then `graft init <target> --no-agents --no-global`
to register the Graft MCP server and freshness hooks for the target repo. The graph itself (`<target>/graft/`,
auto-gitignored) is built during recon. To pre-install manually: `npm install -g @nanonets/graft`. Graft's
structural build is free and needs no API key; the optional `--deep` LLM pass uses `GRAFT_API_KEY` /
`GRAFT_PROVIDER` / `GRAFT_MODEL` when set.

**The other tools are also auto-installed by Stage 0 when missing** (via whatever package manager is on the
machine — winget/choco/scoop, brew, apt, npm/pip/go — see `references/tooling-setup.md`). Installs are
announced, prefer no-elevation methods, and never block the run: anything that can't be installed is simply
marked unavailable and the pipeline falls back. Stage 0 installs only what fills a missing capability group:

- SBOM: [`syft`](https://github.com/anchore/syft) · CVEs: one of [`grype`](https://github.com/anchore/grype)
  (preferred), [`trivy`](https://github.com/aquasecurity/trivy), or [`osv-scanner`](https://github.com/google/osv-scanner)
- Reports: `wkhtmltopdf` or `pandoc` (for PDF/DOCX); otherwise you get `report.html` (or a headless-Chrome PDF).

All of them are optional — the pipeline degrades gracefully to native search + manifest parsing if none install.

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

### Model & cost control

Each stage runs on a model matched to its cognitive load, so tokens are spent where discovery quality
actually depends on them and saved on mechanical work. **This is the default — no arguments needed.**

| Stage | Default model |
|---|---|
| recon | `sonnet` |
| hunt (per class) | `haiku` for pattern classes (secrets, crypto, open-redirect, csrf) · `sonnet` for source→sink tracing (sqli, xss, ssrf, injection, path-traversal, xxe, file-upload, auth) · `opus` for deep-logic classes (access-control, race-conditions, deserialization) |
| chain | `opus` |
| verify | `opus` (the precision gate — kept strong) |
| report | `haiku` |

Override with the `models:` argument (passed through the router too):

```
/sh-security-review .                         # default tiered map above
/sh-security-review . models:max              # every stage + hunter on opus (max quality, max cost)
/sh-security-review . models:cheap            # aggressive downshift (trades some verify precision)
/sh-security-review . models:verify=opus,hunt=sonnet          # per-stage overrides
/sh-security-review . models:report=sonnet,hunt.pattern=sonnet  # per-hunter-tier override
```

Stages: `setup, recon, hunt, chain, verify, report`. Models: `opus, sonnet, haiku, inherit`. For `hunt`,
a bare model flattens all hunters to it; `hunt.pattern` / `hunt.trace` / `hunt.logic` target one tier.
Other token savers are built in: recon only spawns hunters for classes with real attack surface, hunters
query the Graft graph instead of reading whole files, and `findings.json`/SARIF are generated by a
deterministic script rather than the model.

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
