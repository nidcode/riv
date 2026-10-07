# Kiro Crew でスマートフォンから riv を使う

Kiro Crew は、デスクトップアプリ、Web ダッシュボード、Slack/Telegram/Discord などから到達できる 1 つの Gateway です。riv はローカルの MCP サーバーとスキルとして、これに組み込まれます。**riv にはホスト型の部分がありません**。アクセストークンを送ってよいのは `api.awsevents.com` だけなので、Gateway は*あなたの*マシン（ホテルのノート PC や自宅のマシン）で動かす必要があり、riv はその隣で動作します。

```
Phone (Slack / Telegram)  ->  Crew channel  ->  Gateway (your PC)  ->  riv mcp (stdio)  ->  api.awsevents.com
                                   ^ approval buttons
```

## 1. マシンを準備する
1. riv をインストールして `riv login` を実行し、続けて `riv sync --event reinvent2026` を 1 回実行します（README を参照）。認証情報は `~/.config/riv/credentials.json`（0600）に保存されます。これは出発*前*に済ませてください。サインインにはそのマシン上のブラウザが必要です。
2. Kiro Crew をインストールし、Gateway を起動します（`kirocrew gateway`）。起動したままにします（Crew の「Running 24/7」を参照）。チャンネルは Gateway から外向きに接続するため、ポートを外部に公開する必要はありません。
3. `riv init` でアジェンダ用のプロジェクトフォルダを作成し、そのマシン上に `.kiro/specs/reinvent-2026/` が存在するようにします。

## 2. riv の MCP サーバーとスキルを登録する
Gateway の MCP 設定（Crew ダッシュボードの MCP パネル、またはその設定ファイル）に、`mcp.local.json` と同じ定義で MCP サーバーを追加します。

```json
{ "mcpServers": { "riv": { "type": "stdio", "command": "riv", "args": ["mcp"], "env": { "RIV_SPEC": "/path/to/.kiro/specs/reinvent-2026/design.md" } } }
}
```

Crew の公式ドキュメントによると（2026-10-08 確認）:
- **MCP**: ダッシュボードの *Agent Capabilities → Integrations (MCP)* でコマンド（`riv`、引数 `mcp`）を指定してサーバーを追加し、*Probe* で応答を確認します。*Discover & Sync* は既存の MCP 設定ファイルからサーバーを拾います。
- **スキル**: `~/.kiro/crew/skills/` にフォルダごとコピーします（例: `cp -r skills/* ~/.kiro/crew/skills/`。各フォルダに `SKILL.md` が必要）。または *Settings → Skills → Discover* で GitHub から取り込めます（`nidcode/riv:skills/replan`）。取り込みはスナップショットなので、更新時は再取り込みします。任意の frontmatter キーは `triggers` と `always` です。
- 設定は `~/.kiro/crew/`（または `KIROCREW_HOME`）にあり、`kirocrew config get|set|edit` で操作できます。
- 未確認: stdio の MCP ツールに対する承認を Crew がどう表示するか。`riv_apply` を一度試して、適用前に承認プロンプトが出ることを確認してから使ってください。

## 3. 承認（重要）
`riv_apply` は書き込みを行う唯一のツールです。**手動承認**のままにしてください。エージェントがこれを呼び出すと、Crew がチャットに承認ボタンを表示します。エージェントが示した差分を読んだ後にだけ、タップしてください。
- riv に対して `/yolo` やセッション単位の Trust を使**わないで**ください。`replan` スキルとステアリングも同じことを述べています。
- `riv_apply` には直前の `riv_plan` で得た `planId` が必要です。plan は 30 分で期限切れとなり、その間にスケジュールや spec が変わっていた場合は拒否されます。

## 4. スマートフォン向けの出力
riv のすべてのツールは `format: "phone"` を受け付けます。短い行で最大 12 行、表なし、ローカル時刻で、`riv_today` では項目ごとに「leave by HH:MM」（出発時刻）のヒントが付きます。たとえば、エージェントに次のように頼みます。
- 「今日の予定は？（phone 形式）」 → `riv_today`
- 「火曜午後のレベル 300 の agentic セッションを探して」 → フィルタ付きの `riv_search`
- 「AIM301-R1 を SVS302 に入れ替えて」 → `replan` スキル: spec の編集 → `riv_plan` → チャットで差分を表示 → あなたの承認 → `riv_apply` → `riv_verify`

## 5. ここに含まれないもの
riv には Slack/Telegram のコードは含まれていません。チャンネルの設定は Crew の領域です。Crew 自身の Slack/Telegram ガイドと、許可リスト / セキュリティ設定に従い、ボットはあなた自身のアカウントだけに制限してください。
