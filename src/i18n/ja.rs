//! Japanese dictionary (English text -> Japanese).

pub const ENTRIES: &[(&str, &str)] = &[
    ("Reserved", "予約済み"),
    ("Favorites", "お気に入り"),
    ("Personal time", "個人時間"),
    ("No results.", "該当なし。"),
    (
        "Synced {n} sessions ({changed} changed, {removed} removed). Catalog version {version}.",
        "{n} 件のセッションを同期しました（変更 {changed} 件、削除 {removed} 件）。カタログ版: {version}",
    ),
    ("Not signed in. Run `riv login`.", "サインインしていません。`riv login` を実行してください。"),
    ("Signed in.", "サインインしました。"),
    ("Signed out.", "サインアウトしました。"),
    ("Open this URL in your browser to sign in:", "ブラウザで次の URL を開いてサインインしてください:"),
    ("Nothing is scheduled.", "予定はありません。"),
    ("unknown", "不明"),
];

pub fn lookup(key: &str) -> Option<&'static str> {
    ENTRIES.iter().find(|(k, _)| *k == key).map(|(_, v)| *v)
}
