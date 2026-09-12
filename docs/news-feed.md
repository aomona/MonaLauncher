# MarkdownニュースとRSS

## 記事の追加

`news/*.md` にUTF-8のMarkdownを追加し、`main` にpushする。サブディレクトリは走査しない。

```md
---
title: 更新のお知らせ
date: 2026-09-12
description: 今回の変更点を短く説明します。
slug: update-notice
author: 記事の著者名
---

ここに本文を書きます。**太字**やリスト、Markdownリンクを利用できます。
```

- `title`・`date`・`description` と空でない本文は必須。
- `date` は `YYYY-MM-DD`（UTC 00:00）またはタイムゾーン付きISO 8601。例: `2026-09-12T18:30:00+09:00`。時刻付きでタイムゾーンがない値や存在しない日付は拒否する。引用符は任意。
- `slug` は省略すると拡張子を除いたファイル名。Unicode NFCで正規化して小文字に揃える。文字・数字・`-`・`_` が使える。重複やパストラバーサル、Windowsの予約名はエラー。
- `author` は任意。指定すればRSSへ含める。
- 必須値の欠落、型違い、YAMLの重複キー、日付・slugの不正は対象ファイル名付きで失敗する。無視して公開しない。
- 同じ公開日時の記事はslug順。`updated` は最新記事の日付、記事が0件ならUnix epochに固定する。実行時刻・端末のタイムゾーン・ファイル更新時刻に依存しない。
- 本文は`marked`でHTML化し、RSSの`content:encoded`と記事ページに含める。生HTMLはテキストとしてエスケープする。相対リンク・画像URLは記事URLを基準に絶対URLへ変換する。画像ファイルはこの仕組みでは配信しないため、公開済みの絶対画像URLを使う。

`news/2026-09-12-feed-example.md` は動作確認用のサンプル。実際の製品リリース告知ではないので、不要になれば削除してよい。

## 設定と公開先

設定は `scripts/feed-config.ts` の一箇所にまとめている。サイトURLは `NEWS_SITE_URL` で上書き可能。その他のtitle・description・language・copyrightもここで変更する。

既定の予定URL:

- RSS: `https://aomona.github.io/MonaLauncher/rss.xml`
- 記事一覧: `https://aomona.github.io/MonaLauncher/news/`
- 記事: `https://aomona.github.io/MonaLauncher/news/<slug>/`

URLはリポジトリのGitHub Pages標準URLから設定したもの。実装時点ではPagesは未設定・未公開だった。カスタムドメインを設定する場合はローカルの既定値も変更する。Actions内では`configure-pages`が返す実際のbase URLを`NEWS_SITE_URL`へ渡す。

## 初回のGitHub設定

1. この変更を`main`へ反映する。現在のデフォルトブランチは`dev`だが、このworkflowは依頼どおり`main`のみを公開対象にしている。
2. リポジトリの **Settings → Pages → Build and deployment → Source** を **GitHub Actions** にする。
3. `github-pages` Environmentのブランチ制限がある場合は`main`からのデプロイを許可する。
4. `main`へ記事をpushするか、Actionsの **Publish news RSS** を`main`で手動実行する。

公開に使うのは `.github/workflows/generate-feed.yml`。Nodeとpnpmは既存の`mise.toml`（Node 24.18.0 / pnpm 11.20.0）を使用する。

```text
pnpm install --frozen-lockfile
  → pnpm test:feed
  → pnpm build（RSS生成 → TypeScriptチェック → Vite build）
  → dist/rss.xml・dist/news/だけをPages artifactへコピー
  → actions/deploy-pages
```

ランチャー本体・フォント・リポジトリ全体はPagesへ公開しない。生成物のcommit/pushも行わないため、生成による再実行ループは起こらない。`contents`はreadのみで、Pages公開用の権限はdeploy jobだけに付与する。ブランチ制限で手動実行も`main`だけを公開する。

生成に失敗した場合、build jobは停止し、前回の公開内容が維持される。記事を削除した場合も次回の生成で古いHTMLを除去する。

## ローカル生成・検証

```sh
mise install
pnpm install --frozen-lockfile
pnpm generate:feed
pnpm test:feed
pnpm check
pnpm test:ui
```

`pnpm generate:feed` は `public/rss.xml` と `public/news/` を出力する。`pnpm build` でも自動実行し、Viteがこれらを`dist/`へコピーする。どちらも生成物としてGit管理しない。`public/news/`は生成専用なので手書きのファイルを置かない。

`node:test`でメタデータの検証・並び順・決定性・タイムゾーン独立性・重複・失敗時の既存出力保持を検査する。既存のPlaywrightテストではブラウザのXMLパーサーを使い、RSS 2.0、名前空間、各itemの主要値、日本語とHTMLを確認する。

追加したライブラリはビルド用の`feed`・`gray-matter`・`marked`・`yaml`のみ。`yaml`は日付を文字列として読み、YAMLの自動日付変換で不正な日付が補正されるのを防ぐために使用する。専用SSG、TS実行用ランタイム、RSS用Webサーバーは不要。
