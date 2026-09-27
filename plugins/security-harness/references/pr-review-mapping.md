# PR Review Mapping

How a pull request becomes a posted GitHub review. Authoritative for the
`sh-pr-review` skill: triage, tier depth, diff anchoring, fingerprints, posting
policy, verdict, and commit status.

The design constraint behind all of it: **a PR reviewer is judged on signal per
comment, not on coverage.** A reviewer that finds everything and says it all gets
muted. Everything below exists to keep the posted output small, new, and correct.

## Triage

Triage runs in the orchestrator with Bash and Grep. It launches no subagents. It
answers two questions: does this PR touch a security surface, and which classes.

### Class detection

A class fires if **either** the path globs **or** the added-line tokens match.
Added lines only: a `+` line in the patch, never context or removed lines.

Classes are the same slugs the rest of the harness uses, so a fired class maps
straight onto its `sh-kb-<class>` knowledge base and an `sh-hunter` instance.

| Class | Path globs | Added-line tokens (regex, case-insensitive) |
|---|---|---|
| `sqli` | `**/models/**`, `**/repositor*/**`, `**/dao/**`, `**/queries/**`, `**/db/**` | `\.raw\s*\(`, `execute\s*\(`, `executemany\s*\(`, `cursor\.`, `SELECT .*(\+\|\$\{\|%s\|format\|f")`, `query\s*\(\s*[`"'].*\$\{`, `knex\.raw`, `sequelize\.query` |
| `injection` | `**/templates/**`, `**/views/**`, `**/jobs/**`, `**/tasks/**` | `\b(exec\|eval\|system\|popen\|spawn\|execSync)\s*\(`, `child_process`, `Runtime\.getRuntime`, `shell\s*=\s*True`, `render_template_string`, `Template\s*\(`, `new Function\s*\(` |
| `xss` | `**/templates/**`, `**/views/**`, `**/components/**`, `**/pages/**` | `innerHTML`, `outerHTML`, `dangerouslySetInnerHTML`, `v-html`, `\|\s*safe`, `mark_safe`, `html\.raw`, `document\.write` |
| `access-control` | `**/auth*/**`, `**/middleware*/**`, `**/permission*/**`, `**/rbac/**`, `**/polic*/**`, `**/route*/**`, `**/controller*/**` | `@app\.(route\|get\|post\|put\|delete\|patch)`, `@router\.`, `Depends\s*\(`, `@login_required`, `before_action`, `current_user`, `is_admin`, `has_permission`, `authorize`, `skip_auth`, `exempt` |
| `auth` | `**/auth*/**`, `**/session*/**`, `**/login*/**`, `**/identity/**` | `jwt`, `verify\s*\(`, `algorithms\s*=`, `session\[`, `set_cookie`, `httponly`, `samesite`, `password`, `bcrypt`, `argon2`, `mfa`, `otp` |
| `ssrf` | `**/client*/**`, `**/http*/**`, `**/proxy*/**`, `**/webhook*/**`, `**/fetch*/**` | `requests\.(get\|post)`, `urllib`, `httpx`, `axios`, `fetch\s*\(`, `HttpClient`, `curl_exec`, `URL\s*\(`, `open-uri` |
| `path-traversal` | `**/upload*/**`, `**/file*/**`, `**/static/**`, `**/storage/**` | `open\s*\(`, `readFile`, `sendFile`, `path\.join`, `os\.path\.join`, `\.\./`, `send_from_directory`, `File\s*\(` |
| `deserialization` | `**/serial*/**`, `**/cache*/**`, `**/queue*/**`, `**/rpc/**` | `pickle\.loads`, `yaml\.load\s*\(`, `unserialize`, `ObjectInputStream`, `Marshal\.load`, `JSON\.parse\s*\(.*req`, `fromJson` |
| `xxe` | `**/xml/**`, `**/parser*/**`, `**/soap/**`, `**/feed*/**` | `etree`, `DocumentBuilder`, `SAXParser`, `XMLReader`, `libxml`, `simplexml_load`, `resolve_entities` |
| `csrf` | `**/route*/**`, `**/controller*/**`, `**/form*/**`, `**/middleware*/**` | `csrf`, `csrf_exempt`, `SameSite`, `verify_authenticity_token`, `@app\.post`, `@router\.(post\|put\|delete\|patch)` |
| `open-redirect` | `**/auth*/**`, `**/route*/**`, `**/controller*/**`, `**/oauth*/**` | `redirect\s*\(`, `sendRedirect`, `Location:`, `next_url`, `return_to`, `returnUrl`, `callback_url`, `redirect_uri` |
| `crypto` | `**/crypto/**`, `**/encryption*/**`, `**/security/**`, `**/token*/**` | `hashlib`, `md5`, `sha1`, `\bAES\b`, `\bDES\b`, `\bECB\b`, `\bRSA\b`, `random\.`, `Math\.random`, `encrypt\s*\(`, `decrypt\s*\(`, `IV\s*=`, `createCipher` |
| `secrets` | `**/config/**`, `**/settings*`, `.env*`, `**/secrets*/**` | `password\s*=`, `secret\s*=`, `api[_-]?key`, `token\s*=`, `private[_-]?key`, `BEGIN .*PRIVATE KEY`, `aws_access_key`, `connection[_-]?string` |
| `file-upload` | `**/upload*/**`, `**/media/**`, `**/attachment*/**` | `multipart`, `MultipartFile`, `\.save\s*\(`, `move_uploaded_file`, `filename`, `content_type`, `mimetype`, `allowed_extensions` |
| `race-conditions` | `**/payment*/**`, `**/billing/**`, `**/balance*/**`, `**/transaction*/**`, `**/inventory/**` | `SELECT .* FOR UPDATE`, `transaction\s*\(`, `atomic`, `lock\s*\(`, `incr`, `decrement`, `balance`, `check.*then.*update` |
| `dependency` | `Dockerfile*`, `docker-compose*`, `.github/workflows/**`, `requirements*.txt`, `package.json`, `package-lock.json`, `go.mod`, `go.sum`, `Gemfile*`, `pom.xml`, `Cargo.toml`, `*.tf`, `**/k8s/**`, `**/helm/**` | `ARG .*(TOKEN\|KEY\|SECRET)`, `curl .*\|\s*(ba)?sh`, `privileged:\s*true`, `runAsRoot`, `0\.0\.0\.0`, `--no-verify`, `verify\s*=\s*False`, `rejectUnauthorized:\s*false` |

`dependency` has no `sh-kb-*` skill of its own. It routes to the SBOM and CVE
logic that `sh-recon` already owns, scoped to the changed manifest files.

### Paths that never fire a class

`*.md`, `docs/**`, `*.txt`, `LICENSE*`, `CHANGELOG*`, `.github/ISSUE_TEMPLATE/**`,
`**/*.snap`, `**/__snapshots__/**`, and test-only paths: `**/test/**`,
`**/tests/**`, `**/spec/**`, `*_test.*`, `*.test.*`, `*.spec.*`.

Three deliberate exceptions:

- **Lockfile-only changes are not Tier 0.** A dependency bump is a real
  supply-chain surface. `dependency` fires, tier is 1.
- **Test files still count for `secrets`.** A credential committed to a test
  fixture is a committed credential.
- **Test files still count for `access-control`** when the added lines change a
  fixture that grants a role or bypasses a guard, because those leak into
  production defaults more often than they should.

### Tiers

| Tier | Condition | Depth | Subagents |
|---|---|---|---|
| **0** | No class fired | Stop after triage. Post nothing. Set the status. | 0 |
| **1** | Exactly one class, and fewer than 300 changed lines | One `sh-hunter` reading the diff directly. No recon pass, no verify, no chaining. | 1 |
| **2** | Two or more classes, or 300+ changed lines, or a route/endpoint added or changed | Scoped `sh-recon` -> routed `sh-hunter`s -> `sh-verifier` on postable findings. | 3 to 6 |
| **3** | An auth/permission/session/crypto primitive changed, or the PR adds a new external input surface (a new route **and** a new sink), or `--deep` | Full pipeline: scoped recon -> all applicable hunters -> verify -> `sh-chainer`. | 6 to 10 |

`--tier=<n>` pins the tier. Record in `triage.json` that it was pinned, so a
later reader does not mistake it for the computed value.

### `triage.json`

```json
{
  "pr": 412,
  "head_sha": "a1b2c3d",
  "files_changed": 7,
  "lines_added": 143,
  "lines_removed": 22,
  "classes": {
    "access-control": {"fired": true, "why": "app/api/orders.py adds @router.get('/orders/{id}') with no permission check in the added lines"},
    "sqli": {"fired": true, "why": "app/db/orders.py adds an f-string interpolation inside execute()"},
    "crypto": {"fired": false, "why": null}
  },
  "tier": 2,
  "tier_source": "computed",
  "hunters": ["access-control", "sqli"]
}
```

One sentence of real reasoning per fired class. `"matched regex"` is not
reasoning: it does not let a reader judge whether triage was right.

## Diff anchoring

GitHub accepts an inline comment only on a line inside a diff hunk. Build the map
once in triage, then never guess.

### `diff-index.json`

```json
{
  "app/api/orders.py": {
    "added_lines": [41, 42, 43, 58],
    "hunks": [{"start": 38, "end": 46}, {"start": 55, "end": 61}]
  }
}
```

`added_lines` are RIGHT-side line numbers of `+` lines. `hunks` are the
RIGHT-side ranges the patch covers.

### Anchoring rules

- A finding **anchors** if `file` is a key and `line` is in that file's
  `added_lines`.
- A finding on a line inside `hunks` but not in `added_lines` (an unchanged
  context line) **does not anchor inline**. The PR did not write it, so it goes
  in the body.
- Anything else is unanchorable and goes in the body.
- Always `"side": "RIGHT"`. Never comment on the LEFT (deleted) side; the code is
  gone.
- **Never adjust a line number to make it fit.** A 422 from the API means
  anchoring is wrong. Fix the map, do not nudge the number.

## Fingerprints and deduplication

A PR gets re-reviewed on every push. Dedup is what keeps that from being spam.

State lives **in the PR itself**, not on disk, as a hidden marker in each comment
body. That survives force-pushes, re-runs, a different machine, and CI runners
with no shared volume.

```
<!-- sh-pr-review:fp=<12 hex chars> -->
```

### Computing the fingerprint

```
sha256( "<cwe>|<normalized_path>|<normalized_snippet>" )[:12]
```

- `normalized_path`: repo-relative, forward slashes.
- `normalized_snippet`: the vulnerable line, trimmed, internal whitespace runs
  collapsed to one space, and **string literal contents replaced with `""`**, so
  a changed message or renamed variable value does not mint a new fingerprint.
- **Line numbers are deliberately excluded.** Adding an unrelated function above
  a finding must not make it look new.

### Dedup procedure

1. Read every existing inline comment and review body on the PR.
2. Extract all `sh-pr-review:fp=` values.
3. Skip any finding whose fingerprint is already present. Log it to
   `receipts.jsonl` with `"action": "skipped_duplicate"`.
4. Any fingerprint on the PR but not in this run is **resolved**. List it under
   "Resolved since the last review". This is the cheapest trust-building signal
   the reviewer has: it shows the bot noticed the fix.

## How a PR relates to a finding

`pr_impact` is the single most consequential field in a PR review: it decides
whether the PR fails.

| `pr_impact` | Meaning | Verdict effect |
|---|---|---|
| `introduced` | The vulnerable line is an added line in the diff, or sits in a function containing one. The PR wrote this. | **Fails** the check |
| `aggravated` | The flaw predates the PR, but the PR makes it worse: more severe, more reachable, more exploitable, or newly chainable. | **Fails** the check |
| `pre_existing` | Present before, and this PR does not change its severity or reachability. | Never fails, reported as a warning |

The split exists because the author can only be asked to fix what the PR is
responsible for. Blocking a merge over code the author never touched is how a
required check gets removed from branch protection.

### When a finding is `aggravated`

`aggravated` is a strong claim, since it blocks a merge over a line the author
did not write, so it needs a stated reason meeting one of these. Anything that
does not is `pre_existing`.

| Criterion | Example |
|---|---|
| **Reachability increased** | The PR adds a route, handler, or caller that reaches a sink which previously had no reachable path, or was reachable only from an authenticated or internal one. |
| **Guard weakened or removed** | The PR deletes or loosens an auth check, permission test, validation, or escaping step that was containing a pre-existing sink. |
| **Input surface widened** | The PR adds a parameter, body field, or looser type that feeds an existing sink with data it could not previously carry. |
| **Newly chainable** | `sh-chainer` produces a chain with at least one `introduced` member and at least one pre-existing member. The pre-existing member becomes `aggravated`. |
| **Blast radius increased** | The same flaw now spans more tenants, more records, or a higher privilege level because of what the PR changed. |

Every `aggravated` finding must set `pr_scope_note` naming the specific change
and the criterion it meets. A note that only restates the vulnerability is not a
justification, so downgrade it to `pre_existing`.

**When genuinely unsure, choose the lower label.** A false `pre_existing` costs a
warning that someone reads. A false `aggravated` blocks a merge over something
the author cannot reasonably own, and the next conversation is about disabling
the check.

## Posting policy

| Finding | Where it goes |
|---|---|
| `introduced` / `aggravated`, severity >= medium, confidence >= 80, anchorable | **Inline comment** |
| `introduced` / `aggravated`, severity >= medium, confidence >= 80, not anchorable | Body, "Findings without a diff anchor" |
| `introduced` / `aggravated`, confidence 60 to 79 | Body, "Lower-confidence, for reviewer judgment" |
| `introduced` / `aggravated`, severity low | Body, collapsed |
| `pre_existing`, any severity | Body, collapsed, "Pre-existing, not introduced by this PR" |
| confidence < 60 | Not reported. It stays in `findings.jsonl` on disk. |

An `aggravated` finding anchors to the **PR's** line, the change that worsened
it, not to the pre-existing vulnerable line, which is usually outside the diff
and would not anchor anyway. The comment explains both: what the PR changed, and
what that change now exposes.

**Inline cap: 10.** Past ten, keep the ten highest by (severity, confidence) and
move the rest into the body with a line saying how many were collapsed and why.
Ten inline comments is already a lot to answer in one sitting.

**Tier 1 findings carry a caveat line** in their body text: *"Tier 1 review,
hunter confidence, not independently verified."* Never present an unverified
finding as verified.

### Inline comment body

````markdown
**<Severity>** · <CWE-NNN> · confidence <N>/100

<One sentence: what is wrong here.>

<Why it is exploitable, the precondition, concretely. Two sentences at most.>

**Suggested fix**
```<lang>
<the corrected code>
```

<Optional: "Chains with <FINDING-ID>, see the review summary.">

<sub>`<FINDING-ID>` · sh-pr-review · tier <T></sub>
<!-- sh-pr-review:fp=<hash> -->
````

Keep it short. The detail lives in `REVIEW.md` and the audit trail; the inline
comment exists to be read in a diff view and acted on.

### Review summary body

```markdown
## Security review: <P> finding(s) in this PR

<Verdict line: one sentence a reviewer can act on.>

| Severity | Introduced by this PR |
|---|---|
| Critical | N |
| High | N |
| Medium | N |
| Low | N |

<Inline count> posted inline · <D> already raised on an earlier push · <X> pre-existing

### Findings without a diff anchor
<Those whose vulnerable line is not in the diff: file:line and one line each.>

### Lower-confidence, for reviewer judgment
<60 to 79 confidence, one line each.>

<details><summary>Pre-existing, not introduced by this PR (<X>)</summary>

<One line each. These were not written by this PR and are not blocking it.>
</details>

### Resolved since the last review
<Fingerprints that were raised before and are gone now.>

---
<sub>Tier <T> review · <S> analysis agents · scanned <head_sha> · nothing was executed against a running application, payloads are constructed from source.</sub>
```

## Verdict and commit status

The review posts comments; the **commit status** is what a branch protection rule
can enforce. It is the part that makes this a gate rather than an opinion.

### Computing the verdict

```
blocking = findings where
      pr_impact in (introduced, aggravated)
  and severity >= <fail-on threshold, default medium>
  and confidence >= 80

fail    if blocking is non-empty
warn    if blocking is empty and any finding is reported
pass    otherwise
```

| Verdict | `state` | When |
|---|---|---|
| **fail** | `failure` | The PR introduced or aggravated at least one finding at or above the threshold |
| **warn** | `success` | Nothing introduced or aggravated; pre-existing or low-confidence findings reported |
| **pass** | `success` | No findings, or triage stopped at Tier 0 |
| **error** | `error` | The review could not complete: a hunter crashed, the diff could not be fetched |

`warn` deliberately reports `success`. A warning that blocks a merge is a failure
with extra steps, and teams respond by removing the check. The warning lives in
the description and the review body, where it is read without blocking anyone.

**`error` is not `failure`.** A broken review must never look like a clean one,
but it must also not accuse the PR of a vulnerability it did not find. `error`
surfaces as a distinct state, so a required check still stops the merge while the
message says the tooling broke, not the code.

The threshold is `--fail-on=<critical|high|medium|low>`, default `medium`. The
confidence floor of 80 is fixed: below it a finding is a lead, and leads must not
block merges.

### Setting the status

Use the **Commit Statuses API**, not the Checks API. Check runs require a GitHub
App installation token; `gh auth` issues a user token and the call returns 403.
Statuses work with both, and are what branch protection consumes anyway.

```
gh api repos/<OWNER>/<REPO>/statuses/<HEAD_SHA> \
  --method POST \
  -f state=<pending|success|failure|error> \
  -f context=security/pr-review \
  -f description='<140 chars or fewer>' \
  -f target_url='<link to the review or the run log>'
```

- `<HEAD_SHA>` is the PR's `headRefOid` captured in Phase 0, the commit that was
  actually analysed. Never `HEAD`, which may have moved.
- `context` is the check's identity in branch protection. Default
  `security/pr-review`; `--status-context=<name>` overrides it. **Keep it
  stable**, since renaming it silently orphans the branch protection rule, which
  then waits forever for a check that no longer reports.
- `description` is truncated hard by GitHub at 140 characters. Write it to be
  read at that length.
- `target_url` points at the posted review when there is one, else the run
  directory.

### Status lifecycle

1. **Phase 0**: set `pending` with `Security review in progress` as soon as the
   PR and head SHA resolve. A required check that never reports blocks the PR
   indefinitely and looks like a hang; `pending` makes the state legible.
2. **Final phase**: set the computed verdict.
3. **Any abort path**: if the run fails after `pending` was set, set `error` with
   the reason. Never leave a PR stuck on `pending`; that is indistinguishable
   from a crashed runner and will get the check disabled.

### Description templates

| Verdict | Description (140 chars or fewer) |
|---|---|
| fail | `3 issue(s) introduced by this PR - 1 critical, 2 high. See the review for fixes.` |
| fail (aggravated only) | `2 pre-existing issue(s) made worse by this PR. See the review for what changed.` |
| warn | `No new issues. 4 pre-existing finding(s) reported for awareness.` |
| pass | `No issues found in the changed code.` |
| pass (Tier 0) | `No security-relevant changes in this PR.` |
| error | `Review did not complete: <short reason>. Re-run before merging.` |

## API payload

One POST creates the whole review, so it lands as a single reviewable unit rather
than N separate notifications.

`review-payload.json`:

```json
{
  "commit_id": "<head_sha>",
  "event": "COMMENT",
  "body": "<the review summary body>",
  "comments": [
    {"path": "app/api/orders.py", "line": 42, "side": "RIGHT", "body": "<inline comment body>"}
  ]
}
```

```
gh api repos/<OWNER>/<REPO>/pulls/<N>/reviews --method POST --input <LOG_DIR>/review/review-payload.json
```

- `commit_id` pins the review to the head SHA that was actually analysed. Without
  it, a review can land against a newer commit the scan never saw.
- `event` is **always `COMMENT`**. Never `REQUEST_CHANGES` (blocks a merge) or
  `APPROVE` (signs off work nobody read). A bot that can block merges loses its
  token, and the commit status is already the enforcement mechanism.
- An empty `comments` array is fine. A body-only review is normal when nothing
  anchored.

### Failure handling

| Status | Meaning | Response |
|---|---|---|
| 422 with `line is not part of the diff` | Anchoring bug | Move that finding to the body and repost. Do not shift the line number. Record it: repeated 422s mean `diff-index.json` is wrong. |
| 403 | Token lacks PR write | Stop. Report it. Keep the payload; the user can post it. |
| 404 | Wrong repo/PR, or no access | Stop. Do not create anything elsewhere. |
| 5xx or rate limit | Transient | Retry once. On a second failure keep the payload and report. |

**Never fabricate a posted-comment URL.** Receipts come from the API response
only. If the POST failed, `REVIEW.md` says the review was not posted.

## Enforcing the check on a repository

A commit status only gates a merge once branch protection requires it. Two steps,
in this order, and the order matters.

This is optional, and it is a separate decision from using the skill. Most people
should run `sh-pr-review` from their own machine on their own PRs for a while
first. Automate it only once you know what it says about your codebase.

### 1. Let it report on every PR first

Before requiring the check, make sure something sets it on every pull request. A
required status that nothing reports leaves every PR blocked on a check that
never arrives, and the first fix anyone reaches for is deleting the rule.

Drop this into your own repository as
`.github/workflows/security-pr-review.yml`:

```yaml
name: Security PR Review

on:
  pull_request:
    types: [opened, synchronize, reopened, ready_for_review]

permissions:
  contents: read # read the diff
  pull-requests: write # post the review
  statuses: write # set security/pr-review

concurrency:
  group: sh-pr-review-${{ github.event.pull_request.number }}
  cancel-in-progress: true

jobs:
  review:
    runs-on: ubuntu-latest
    timeout-minutes: 30
    # A fork PR gets a read-only token, so the review could never post and the
    # status could never be set. Skip rather than leave a red cross on a
    # contributor's first PR.
    if: github.event.pull_request.head.repo.full_name == github.repository
    steps:
      - uses: actions/checkout@v4
        with:
          fetch-depth: 0 # the merge-base needs full history
      - uses: anthropics/claude-code-action@v1
        with:
          claude_code_oauth_token: ${{ secrets.CLAUDE_CODE_OAUTH_TOKEN }}
          # Pass this explicitly. Without it the action swaps its OIDC token for
          # a Claude App token, and that exchange refuses to mint one while the
          # calling workflow is itself new or modified in the PR under review.
          # The step then exits as a *success* having done nothing.
          github_token: ${{ secrets.GITHUB_TOKEN }}
          claude_args: '--max-turns 80 --allowedTools "Read,Write,Edit,Glob,Grep,Bash,Task,Skill"'
          prompt: |
            Run the sh-pr-review skill on pull request
            #${{ github.event.pull_request.number }} with --ci.
        env:
          GH_TOKEN: ${{ secrets.GITHUB_TOKEN }}
          SH_PR_REVIEW_AUTOPOST: "1"
```

`cancel-in-progress` matters: without it, two reviews of different commits race,
and the loser overwrites the winner's status with a verdict for a stale SHA.

**Add a fail-safe if you make the check required.** The skill sets `pending`
before it analyses anything and owns every exit path after that, but it cannot
cover the job itself dying, or the agent step exiting as a no-op success. Either
leaves a required check stuck with no terminal status, which is
indistinguishable from a hang and is what gets the rule deleted. A final step
with `if: always()` that reads the current status and posts `error` when it finds
`pending` or nothing closes that gap.

### 2. Require it

Once it reports reliably on real PRs, make it required.

**UI**: Settings -> Branches -> branch protection rule for `main` -> *Require
status checks to pass before merging* -> add `security/pr-review`.

**API**, as a ruleset:

```
gh api repos/<OWNER>/<REPO>/rulesets --method POST --input - <<'JSON'
{
  "name": "Require security PR review",
  "target": "branch",
  "enforcement": "active",
  "conditions": {"ref_name": {"include": ["~DEFAULT_BRANCH"], "exclude": []}},
  "rules": [{
    "type": "required_status_checks",
    "parameters": {
      "strict_required_status_checks_policy": false,
      "required_status_checks": [{"context": "security/pr-review"}]
    }
  }]
}
JSON
```

Or on classic branch protection:

```
gh api repos/<OWNER>/<REPO>/branches/main/protection/required_status_checks \
  --method PATCH -f strict=false -f 'contexts[]=security/pr-review'
```

### Operating it

- **The context string is the contract.** `security/pr-review` is what branch
  protection matches on. Renaming it orphans the rule, and PRs then block on a
  check that will never report. Change it in the rule and the workflow together,
  or not at all.
- **Give people a documented way through.** A blocked PR with no escape hatch
  gets the rule deleted the first time the reviewer is wrong. Decide up front
  whether that is an admin override, a `security-reviewed` label that a follow-up
  job honours, or a time-boxed `--fail-on=high`. Write it down where the team
  will find it, including when it expires.
- **The default threshold is `medium`, and it stays there.** Medium is where
  the findings that actually get exploited live, so raising the bar to `high`
  to keep the board green is a decision to ship known-exploitable code. If the
  first runs on an unscanned codebase produce a wall of red, the fix is
  `pre_existing`, not a higher threshold: those findings do not block a merge,
  and only what a PR introduces or aggravates ever does. A wall of red on day
  one means the PRs really are introducing medium-severity issues, which is
  exactly what you turned the check on to learn. `--fail-on=high` exists for a
  deliberate, temporary, written-down exception, not as a starting position.
- **Pre-existing findings never block.** That is deliberate and worth telling the
  team explicitly, because the first question when the check goes red is always
  "why is it failing on code I didn't touch", and the answer must be that it is
  not.
- **A failed run is `error`, not `failure`.** If the check is red, read the
  description before reading the code: the tooling may simply have broken.

## Receipts

`<LOG_DIR>/review/receipts.jsonl`, one line per decision:

```json
{"finding_id":"AC-001","fp":"9f2c1a4b7e03","action":"posted_inline","path":"app/api/orders.py","line":42,"comment_url":"https://github.com/o/r/pull/412#discussion_r123","at":"2026-09-27T15:02:11Z"}
{"finding_id":"SQL-002","fp":"3d81ff90aa12","action":"skipped_duplicate","seen_on":"review","at":"2026-09-27T15:02:11Z"}
{"finding_id":"CRY-004","fp":"77aa10bc9de5","action":"body_only","reason":"not_anchorable","at":"2026-09-27T15:02:11Z"}
```

Actions: `posted_inline`, `posted_body`, `body_only`, `skipped_duplicate`,
`skipped_below_bar`, `skipped_pre_existing`, `failed`.

## Scope

This skill posts to the pull request and sets a commit status. That is the whole
of its outward-facing surface. It does **not** create issues, tickets, or tasks
in any external tracker, and it does not call out to any third-party service.
Findings that outlive the PR belong in the repository's own issue tracker, opened
by a human who decided they were worth tracking.
