# アーキテクチャ

## 1 つのエンジン、3 つのインターフェース

```mermaid
flowchart LR
    subgraph Faces["インターフェース"]
      CLI["CLI<br/>riv plan / apply / ..."]
      MCP["MCP + Kiro Power<br/>riv mcp (stdio)、スキル、ステアリング、フック"]
      CREW["Kiro Crew<br/>Slack / Telegram -> Gateway -> riv mcp"]
    end
    CLI --> ENG
    MCP --> ENG
    CREW --> MCP
    subgraph ENG["エンジン (lib `riv`)"]
      DESIRED["desired/<br/>パース + 検証"]
      PLAN["plan/<br/>純粋な差分計算"]
      APPLY["apply/<br/>ステートマシン + ジャーナル"]
      SEARCH["search/ + db/<br/>SQLite + FTS5"]
      SYNC["sync/"]
      TODAY["today/ prep/"]
    end
    SYNC --> SEARCH
    DESIRED --> PLAN --> APPLY
    SEARCH --> PLAN
    SEARCH --> TODAY
    APPLY -->|EventsApi trait| API
    SYNC -->|EventsApi trait| API
    subgraph IO["外部 I/O (ここだけ)"]
      API["api/ HttpApi<br/>401 リフレッシュ, 429, 409, 5xx"]
      AUTH["auth/<br/>PKCE、トークンストア"]
    end
    API --> AUTH
    API ==> AWS[("api.awsevents.com")]
    API -.-> MOCK["mock/ (axum, インプロセス)<br/>合成カタログ + 障害シナリオ"]
```

ロジックは lib が持ち、`main.rs`、`mcp/`、`mock/` は薄いエントリポイントです。ネットワークと通信するものはすべて `EventsApi` トレイト（`api/`）と `auth/` を経由するため、テストでは空きポート上の mock サーバーに差し替えられます。plan / apply の判断は、単純なデータに対する同期的な純粋関数（`plan::build_plan`、`apply::verify`）であり、非同期なのは I/O の境界だけです。

## 各ルールの所在（単一の情報源）

| 知識 | 場所 |
|---|---|
| REST オペレーション（メソッド + パス） | `src/api/ops.rs`。`tests/openapi_conformance.rs` が `openapi/awsevents.v1.json` と照合します |
| 分あたりのクォータ | `src/api/quota.rs` |
| UTC / ローカル / ワイヤー形式の時刻フォーマット | `src/timeutil.rs` |
| 望ましい状態（desired state）のセマンティクス | `src/plan/build.rs`。`tests/plan_semantics.rs` で固定されています |
| apply のセマンティクス | `src/apply/exec.rs`。`tests/apply_dod.rs` で固定されています |
| ユーザーに表示する文言 | `src/i18n/`（英語の文言がキーです） |

## plan / apply ステートマシン

```mermaid
stateDiagram-v2
    [*] --> Planned: riv plan (保存、30 分で期限切れ)
    Planned --> Rejected: 期限切れ / spec ハッシュの変更 /<br/>スケジュールハッシュの変更 / 別アカウント /<br/>--accept-seat-loss がない
    Planned --> Executing: riv apply --plan (確認済み)
    state Executing {
      [*] --> Action
      Action --> DONE: 2xx、バルク結果で成功
      Action --> ALREADY: すでに予約済み / お気に入り済み、削除時の 404
      Action --> FAILED: 満席、重複、予約不可、...
      Action --> SKIPPED: 409 closed (同じ種別は再送しない)、依存先が未完了
      Action --> UNKNOWN: タイムアウト / 切断 / 500 / 503
    }
    Executing --> Stopped: いずれかが UNKNOWN (書き込み停止)
    Stopped --> Executing: riv apply --resume (GetSchedule で突き合わせ、足りないものだけ送信)
    Executing --> Verified: GetSchedule で読み戻し、tasks.md を再生成
    Verified --> [*]
```

置き換えは `cancel` の後に `reserve` を行うものです（reserve は cancel に `dependsOn` します）。cancel が成功した後で reserve が失敗した場合、riv は元の席を 1 回だけ予約し直そうとし、「restored」または「seat lost」を報告します。ただし、席の復元は決して保証されません。

## データ

SQLite（`~/.local/share/riv/riv.db`）: `events`、`sessions`（+ 生の JSON とハッシュ）、`sessions_fts`、`sync_runs`、およびジャーナルとして `runs`、`run_actions`、`managed`、`block_ids`、`prep_notes`。Plan は `~/.local/share/riv/plans/` 内の JSON ファイルです。
