# Search-quality fixtures

- `queries.mock.json` — 10 questions over the **synthetic** mock catalog (made by us). Run: `riv bench search-quality --mock`.
- `queries.json` — your 30 questions over the **real** catalog; empty until you fill it. Format:

```json
[{ "query": "how do I secure agentic workflows", "expected": ["<sessionId>", "<sessionId>"], "level": 300 }]
```
`expected` are API sessionIds (find them with `riv search ... --fields id,code,title`). `level` is optional. Run: `riv bench search-quality` after `riv login`.

recall@10 per question = fraction of its expected ids found in the top 10; the reported number is the average.
