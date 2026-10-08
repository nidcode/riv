# re:Invent 2026 — 設計

欲しいアジェンダを宣言します。`riv plan` が実際のスケジュールとの差分を表示し、`riv apply` が承認済みの plan を適用します。

- `id` は API の `sessionId` です（`riv search` で探せます）。短いコードではありません。
- `want`: `reserved` | `favorite` | `none`。行を削除しても何も取り消されません。取り消すときは `want: none` を使います。
- `pin: true` は固定です。riv は触りませんが、衝突の判定には使います。
- `replaces: <sessionId>` は予約の置換です（先に取り消してから予約します。元の席を失う可能性があります）。
- `blocks` の時刻は `timezone` の現地時刻です。ブロックに `want: none` を書くと、riv が作った個人時間を削除します（手で作ったものには触れません）。

```yaml
# riv:desired-state v1
event: reinvent2026
timezone: America/Los_Angeles
sessions: []
blocks: []
```
