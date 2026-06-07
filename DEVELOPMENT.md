# 開発記録 - Electron 版から Tauri 移植まで

このドキュメントは Taskbar Aquarium プロジェクトの開発過程と、技術的な意思決定を記録したものです。

---

## 全体の流れ

```
Electron で初期実装
   ↓
タスクバー押し負け問題に直面、複数案を試行
   ↓
編集モードを削除して常時操作可能な UI に簡素化
   ↓
Electron 版を MSIX 化（99MB）
   ↓
サイズ削減のため Tauri に移植（3MB）
   ↓
Tauri 版を Microsoft Store に提出
```

---

## Electron 版で遭遇した問題

### 問題1: タスクバー押し負け

タスクバー上に常時最前面のウィンドウを置くと、タスクバーのアイコンをクリックした際にウィンドウが背面に押し込まれて戻ってこないことがありました。

#### 試したアプローチ

| 手法 | 結果 |
|------|------|
| `setAlwaysOnTop(true, 'screen-saver')` の即時適用 | 押し負けが起きる |
| `win.on('blur', ...)` で `setAlwaysOnTop` を再適用 | blur イベントが発火しない（フォーカスを持たないため） |
| `app.on('browser-window-blur', ...)` | 同上、発火しない |
| 500ms 間隔の `setAlwaysOnTop` | 描画リセットでアニメ停止 |
| 100ms 間隔の `moveTop()` | 動作するが時々アニメが止まる |
| 1000ms 間隔の `moveTop()` | 改善するが押し負けたままになる場合あり |
| `focusable: false` | 逆効果、クリックで完全に消える |
| **クリックスルーを無効化** | ✅ 解決 |

#### 学び

`setIgnoreMouseEvents(true)` でクリックスルーにすると、ウィンドウ領域内のクリックがタスクバーに届き、タスクバーが activate して水槽を背面に押し込みます。クリックスルーを無効化すれば、水槽領域内のクリックは水槽が吸収するためタスクバーが activate されません。トレードオフとして水槽領域にあるタスクバーボタンはクリックできなくなります。

### 問題2: 背景隠れ中のアニメーション停止

Electron はウィンドウが隠れると `requestAnimationFrame` を絞ります。これにより押し負けで一瞬隠れた後、再表示されたときに魚が止まって見えました。

**解決**: `webPreferences.backgroundThrottling: false`

### 問題3: 編集モードの複雑さ

初期実装では「閲覧モード（クリックスルー）」と「編集モード（ドラッグ可能）」を切り替える設計でしたが：

- ユーザー操作が増える
- モード切替UIの実装が複雑
- クリックスルーを止めたため不要に

**結論**: 編集モードを完全削除。常時ドラッグ・リサイズ可能、右クリックで設定メニュー。

### 問題4: `-webkit-app-region: drag` と contextmenu の競合

ウィンドウをドラッグ可能にする標準的な CSS `-webkit-app-region: drag` を使うと、その領域で右クリック（contextmenu イベント）が発火しなくなります。

**解決**: カスタムドラッグ実装。`mousedown` → `mousemove` で差分計算 → IPC で main プロセスの `win.setPosition()` を呼ぶ。

---

## Tauri 移植の経緯

### 動機: サイズ削減

| 項目 | Electron 版 | Tauri 版 |
|------|------------|---------|
| exe 本体 | 173MB | 9MB |
| MSIX パッケージ | 99MB | 3MB |
| ランタイム | Chromium 同梱 | OS の WebView2 |

Electron アプリは Chromium ランタイムをバンドルするため最低 100MB 以上になります。Tauri は OS の WebView2 を使うため大幅に軽量化されます。

### セットアップで詰まったポイント

#### MSVC リンカが見つからない

初回ビルドで `link.exe not found` エラー。Rust の MSVC ターゲットには Visual Studio Build Tools の C++ ワークロードが必要：

```powershell
winget install Microsoft.VisualStudio.2022.BuildTools --override "--passive --wait --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended"
```

VS Code は別物。インストール後はターミナル再起動で `where.exe link` を確認。

---

## Tauri での技術的決断

### 1. ウィンドウ設定は `tauri.conf.json`

Electron で BrowserWindow のコンストラクタに渡していた設定が、Tauri では JSON 設定ファイルになります：

```json
{
  "decorations": false,
  "transparent": true,
  "alwaysOnTop": true,
  "skipTaskbar": true,
  "resizable": true,
  "shadow": false
}
```

### 2. タスクバー高さの取得は Windows API

Tauri の `Monitor` 構造体には `work_area()` がないため、Windows API `SystemParametersInfoW(SPI_GETWORKAREA)` で work area を取得し、画面高さとの差からタスクバー高を算出。

```rust
let mut rect = RECT::default();
SystemParametersInfoW(SPI_GETWORKAREA, 0, Some(&mut rect as *mut _ as *mut _), ...);
let taskbar_h = monitor_height - (rect.bottom - rect.top) as u32;
```

### 3. `startDragging()` は使えない

Tauri 2 標準の `WebviewWindow::start_dragging()` は `alwaysOnTop` ウィンドウだと反応しません（既知の挙動）。Electron 時代と同じく、JS で mousedown/move/up を捕捉し、Rust 側に move コマンドを送って `set_position` する方式に切り替え。

```rust
#[tauri::command]
fn move_window(window: tauri::WebviewWindow, dx: i32, dy: i32) {
    if let Ok(pos) = window.outer_position() {
        let _ = window.set_position(PhysicalPosition {
            x: pos.x + dx,
            y: pos.y + dy,
        });
    }
}
```

リサイズは `startResizeDragging('West' | 'East')` が動作するのでこちらは使用。

### 4. Z オーダー復帰は Windows API 直叩き

Tauri の `set_always_on_top(true)` を繰り返し呼んでも、既に true なら無効化されて Z オーダーが再適用されません。Windows API を直接呼び出し：

```rust
SetWindowPos(
    hwnd,
    Some(HWND_TOPMOST),
    0, 0, 0, 0,
    SWP_NOACTIVATE | SWP_NOMOVE | SWP_NOSIZE,
);
```

- `SWP_NOACTIVATE`: フォーカスを奪わない（重要）
- `SWP_NOMOVE | SWP_NOSIZE`: 位置・サイズは変更しない

これを 1 秒間隔で std::thread から呼び出して押し負け復帰を実現。

#### HWND を別スレッドへ渡す

`HWND` 型は `*mut c_void` で `Send` を実装していないので、生ポインタを `isize` にキャストして渡し、新スレッドで `HWND(ptr as *mut _)` で復元。

### 5. windows クレートのバージョン整合

Tauri 2.11.2 は内部で `windows-core 0.61` を使っているため、こちらも `windows = "0.61"` に揃える必要があります。`0.58` を指定すると同一 `HWND` 型でも別物として扱われコンパイルエラー。

```toml
[target.'cfg(windows)'.dependencies]
windows = { version = "0.61", features = ["Win32_Foundation", "Win32_UI_WindowsAndMessaging"] }
```

`SetWindowPos` のシグネチャもバージョン間で異なり、0.61 では `hwndinsertafter` が `Option<HWND>` 型に変更されています。

### 6. トレイアイコン

PNG 埋め込みで `image-png` フィーチャが必要：

```toml
tauri = { version = "2", features = ["tray-icon", "image-png"] }
```

```rust
let tray_icon = Image::from_bytes(include_bytes!("../icons/build/icon.png"))?;
TrayIconBuilder::new()
    .icon(tray_icon)
    .menu(&tray_menu)
    .tooltip("Taskbar Aquarium")
    .build(app)?;
```

`from_bytes` は `image-png` フィーチャがないと存在しません（このフィーチャ未指定だと `Image::new` で raw RGBA を渡す必要あり）。

### 7. 右クリックメニューの押し負け

`window.popup_menu(&menu)` で表示したコンテキストメニューが、1秒ごとの押し負け復帰によって背面に押し込まれる問題が発生。

**解決**: グローバル `AtomicU64` で「Z オーダー復帰を一時停止する時刻」を管理。メニュー表示時に 3 秒間停止、メニュー項目クリック時（`on_menu_event`）で即時再開。

```rust
static TOPMOST_PAUSE_UNTIL: AtomicU64 = AtomicU64::new(0);

// メニュー表示時
pause_topmost_for(3);

// メニュー項目クリック時
fn handle_menu_event(app: &AppHandle, id: &str) {
    resume_topmost();
    // ...
}

// 復帰ループ内
if topmost_paused() { continue; }
```

### 8. 設定の永続化

Tauri の `app.path().app_data_dir()` で `%APPDATA%\com.sobu-lab.taskbar-aquarium-tauri\` を取得し、`config.json` として読み書き。`Mutex<Settings>` で状態管理、`#[serde(default)]` で後方互換。

### 9. ウィンドウ移動・リサイズの保存

```rust
window.on_window_event(move |event| {
    match event {
        WindowEvent::Moved(pos) => {
            update_settings(&h, |s| {
                s.x = Some(pos.x);
                s.y = Some(pos.y);
            });
        }
        WindowEvent::Resized(size) => {
            update_settings(&h, |s| {
                s.width = Some(size.width);
            });
        }
        _ => {}
    }
});
```

毎ピクセル発火するため頻繁に保存されますが、JSON が小さいので問題なし。

---

## MSIX 化と Microsoft Store 提出

### ツール選定

Electron 版は `electron-builder` の `appx` ターゲットで MSIX 化しました。Tauri 版は Microsoft 公式の **winapp CLI** (`@microsoft/winappcli`) を採用。理由：

- Electron 版検討時に存在を確認していた
- Tauri exe + manifest + アセットだけで `winapp pack` 可能
- 公式ツールなのでサポートが安定

```bash
npm install --save-dev @microsoft/winappcli
npx winapp pack ./msix --output ./msix --manifest ./msix/Package.appxmanifest
```

### Package.appxmanifest の Identity

Partner Center で予約したアプリ名から取得した値を埋め込む：

```xml
<Identity
  Name="sobu-lab.TaskbarAquarium"
  Publisher="CN=98B3F3D9-FA37-453E-B42F-BEDE74BABF4B"
  Version="2.0.0.0"
  ProcessorArchitecture="x64" />
```

Version は Electron 版の `1.0.0.0` から `2.0.0.0` に。Tauri リライトを区切る意味も込めて。

### runFullTrust の説明

MSIX 化したデスクトップアプリは `runFullTrust` 機能が必須。Partner Center で承認理由を求められるため、以下を提出：

> Win32 ウィンドウ API（SetWindowPos with HWND_TOPMOST、SetLayeredWindowAttributes 等）でタスクバー上の常時最前面・フレームレス透過ウィンドウを実現するため。設定の JSON ファイルを %APPDATA% に保存するため。システムトレイアイコンと右クリックメニューのため。ネットワーク通信や個人情報収集は一切なし。

### プライバシーポリシー

最近の Microsoft Store はデスクトップアプリ全般でプライバシーポリシー URL が必須化されています（WebView2 が Microsoft Edge の一部としてテレメトリを持つため）。アプリ自体が無収集でも URL の提示が必要。

[PRIVACY.md](./PRIVACY.md) を作成し、GitHub の URL を Partner Center に登録：
- データ収集なし
- ローカル設定のみ
- ネットワーク通信なし
- WebView2 については Microsoft のポリシーに準ずる旨を明記

### ローカル動作確認の制約

Windows 10 では `Add-AppxPackage -AllowUnsigned` が使えないため、未署名 MSIX のローカルインストールが不可能。Electron 版・Tauri 版ともに：

- exe 単体での動作は事前確認
- MSIX のローカルテストはスキップ
- Partner Center 側で検証＆署名されるため問題なし

---

## アイコン処理

### Windows のアイコンキャッシュ

exe にアイコンを埋め込んでも、Windows Explorer が古いアイコンキャッシュを表示することがあります。

確認方法：
```powershell
Copy-Item taskbar-aquarium-tauri.exe test-icon.exe
```
新しいファイル名でコピーすると、キャッシュが効かず本物のアイコンが見えます。

キャッシュクリア：
```powershell
ie4uinit.exe -show
```

### Tauri のアイコン生成

`npx tauri icon path/to/source.png` で全プラットフォームの必要サイズを一括生成。iOS/Android のアイコンまで生成されますが Windows のみ使用するので残りは無視。

---

## Microsoft Store 提出時の細部

### スクリーンショット

最低 1366×768 が必要。1920×1080 が無難。Windows 標準の Snipping Tool で範囲指定キャプチャ。

良いスクショの構図：
- VS Code やブラウザを開いた状態 + タスクバー上の水槽
- 「日常使いに溶け込む」コンテキストが伝わる

### カテゴリ選択

- 主カテゴリ: **エンターテイメント**（個人用設定にアクアリウム向けサブカテゴリがないため）
- セカンダリカテゴリ: **ライフスタイル**（癒し系の文脈）

### 年齢区分

IARC 質問票で全部「いいえ」回答 → 3+（Everyone）判定。

「ゲームではない」「ソーシャルでもない」→「その他のすべてのアプリの種類」を選択。

### 製品の宣言

- ゲームクリップ録画/ブロードキャストはゲーム専用なので外す
- 代替ドライブへのインストール許可は残す
- OneDrive バックアップ許可は残す
- アクセシビリティテスト済みのチェックは外す（実際にテストしていない）

### 価格

ドロップダウンから「**無料**」を明示的に選択。未選択だと「価格表に対して、有効な価格を設定してください」エラー。

---

## まとめ

### 技術スタックの比較

| | Electron 版 | Tauri 版 |
|---|------------|---------|
| メイン言語 | JavaScript | Rust + JavaScript |
| ランタイム | Chromium 同梱 | WebView2 (OS 標準) |
| ウィンドウ管理 | BrowserWindow + Win32 概念 | Tauri Window + 生 Win32 API |
| MSIX ビルド | electron-builder | winapp CLI |
| 配布サイズ | 99MB | **3MB** |
| 開発体験 | npm install して即開発 | Rust + MSVC セットアップ必要 |

### 何を得たか

- **小さいバイナリ**: Electron の Chromium ランタイム依存を脱却
- **OS 機能の直接利用**: Windows API を Rust から呼び出せる、性能と制御の両立
- **学習リソース**: Tauri 2 のメニュー・トレイ・IPC・ウィンドウ管理を一通り学べた

### 残った課題・今後の改善案

- 複数モニター対応の整理（現状は primary のみ前提）
- タスクバーが上/左/右配置のケース対応
- アクセシビリティ（スクリーンリーダー等）
- 国際化（メニュー文字列の英語化）
- Linux/macOS 対応（タスクバー概念が異なるので別UIが必要）

---

## 関連ドキュメント

- [README.md](./README.md) - 使い方とビルド方法
- [MICROSOFT_STORE.md](./MICROSOFT_STORE.md) - Store 提出の詳細手順
- [PRIVACY.md](./PRIVACY.md) - プライバシーポリシー
- [Electron 版リポジトリ](https://github.com/sobu-lab/taskbar-aquarium) - 移植元
