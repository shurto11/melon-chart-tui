---
name: verify
description: melon-chart-tui をビルドして tmux 上で起動し、変更を実画面で検証する手順
---

# melon-chart-tui の検証手順

## ビルドと起動

```bash
cargo build
tmux -L verify new-session -d -s app -x 160 -y 40 './target/debug/melon-chart-tui'
sleep 6   # guyso.me からのチャート取得を待つ
tmux -L verify capture-pane -t app -p
```

起動直後は TOP100 一覧（Ranking画面）。データ取得にネットワーク必須。

## 主要キー操作（tmux send-keys で送る）

- `]` / `[` — チャート切替（TOP100 ⇄ 일간）。切替後は再取得に数秒待つ
- `j` / `k` — スクロール、`l` — 曲詳細、`h` — 戻る
- `/` + 文字列 + Enter — 検索、`n`/`N` — 移動
- `r` — 再取得、Ctrl-C — 終了

## 後片付け

```bash
tmux -L verify kill-server
```

## 注意点

- 이용자（利用者数）列は일간チャートのみ表示される
- ASCIIアートのアルバムサムネイルは非同期取得のため、キャプチャ前に数秒待つ
- 「Exec format error」が出たら成果物破損。`cargo clean -p melon-chart-tui && cargo build` で復旧（バイナリは deps 成果物へのハードリンクなので、バイナリ削除だけでは直らない）
