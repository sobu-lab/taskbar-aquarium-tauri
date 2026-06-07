# Microsoft Store に無料公開する手順（Tauri 版）

この Tauri 版を MSIX 化して Microsoft Store に無料公開するための手順です。
費用はかかりません（個人開発者の登録料は無料）。

全体は「①アカウント準備 → ②Identity 取得 → ③MSIX化 → ④提出」の4段階です。
①②④は Web 上の作業、③がこのプロジェクトでの作業です。

---

## 前提環境
- Windows 10/11
- Node.js（`node -v` で確認）
- Rust toolchain（`cargo --version` で確認）
- Visual Studio Build Tools 2022（C++ ワークロード）

## 採用ツール
- **Tauri 2** : フロントエンド + Rust バックエンドのビルド
- **@microsoft/winappcli** : Tauri の出力 exe を MSIX に包む Microsoft 公式 CLI

---

## ステップ1：開発者アカウントを作る（無料・Webのみ）✅完了済み

1. ブラウザで **https://storedeveloper.microsoft.com** を開く
   （※この入口だけが新しい無料フロー。Partner Center から直接入ると有料の旧フローになるので注意）
2. 「Get started for free」→ **Individual developer (free)** を選択
3. Microsoft アカウントでサインイン
4. 政府発行の身分証＋セルフィーで本人確認（クレジットカード不要）

---

## ステップ2：アプリ名予約と Identity 取得 ✅完了済み

MSIX には、ストアが発行する固有の ID（Identity）を埋め込む必要があります。

1. Partner Center の「アプリとゲーム」→ **新しい製品 → アプリ** を作成
2. アプリ名を予約（このプロジェクト: `Taskbar Aquarium`）
3. 予約したアプリの **製品管理 → Microsoft Store への提出に必要な値** を開く
4. 3つの値を `msix/Package.appxmanifest` に反映する：
   - **Package/Identity/Name** = `sobu-lab.TaskbarAquarium`
   - **Package/Identity/Publisher** = `CN=98B3F3D9-FA37-453E-B42F-BEDE74BABF4B`
   - **Package/Properties/PublisherDisplayName** = `sobu-lab`

---

## ステップ3：MSIX を作る（このプロジェクトでの作業）

### 3-1. 開発依存パッケージ導入（初回のみ）
```bash
npm install --save-dev @microsoft/winappcli
```

### 3-2. リリースビルド
```bash
npm run tauri build
```
`src-tauri/target/release/taskbar-aquarium-tauri.exe`（約 10MB）が生成されます。

### 3-3. アイコンを更新したい場合
高解像度の元画像を用意して：
```bash
npx tauri icon path/to/new-icon.png
```
`src-tauri/icons/` 配下の全サイズが再生成されます。その後 `msix/Assets/` にも反映が必要なら下記コマンドで再コピー：
```bash
cp src-tauri/icons/StoreLogo.png msix/Assets/
cp src-tauri/icons/Square150x150Logo.png msix/Assets/
cp src-tauri/icons/Square44x44Logo.png msix/Assets/
```

### 3-4. exe をステージングに配置
```bash
cp src-tauri/target/release/taskbar-aquarium-tauri.exe msix/
```

### 3-5. バージョン更新
リリース毎に `msix/Package.appxmanifest` の `Identity Version` を上げる：
```xml
<Identity ... Version="2.0.0.0" ... />
```
- ストア提出は **必ず前回より大きい4桁バージョン** が必要
- 例：`2.0.0.0` → `2.0.1.0` → `2.1.0.0`

### 3-6. MSIX をパッケージ
```bash
npx winapp pack ./msix --output ./msix --manifest ./msix/Package.appxmanifest
```
`msix/sobu-lab.TaskbarAquarium_<version>_x64.msix` が出力されます（約 3MB）。

> 初回実行時は Microsoft.Windows.SDK.BuildTools が自動ダウンロードされます（数百MB、1回だけ）。

---

## ステップ4：手元で MSIX を動作確認（任意・スキップ可）

開発者モードを ON にしてから（`ms-settings:developers`）：

```powershell
# 自己署名証明書を生成
npx winapp cert generate

# 証明書をインストール（管理者 PowerShell）
npx winapp cert install .\devcert.pfx

# 署名付きで再パッケージ
npx winapp pack ./msix --output ./msix --manifest ./msix/Package.appxmanifest --cert ./devcert.pfx

# インストール
Add-AppxPackage .\msix\sobu-lab.TaskbarAquarium_2.0.0.0_x64.msix
```

> Windows 10 では `-AllowUnsigned` が使えないため、ローカル動作確認には必ず自己署名が必要です。Tauri exe は `npm run tauri build` 後に単体で動作確認できているため、このステップはスキップしても問題ありません。

---

## ステップ5：ストアの掲載情報を準備

提出前に Partner Center で次を用意します（無料配布でも必要）。
- **ストアロゴ**：`msix/Assets/StoreLogo.png`（または高解像度版）
- **スクリーンショット**：水槽が動いている画面のキャプチャを数枚（推奨 1920×1080）
- **説明文**：何をするアプリか
- **年齢レーティング**：質問票（IARC）に答えると自動で決まる
- **価格**：**無料** を選択
- **プライバシー**：個人データを集めない旨を記載（このアプリはネット送信なし）

### 説明文の案

```
タスクバーに重ねるドット絵の水槽です。熱帯魚や金魚がのんびり泳ぐ
レトロな8ビット風アニメーションが、作業中の画面を癒します。

特徴：
・タスクバー上に常時最前面で表示
・ドラッグで位置移動、端のドラッグで横幅調整
・右クリックメニューから魚の数・サイズ・背景透過を設定
・低負荷で動作（CPU使用率はわずか、わずか3MB のパッケージ）
・ネット通信なし、データ収集なし

カスタマイズ可能なミニ水槽として、デスクトップに彩りを加えます。
```

---

## ステップ6：提出して審査

1. Partner Center のアプリ提出画面で、生成した `msix/sobu-lab.TaskbarAquarium_*.msix` をアップロード
2. 掲載情報・レーティング・価格（無料）を確定
3. 提出 → Microsoft の審査（2〜3日程度）
4. 承認されるとストアに公開されます

---

## トラブルシューティング

### Identity の不一致エラー
提出時に「Identity does not match the reserved name」と出たら、`msix/Package.appxmanifest` の `Identity Name` / `Publisher` / `PublisherDisplayName` が Partner Center の値と1文字でもズレています。Partner Center の値をそのままコピーしてください。

### バージョンが古いエラー
「Package version must be greater than the previous submission」が出たら、`Identity Version` を上げて再パッケージしてください。

### exe の更新がパッケージに反映されない
`npm run tauri build` 後に `msix/taskbar-aquarium-tauri.exe` の置き換えを忘れているケースが多いです。ステップ 3-4 を再実行してください。

### winapp pack で SDK ダウンロードが失敗
プロキシ環境などで失敗することがあります。`%LOCALAPPDATA%\Microsoft\winapp` のキャッシュを削除して再実行を試してください。

---

## 参考資料
- [winapp CLI（公式手順）](https://learn.microsoft.com/windows/apps/dev-tools/winapp-cli/guides/electron-packaging)
- [Microsoft Store への公開](https://learn.microsoft.com/windows/apps/publish/)
- [Tauri 2 ドキュメント](https://tauri.app/)

---

## 比較：Electron 版との違い
| 項目 | Electron 版 | Tauri 版 |
|------|------------|---------|
| 配布サイズ | 99MB (appx) | **3MB (msix)** |
| 実行ファイル | 173MB | **10MB** |
| ランタイム | Chromium 同梱 | OS の WebView2 |
| ビルドツール | electron-builder | Tauri CLI + winapp CLI |

Tauri 版はパッケージサイズが約 97% 小さく、Microsoft Store のダウンロード速度・配信コストも有利です。
