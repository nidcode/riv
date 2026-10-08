# Rules compliance

> Status: checked against the official "re:Invent Catalog API Builder Challenge" page and rules (updated 2026-09-24) on 2026-10-08. Items that only the participant can attest are marked **(you)**.

## Contest rules at a glance
- Period: 2026-09-24 10:00 PT to **2026-11-06 23:59 PT** (submission deadline). Winners by 2026-11-27; top 3 featured at re:Invent (Nov 29 - Dec 4).
- Submission needs **all** of: (1) a public code repository with working code and a README covering setup, dependencies and how to run; (2) an article published on **builder.aws.com** (a published AWS Builder Center project) explaining what was built, why, and how the API and/or MCP server is used; (3) original work by the participant.
- Eligibility **(you)**: current AWS Heroes or Community Builders member, registered for re:Invent 2026, 18+, not resident in an excluded country/region, not an Amazon/AWS employee or close relative/household member; one submission per person.
- Judging (25% each): creativity and novelty of the integration; usefulness to re:Invent attendees; technical depth and use of the API surface; quality of the Builder Center project.
- Warranties: the submission must not facilitate illegal acts, infringe others' IP, be offensive/defamatory, or harm others or AWS's business or reputation.

| Requirement | Status |
|---|---|
| Public repo with working code | done: https://github.com/nidcode/riv (Apache-2.0) |
| README with setup, dependencies, how to run | done (install, quick start, real API); add an explicit "Requirements" list (see docs/progress.md) |
| Article on builder.aws.com | **open** — outline only (`docs/article-outline.md`) |
| Original work | done; prior projects are credited in the article, not copied |
| Built in the contest period | done: first commit 2026-10-05 |
| Eligibility attestations | **(you)** |


## Forbidden by the rules / API terms (riv complies)

| Rule | How riv complies |
|---|---|
| No harm to other participants or AWS staff | Only your own schedule is read and written; no scraping of other people's data; no bulk catalog redistribution. |
| No IP infringement | All sample/mock data is synthetic and generated in code (`src/mock/catalog.rs`). The real catalog is never committed or shipped; it exists only in the user's local database. |
| Original work | The code is written for this project; prior projects (reinvent26-planner, re:Plan 2026, reinvent-scout, reinvent2026-mcp) are credited in the article, not copied. |
| API quotas | Per-operation per-minute quotas are tracked client-side (`src/api/quota.rs`); batches shrink to the remaining quota; `429` waits for `Retry-After`; no fixed-sleep hammering. Real-API tests are read-only and opt-in (`RIV_REAL_API=1`). |
| Token handling | The access token is sent only to the API base (`api.awsevents.com`; overridable only for the mock). Tokens are never logged, never put in URLs, errors, commits or structured logs (`Credentials` has a redacting `Debug`). Stored in a 0600 file; refresh token replaced when rotated; `riv logout` revokes then deletes. |
| 401 handling | Refresh once, retry once, then stop (no loops). |
| Registration required | The tool signs in as the attendee and relies on their registration; it does not try to bypass authentication. |

## Things riv chooses not to do (self-imposed)

- **No polling auto-reservation.** There is no watch/retry loop for open seats. Writes happen only for an approved plan, once.
- **No apply without an approved plan.** `riv apply` requires `--plan` (or `--resume` of a run made from one); plans expire after 30 minutes and are bound to the spec, the observed schedule and the account.
- **No interference with reservations riv did not create.** They are listed as `unmanaged (untouched)`; removing a row from the spec never cancels; `want: none` on an unmanaged reservation only warns.
- **No blind resend after an unknown outcome**; reconcile with `GetSchedule` first.
- **No repeated real-API write benchmarks.** The bench commands read only, or run against the mock.
- **No video transcripts** are fetched for prep notes; quotes stay short with source links.
- **No hosted component.** Everything runs on the attendee's machine.
