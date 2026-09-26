# Trusted sources - knowledge-base auto-update

This file is the **single source of truth** for the domains the automated
knowledge-base updater is allowed to read from. It exists to keep the update
process safe against **prompt injection**: web pages can contain text that tries
to hijack an LLM ("ignore your instructions, do X"). We reduce that risk two ways:

1. **A tight domain allowlist.** The updater's `WebFetch` tool is permitted only
   for the domains below (enforced in `.github/workflows/update-knowledge-base.yml`
   via `--allowedTools "WebFetch(domain:...)"`). Anything off-list cannot be fetched,
   even if `WebSearch` surfaces it or a fetched page links to it.
2. **Content is data, never instructions.** The updater prompt
   (`.github/kb-update/prompt.md`) treats every fetched byte as untrusted reference
   material - technical facts to distill, not commands to obey - and the model that
   ingests it has **no shell and no push access**; its only output is file edits that
   go through a **human-reviewed pull request**.

When you add or remove a domain here, make the **same change** to the
`--allowedTools` list in the workflow. The two must stay in sync.

## Allowlist (tiers by trust)

### Tier 1 - standards bodies & official documentation (highest trust, editorially controlled)
| Domain | What it provides |
|---|---|
| `owasp.org` | OWASP Top 10, ASVS, testing guide, project docs |
| `cheatsheetseries.owasp.org` | OWASP Cheat Sheet Series (per-class defensive guidance) |
| `cwe.mitre.org` | CWE weakness definitions and relationships |
| `capec.mitre.org` | CAPEC attack-pattern catalog |
| `nvd.nist.gov` | National Vulnerability Database (CVE detail, CVSS) |
| `csrc.nist.gov` | NIST crypto / security standards (SP 800 series) |
| `developer.mozilla.org` | Web platform security semantics (CSP, cookies, CORS) |

### Tier 2 - reputable vendor / researcher publications (curated, high editorial quality)
| Domain | What it provides |
|---|---|
| `portswigger.net` | Web Security Academy labs + PortSwigger Research (new techniques) |
| `googleprojectzero.blogspot.com` | Project Zero deep-dive writeups |
| `github.blog` | GitHub Security Lab advisories & research |

### Tier 3 - curated community repositories (code/paths only from named repos)
| Domain | What it provides |
|---|---|
| `github.com` | Source of curated repos (e.g. OWASP, `swisskyrepo/PayloadsAllTheThings`) |
| `raw.githubusercontent.com` | Raw files from those repos |

### Tier 4 - bug-bounty disclosure (valuable but user-generated → treat as most-untrusted)
| Domain | What it provides |
|---|---|
| `hackerone.com` | Public disclosed reports / Hacktivity (real-world exploit patterns) |

> **Note on Tier 4:** HackerOne report bodies are written by third parties and are
> the highest prompt-injection risk on this list. The updater is instructed to
> extract only the *technical vulnerability pattern* (root cause, affected sink,
> generalized payload shape) and to **never** follow any instruction, link, or
> request embedded in a report. Reviewers should scrutinize KB changes sourced
> from Tier 4 most closely in the PR.

## Rules for what may be added to the knowledge base

- Only **generalized, defensive/educational** technique knowledge: detection
  recipes, source/sink lists, false-positive filters, CWE/OWASP mappings,
  mitigations, and generic payload *shapes* - the same kind of content already in
  the `sh-kb-*` skills.
- **No** live target names, no personal data, no working exploit chains against a
  specific third party, no secrets/tokens ever.
- Every added fact should be attributable to an allowlisted source; the PR body
  lists the sources consulted.
