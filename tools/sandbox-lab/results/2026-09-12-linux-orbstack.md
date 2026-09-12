# OrbStack Linuxゲスト検証 — 2026-09-12

## 環境

- OrbStack 2.2.3（2020300）、macOS上のLinuxマシン。Dockerコンテナは使用していない。
- マシン名: `monalauncher-sandbox`。Ubuntu 24.04.5 LTS / ARM64。
- カーネル: `7.0.14-orbstack-00380-ga7e0a2dc9535`。
- 設定: メモリ上限4 GiB、CPU上限4、ディスク上限16 GiB、`--isolated`。
- macOSのファイル共有・SSH agent転送は無効。検証対象のソースだけを`git archive`で転送。
- Rust 1.97.1、Ubuntu OpenJDK 21.0.12+8、bubblewrap 0.9.0（`0.9.0-1ubuntu0.1`）。
- 検証プログラムは一般ユーザー（UID 501）で実行。`bwrap`はsetuidではない。rootは必要なパッケージのインストールにのみ使用。
- `user.max_user_namespaces=2147483647`。非特権user/network/mount/PID namespaceの作成を確認。
- `kernel.apparmor_restrict_unprivileged_userns`と`/sys/module/apparmor/parameters/enabled`は存在しなかった。sysctlやAppArmor設定の緩和は行っていない。

## 結果

| 対象          | 対照実験 | bubblewrapによる隔離実験 |
| ------------- | -------- | ------------------------ |
| Rust + 子Rust | 26/26    | 26/26                    |
| Java + 子Java | 26/26    | 26/26                    |

4回の実行すべてが成功。各26件は13操作×2世代。対照実験ではすべての操作が成功し、隔離実験では許可した3操作だけが成功した。両言語・両世代で以下を確認した。

- 共有asset・自インスタンス設定は読み取り可能。
- ゲーム領域には書き込み可能。
- 共有asset・自インスタンス設定の書き込み用openは`Read-only file system`で失敗。
- ホストの秘密ファイル相当、別インスタンス、そこへ向くsymlinkの参照先はmount namespaceに存在せずアクセス失敗。
- ホスト側（Linuxゲスト内の試験親プロセス）のTCP/UDP listenerには、隔離したnetwork namespaceから到達できない。TCPは`Connection refused`、UDPもechoに失敗。
- ホスト領域のUnix socketは公開されておらず接続失敗。
- 子プロセスには追加のサンドボックスコマンドを挟まず、同じ制限の継承を確認。Javaの子も別JVM。

LinuxとmacOSの両方で、追加修正後のRust単体テスト4件、Clippy（警告をエラー扱い）、Rustフォーマット確認も成功した。

## 検証で見つけた問題と修正

最初の実行ではRust側は合格したが、Java側はネットワーク検証の途中で`java.lang.InternalError: Error loading java.security file`となった。チェック結果が欠けるため、不正に「通信遮断成功」と判定せず全体を失敗にした。

Ubuntu版OpenJDKでは、例えば次の設定ファイルがJDK外を参照する。

```text
/usr/lib/jvm/java-21-openjdk-arm64/conf/security/java.security
  -> /etc/java-21-openjdk/security/java.security
/usr/lib/jvm/java-21-openjdk-arm64/conf/net.properties
  -> /etc/java-21-openjdk/net.properties
```

Linuxバックエンドで2ファイルのcanonical pathを解決し、外部の実体だけを読み取り専用bind mountするよう修正した。JDK内部に実体がある場合は追加公開しない。外部ディレクトリ全体を公開しないことと、参照先が欠ける場合に失敗することを単体テストでも確認した。

修正後、同じUbuntu JDKでJavaと子JVMの全26項目も合格した。

## 再実行

マシンは次のコマンドで作成した。既に存在する場合は作成を繰り返す必要はない。

```sh
orbctl create --isolated --memory 4G --cpus 4 --disk 16G \
  ubuntu:24.04 monalauncher-sandbox
```

初回にインストールしたパッケージとツールチェーン:

```sh
orb -m monalauncher-sandbox -u root -w / apt-get update
orb -m monalauncher-sandbox -u root -w / apt-get install -y --no-install-recommends \
  build-essential ca-certificates curl bubblewrap openjdk-21-jdk-headless rustup
orb -m monalauncher-sandbox -w / rustup toolchain install 1.97.1 \
  --profile minimal --component rustfmt,clippy
orb -m monalauncher-sandbox -w / rustup default 1.97.1
```

以下はこのマシンのユーザー名・配置先を使ったコマンド。ソース転送はmacOSのリポジトリルートで実行する。`git archive HEAD`はコミット済みの内容のみを転送する。

```sh
orb -m monalauncher-sandbox -w / mkdir -p /home/aobahibino/monalauncher-sandbox
git archive HEAD tools/sandbox-lab | \
  orb -m monalauncher-sandbox -w /home/aobahibino/monalauncher-sandbox tar -xf -

orb -m monalauncher-sandbox -w /home/aobahibino/monalauncher-sandbox \
  cargo run --manifest-path tools/sandbox-lab/Cargo.toml --locked -- \
  --java-home /usr/lib/jvm/java-21-openjdk-arm64 \
  --report tools/sandbox-lab/reports/linux-headless.json

mkdir -p tools/sandbox-lab/reports
orb -m monalauncher-sandbox -w /home/aobahibino/monalauncher-sandbox \
  cat tools/sandbox-lab/reports/linux-headless.json \
  > tools/sandbox-lab/reports/linux-headless.json
```

macOS側に保存した初回失敗レポートは`reports/linux-headless-initial.json`、修正後の成功レポートは`reports/linux-headless.json`。どちらもGit管理対象外。

## この結果が示さないこと

今回の実装は`bubblewrap-namespaces-only`。**seccomp、Linuxデスクトップ、Minecraft本体は未検証・未実装の範囲を残している。**

このゲストには`/dev/dri`がなく、`DISPLAY`・`WAYLAND_DISPLAY`も未設定だった。LWJGL、GPU描画、Wayland/X11、音声、入力は今回実行していない。

OrbStackのカーネルとnamespace利用条件による結果であり、標準UbuntuのAppArmor制約、他ディストリビューション、Linux x86_64、実GPUを持つデスクトップを保証しない。「ホスト」はこの試験では同じLinuxゲスト内のサンドボックス外側のnamespaceを意味し、macOSホストへの直接アクセスを試験したものではない。

外部インターネット、IPv6・DNS、既存FDからの脱出、リソース制限、異常終了時の子孫回収なども[共通の未検証項目](../README.md)として残る。
