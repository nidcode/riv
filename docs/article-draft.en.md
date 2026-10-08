# Managing re:Invent Sessions as "Code": Riv

I built Riv, a tool where you write your re:Invent plan in Markdown, review the diff, and only then apply it. Its main goal is to let you rearrange your plans without losing the seats you already hold.

## The problem

- **Losing a seat.** Once you give up a reserved seat, you may not get it back. Every time you rearrange your schedule, one wrong operation can cost you a seat you had.
- **The effort of searching and comparing.** There are more than 2,000 sessions. Finding the talks you want, noticing time clashes and reshuffling is something you repeat every day.
- **Hard to hand over to AI.** The API has no search, so you have to download everything and filter locally. If you give the job to an AI assistant, the whole catalog flows into the chat.

## How it works

- The catalog is stored in a local SQLite database. The AI only gets a few lines of search results.
- You write what you want at the end of `design.md` as YAML: `reserved`, `favorite` or `none`.
- `riv plan` shows the difference from your actual schedule. It writes nothing at this point.
- `riv apply --plan <ID>` runs only that difference.

The rules it keeps:

- It never touches reservations it did not make. Deleting a line from the YAML does not cancel anything. To cancel, write `want: none`.
- Replacing a reservation cancels first and then reserves, so a seat may be lost. The diff says so, and you cannot apply it without an explicit flag.
- When the result is unknown (a timeout, for example), it stops instead of resending. `--resume` re-reads your schedule and sends only what is missing. The API has no idempotency key, so a blind resend is not safe.
- There is no feature that polls for open seats and grabs them.

## How I use the API and MCP

All 12 REST operations are implemented: reserve, cancel, favorites, and create, update and delete for personal time. I could not try personal-time create, update and delete on the real API; they are tested against a mock server.

I used the remote MCP server for comparison with REST. For local use I wrote my own MCP server with nine tools (`riv_search`, `riv_plan`, `riv_apply` and so on). Its replies are small, and `riv_apply` is meant to run only after a person approves it.

In Kiro it installs as a Power, and registered in Kiro Crew it works from your phone too.

## Measurements

I measured against the real catalog (2,195 sessions).

| Item | Result |
|---|---|
| First full sync | about 13 s (with and without abstracts) |
| Local search (50 runs) | median 0.6 ms, 95th percentile 1.8 ms |
| One page over REST | median 1.34 s, 317 KB |
| One page over the remote MCP server | median 1.64 s, 317 KB |

A page is the same size over REST and over MCP. If you let an AI read the whole catalog through MCP, pages of this size go through it dozens of times. If you keep the catalog locally and search it, only a few lines reach the AI. Timing was measured five times, and MCP was about 0.3 s slower.

## Using it myself

I built my own plan. I narrowed the catalog down by my interests and put seven sessions into my favorites. Then I noticed that a GameDay (three hours) overlapped other sessions. A clash like that is easy to miss in a list, but it showed up at the diff step.

On October 8 the API still returned 409 for reservations; they were already open on the official site. So I reserved by hand. When the API opens, `riv plan` and `riv apply` will add only what is missing.

## What is not done

- Matching session codes across years is a guess.
- Walking times between venues have to be written to a config file yourself. I ship no values.
- Japanese search is not very accurate because there is no morphological analysis.

## How to use it

You can try it without signing in, against a mock server.

```
riv mock &                       # start a mock API
export RIV_API_BASE=http://localhost:8787 RIV_TOKEN=mock-token RIV_EVENT=demo-reinvent
riv sync                         # download the catalog
riv search agents bedrock        # find candidates (the left column is the sessionId)
riv init                         # answer three questions to create the spec
```

Put the sessions you want into the YAML at the end of `.kiro/specs/reinvent-2026/design.md`.

```yaml
sessions:
  - {id: "mock-0001", want: reserved}
```

```
riv plan                         # review the diff (writes nothing yet)
riv apply --plan <planID>        # apply exactly that diff (within 30 minutes)
riv verify                       # compare the spec with your real schedule
```

With the real API, sign in first with `riv login` and remove the `RIV_*` settings. While reservations are closed, you can try `want: favorite`.

See the README for details. I learned a lot from earlier projects: reinvent26-planner, re:Plan 2026, reinvent-scout and reinvent2026-mcp.
