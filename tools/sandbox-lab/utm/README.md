# UTM Linuxデスクトップの作成

ARM64のUbuntu 24.04をQEMU/HVFで動かす、検証専用VM。6GB RAM、4 vCPU、40GBの仮想ディスク、GNOME/Xorg、virtio-ramfb（GLなし）を使用する。ホストのディレクトリは共有しない。ネットワークはパッケージ導入用のemulated NATで、公開port forwardは設定しない。

`mona`はこのVMだけの検証ユーザーで、GUIは自動ログイン、sudoはパスワード不要。SSHパスワード認証とrootログインは無効。ホストのアカウントや認証情報はコピーしない。本番環境向けのアカウント設定ではない。

## 作成手順

1. [Ubuntu公式cloud image](https://cloud-images.ubuntu.com/noble/current/)の`noble-server-cloudimg-arm64.img`と`SHA256SUMS`を取得し、対象イメージのSHA-256を照合する。
2. **インポート前に**`qemu-img resize noble-server-cloudimg-arm64.img 40G`を実行する。UTM 4.7.4では、既存イメージの`source`と同時に`guest size`を指定しても拡張されない。既存driveの更新でも同様。実行に使うディスクが停止中であることを確認する。
3. 新しい`seed/`に、このディレクトリの`cloud-init.yaml`を`user-data`、`network-config.yaml`を`network-config`としてコピーする。`meta-data`には一意な`instance-id`と`local-hostname: monalauncher-desktop`を記述する。
4. macOSで`hdiutil makehybrid -o seed.iso seed -iso -joliet -default-volume-name cidata`を実行する。
5. UTMアプリを通常起動し、`osascript tools/sandbox-lab/utm/create.applescript /absolute/path/to/noble-server-cloudimg-arm64.img /absolute/path/to/seed.iso`を実行する。戻り値がVM UUID。**同じ名前のVMがある場合は再作成しない。**
6. `utmctl start UUID`で起動する。cloud-initがデスクトップを導入して再起動する。`utmctl exec UUID --cmd /usr/bin/test -f /var/lib/monalauncher-desktop-ready`だけで完了判定せず、cloud-initの終了状態とGNOMEセッションも確認する。

seedは内蔵VirtIOディスクとして取り込み、NICのMACアドレスをnetwork-configと一致させる。複数VMを同じネットワークで動かす場合は、両ファイルのMACを一意なものへ変更する。初回に試した外付けUSB seed・自動ネットワーク設定ではnetwork-online待ちになったため、再現用手順は成功した構成に揃えている。

UTM 4.7.4の`utmctl attach UUID`はシリアル接続を実装しておらず、PTYのパスを表示する。このPTYを端末で読み続けると、起動中のcloud-initログを確認できる。ゲストエージェントの導入後は`utmctl exec`と`utmctl file push/pull`が使える。CLIのエラーが終了コード0になる場合があるため、ゲストの結果ファイルを読み戻して判定する。

このバージョンの`utmctl exec`では、時間のかかるコマンドが結果なしで早期終了する挙動も確認した。検証時は`osascript tools/sandbox-lab/utm/exec.applescript UUID /program args...`で、APIの`exited`を待って出力と終了状態を取得できる。待機は最大10分。タイムアウト時にゲストの処理は継続する場合があるため、必要ならゲストの`/usr/bin/timeout`も指定する。1秒待つ成功コマンドと、1秒待って非ゼロ終了するコマンドで、この補助スクリプトの結果取得を確認した。

このホストではUTMをCLI経由でバックグラウンド起動した際にAppKitの`canBecomeMainWindow`エラーも観測した。VM停止中にUTMを終了し、通常のアプリとして起動し直すと解消した。

## 検証コード

`tools/sandbox-lab`のソースと、検証済みLinux ARM64用JARをゲストへコピーする。ビルド・実行は`mona`ユーザーで行い、rootでは実行しない。

```sh
rustup toolchain install 1.97.1 --profile minimal --component rustfmt --component clippy
rustup default 1.97.1
cargo test --manifest-path tools/sandbox-lab/Cargo.toml --locked
cargo clippy --manifest-path tools/sandbox-lab/Cargo.toml --all-targets --locked -- -D warnings
```

GNOMEの端末から、`DISPLAY`と`XAUTHORITY`が設定された状態で実行する。`utmctl exec`はrootとして動くため、利用する場合は`runuser`でユーザーを切り替え、対象デスクトップの環境変数を明示する。

```sh
cargo run --manifest-path tools/sandbox-lab/Cargo.toml --locked -- \
  --java-home /usr/lib/jvm/java-21-openjdk-arm64 \
  --lwjgl tools/sandbox-lab/.cache/lwjgl-linux-arm64 \
  --report tools/sandbox-lab/reports/linux-utm-lwjgl.json
```

GUIプロファイルは選択したX11ソケット、XAUTHORITYファイル、存在するDRM render nodeだけを追加公開する。D-Bus、音声ソケット、`/dev/input`、ホーム全体、`/run/user`全体は公開しない。X11自体のクライアント間の隔離は保証しない。Wayland、音声、キーボード・マウスの入力、Minecraftの動作はこの試験の対象外。

## UbuntuのAppArmorによるuser namespace制限

今回のUbuntuでは`kernel.apparmor_restrict_unprivileged_userns=1`で、通常ユーザーからのbwrap起動が`Failed RTM_NEWADDR: Operation not permitted`になった。カーネルログでは`unprivileged_userns`プロファイルが`net_admin`を拒否していた。これは隔離チェックのPASSではなく、起動失敗として保存する。

専用の検証VMでは、Ubuntu同梱の`/etc/apparmor.d/flatpak`と同じ方式で、root所有の検証用runnerにだけ`userns`を許可する。**このAppArmorファイル自体はファイル・ネットワークの隔離ポリシーではない。** 実際の隔離はbubblewrapで行う。システム全体のsysctlやAppArmorは無効化しない。

```sh
# ビルド後、専用ゲスト内で管理者として実行する。
sudo install -d -m 755 /usr/local/libexec
sudo install -o root -g root -m 755 \
  tools/sandbox-lab/target/debug/monalauncher-sandbox-lab \
  /usr/local/libexec/monalauncher-sandbox-lab
sudo install -o root -g root -m 644 \
  tools/sandbox-lab/utm/monalauncher-sandbox-lab.apparmor \
  /etc/apparmor.d/monalauncher-sandbox-lab
sudo apparmor_parser -r /etc/apparmor.d/monalauncher-sandbox-lab
```

この後は、一般ユーザーのデスクトップから`cargo run ... --`の代わりに`/usr/local/libexec/monalauncher-sandbox-lab`を起動する。ソース変更後にはroot所有のrunnerも再インストールする。プロファイル導入前の失敗と、導入後の結果を別名で保存する。本番配布時のパッケージ所有権、AppArmor登録、更新方法は別途設計する。

参考: [Ubuntuの非特権user namespace制限](https://discourse.ubuntu.com/t/understanding-apparmor-user-namespace-restriction/58007)。

参考: [UTM scripting](https://docs.getutm.app/scripting/cheat-sheet/)、[UTM Linux guest support](https://docs.getutm.app/guest-support/linux/)、[UTM 4.7.4のdriveインポート実装](https://github.com/utmapp/UTM/blob/v4.7.4/Scripting/UTMScriptingConfigImpl.swift)。
