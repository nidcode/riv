# riv — re:Invent as Code

English: [README.md](README.md)

re:Invent の予定を [Kiro](https://kiro.dev) の spec（Markdown）として宣言し、`riv plan` で差分を確認し、`riv apply` で反映します。セッションカタログはローカルの SQLite データベースに置かれ、**LLM のコンテキストには一切入りません**。入るのは回答だけです。

> 中心となる約束: 今週の目標を変えても、すでに確保した席を失わずに予定を更新できます。riv が作成していないものには決して触れず、置き換えでは席を失う可能性があることをはっきり伝え、承認した plan がなければ何も書き込みません。

re:Invent 2026 に登録している AWS Heroes / Community Builders、特に Kiro ユーザー向けです。riv は**あなた自身のマシン上**で動作します（API はトークンを `api.awsevents.com` 以外へ送ることを禁じているため、ホスト型の部分はありません）。

記事（AWS Builder Center）: https://builder.aws.com/project/3KOnbvKWkMlzzcInr6wKnZsJV0x/managing-reinvent-sessions-as-code-riv  
置換と席喪失ガードのデモ: [docs/demo.md](docs/demo.md)

## インストール

いずれか 1 つを選んでください（最初のリリース後に `nidcode` を置き換えてください。[docs/progress.md](docs/progress.md) を参照）。

```sh
# 1. shell installer (macOS / Linux)
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/nidcode/riv/releases/latest/download/riv-reinvent-installer.sh | sh
# Windows: powershell -c "irm https://github.com/nidcode/riv/releases/latest/download/riv-reinvent-installer.ps1 | iex"

# 2. npx (no install; this is what the Kiro Power uses)
npx -y riv-reinvent --help
```

ソースから（開発者向け）: `cargo install --path .`（Rust stable、edition 2024 が必要です）。

## 30 秒クイックスタート（サインイン不要、合成データ）

```sh
riv mock &                                   # mock Events API on http://localhost:8787 (synthetic catalog)
export RIV_API_BASE=http://localhost:8787 RIV_TOKEN=mock-token RIV_EVENT=demo-reinvent
riv sync                                     # downloads all pages (page sizes vary on purpose)
riv search agents bedrock --fields id,code,title,start,seat --limit 5
riv init --event demo-reinvent --goal "learn agents" --interests "Bedrock" --constraints "none"
```
`.kiro/specs/reinvent-2026/design.md` を開き、末尾の YAML ブロックで `sessions: []` を、席が `available` のセッション 1 件に置き換えます。
```yaml
sessions:
  - {id: "mock-0001", want: reserved}
```
```sh
riv plan                                     # shows the diff; writes nothing to the API
riv apply --plan <planId printed by riv plan>   # asks to confirm on a TTY (or pass --yes)
riv verify                                   # spec vs real schedule
riv schedule                                 # what you hold now
cat .kiro/specs/reinvent-2026/tasks.md       # generated checklist (do not edit)
```
失敗も試せます: `riv mock --scenario closed-reservations`（409）、`throttle:5`、`full:<sessionId>`、`clash`、`drop-after-write`（結果不明）、`edge-html-500`。

## 実際の API を使う（まずは読み取り専用）

```sh
riv doctor            # sign-in, catalog, API reachability
riv events            # public listing, no sign-in
riv login             # Builder ID, OAuth 2.0 + PKCE; the callback uses one of ports 8484-8489
riv sync              # event reinvent2026; add --locale ja-JP for localized text when the server provides it
riv search "agentic" --level 300 --day tue --free-between 13:00-15:00
```
re:Invent 2026 は登録制です。カタログの読み取りにはサインイン**と**イベント登録の両方が必要です。予約・キャンセルは、受付が開始されるまで（re:Invent 2026 では 2026-10-08）`409` を返します。閲覧とお気に入りはそれ以前でも使えます。認証情報は `~/.config/riv/credentials.json`（モード 0600。Windows では `%APPDATA%\riv` 配下で、ユーザープロファイルによる保護のみです。マシンの扱いには注意してください）に保存されます。`riv logout` で失効させたうえで削除します。

## Kiro Power

このリポジトリのルートがそのまま Power です（`plugin.json`、`mcp.json`、`skills/`、`dev.kiro/`）。
- **GitHub から**: Kiro IDE → Powers → *Import from GitHub* → このリポジトリの URL。`mcp.json` は `npx -y riv-reinvent@latest mcp` でサーバーを起動します。
- **ローカル / npm リリース前**: `mcp.local.json`（インストール済みの `riv mcp` を実行）を使うか、チェックアウト内で `cargo run --quiet -- mcp` を実行します。
- ツール: `riv_status`、`riv_search`、`riv_session`、`riv_schedule`、`riv_plan`、`riv_apply`、`riv_verify`、`riv_today`、`riv_prep_pack`（応答は小さく、必要なものはすべて `format: "ide"|"phone"` を受け付けます）。
- スキル: `setup`、`replan`、`prep`。ステアリング: `dev.kiro/steering/riv.md`。フックの例: `dev.kiro/hooks/`（`.kiro/hooks/` にコピーしてください）。
- Kiro なしでツールを確認する: `npx @modelcontextprotocol/inspector riv mcp`。
- Kiro Crew でスマートフォンから使う: [docs/crew.ja.md](docs/crew.ja.md)。

## 変更が適用される流れ（安全モデル）

1. **Spec**: `design.md` 内の YAML ブロックで望む状態を記述します（`reserved | favorite | none`、`pin`、`replaces`）。
2. **Plan**: `riv plan` が実際のスケジュールを読み取り、`plan.json` を書き出します（有効期限 30 分。spec のハッシュ、観測したスケジュールのハッシュ、あなたのアカウントに紐づきます）。語彙: `+ reserve`、`- cancel`、`-/+ replace (seat may be lost)`、`~ favorite`、`# unmanaged (untouched)`、`! re-reserve (may be full)`。
3. **Apply**: `--plan` を指定したときだけ実行できます。plan の期限切れ、spec やスケジュールの変更、アカウントの不一致、置き換えに対する `--accept-seat-loss` の欠如のいずれかがあると拒否されます。実行順序: cancel → unfavorite → reserve → favorite → personal time。分あたりのクォータの範囲内で 10 件以下のバッチに分け、`429` では `Retry-After` の間待機し、`409` ではその種別をスキップして続行します。
4. **結果不明の場合**（タイムアウト、接続の切断、500/503）は、盲目的に再送しません。書き込みを止め、`riv apply --resume <runId>` が `GetSchedule` と突き合わせ、足りないものだけを送ります。
5. **Verify** は apply のたびに実行され、`tasks.md` が再生成されます。

riv が**行わない**こと: 空席のポーリング / 自動予約、承認された plan なしでの apply、riv が作成していない予約への操作（それらは `unmanaged` と表示されます。spec の行を削除しても何もキャンセルされません。キャンセルするには `want: none` と書きます）。詳細: [docs/rules-compliance.ja.md](docs/rules-compliance.ja.md)、[docs/architecture.ja.md](docs/architecture.ja.md)。

## コマンド

`riv show <id> --live` は、前回の同期ではなく API（GetSession）に今の席の状況を問い合わせます。spec の個人時間ブロックは、作成・更新に加え、`want: none` で削除もできます。

`init`、`login`、`logout`、`whoami`、`events`、`sync`、`search`、`show`、`schedule`、`plan`、`apply`、`verify`、`today`、`prep`、`doctor`、`mcp`、`mock`、`bench`。機械が出力を読む場合は `--json` を使います。`RIV_LANG=ja|en` で表示言語を切り替えます。終了コード: 0 成功、1 エラー、2 検証エラー、3 認証エラー、4 plan 拒否、5 部分的な失敗。

言語: `riv config set lang ja` で保存できます（`riv config show` で有効な値の出どころが分かります）。優先順位は `RIV_LANG` > 保存した設定 > OS のロケールです。

環境変数: `RIV_API_BASE`（デフォルトは `https://api.awsevents.com`）、`RIV_TOKEN`（PKCE を省略します。mock 専用）、`RIV_EVENT`、`RIV_SPEC`、`RIV_LANG`、`RIV_CONFIG_DIR` / `RIV_DATA_DIR`、`RIV_LOG`。

## 既知の制限

- 年をまたいだセッションコードの照合（`prep` の前年ヒント）は**ヒューリスティック**です。コードは年ごとに変わります。
- `config/venues.example.json` の移動時間は**推定値**です。`~/.config/riv/venues.json` にコピーして編集してください。記載のない組み合わせは「unknown」と表示されます。
- Kiro の File Save フックが発火するのは**エージェント**による変更だけで、手動での保存では発火しません。`design.md` を手で編集した後は、`riv plan`（または `/riv-plan`）を実行してください。
- 検索は日本語の形態素解析を行わない FTS5 です。日本語の再現率には限りがあります。
- MCP 経由の `riv_apply` はホストのツール承認 UI に依存します。手動承認のままにしてください（`/yolo` は決して使わないでください）。
- Windows の認証情報は、プロファイル配下の平文ファイルです。
- personal time の更新・作成には対応していますが、削除されたブロックの削除には対応していません（stretch）。

## 開発

`cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test`。テストは `RIV_REAL_API=1`（読み取り専用）の場合を除き、ネットワークに触れません。mock サーバーは空きポート上でインプロセスで動作します。ベンチマーク: `riv bench {protocol|first-run|warm|search-quality} [--mock]`、結果は `bench/results/` にあります。リリース: タグ `vX.Y.Z` を付けると、`dist` ワークフローがバイナリ、npm パッケージ（`NPM_TOKEN` シークレット）を公開します。

このリポジトリ内のカタログデータはすべて合成データです。AWS とは無関係です。ライセンス: Apache-2.0。
