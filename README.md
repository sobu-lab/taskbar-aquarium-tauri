# タスクバー水槽（Tauri版）

Windows のタスクバーに重ねて、ドット絵の熱帯魚・金魚を泳がせる常時最前面アプリです。
[Electron 版](https://github.com/sobu-lab/taskbar-aquarium) の Tauri 移植版で、配布サイズが大幅に小さくなっています（約 100MB → 10MB級）。

## 必要なもの
- Windows 10/11
- 開発時：Node.js, Rust toolchain, Visual Studio Build Tools (C++)
- 実行時：WebView2（Windows 11 標準、Windows 10 にも一般的に入っている）

## 起動（開発モード）
```bash
npm install
npm run tauri dev
```
起動するとタスクバーの上に水槽が重なります（初期は画面下中央あたり）。

## ビルド
```bash
npm run tauri build
```
`src-tauri/target/release/bundle/` に msi / exe が生成されます。

## 使い方

| 操作 | 内容 |
|------|------|
| 水槽の中央をドラッグ | 位置の移動 |
| 水槽の**左右の端をドラッグ** | 横幅の変更（高さはタスクバーに固定） |
| 水槽を右クリック | 設定メニュー（魚の数・ピクセルサイズ・背景透過） |
| トレイアイコンを右クリック | 設定メニュー・終了 |

- 位置・幅・魚の数・ピクセルサイズ・背景透過は自動保存され、次回起動時に復元されます。
- 設定の保存先：`%APPDATA%\com.sobu-lab.taskbar-aquarium-tauri\config.json`
- **背景透過**を ON にすると、青い水槽の背景が消えて魚だけが浮かびます。タスクバーに馴染ませたいときはこちら。

## アーキテクチャ
- **フロントエンド** (`src/index.html`)：HTML + Canvas でドット絵描画とアニメーション。Electron 版とほぼ共通。
- **バックエンド** (`src-tauri/src/lib.rs`)：Rust + Tauri 2。ウィンドウ管理、タスクバー位置計算、トレイ、メニュー、設定永続化、Z オーダー復帰。
- **押し負け復帰**：Windows API `SetWindowPos(HWND_TOPMOST)` を1秒ごとに呼び出して、タスクバーに隠れた際に前面に戻します。

## 既知の制限
- **下に配置されたタスクバー**を前提にしています。
- タスクバーを「自動的に隠す」設定にしていると、隠れる動きには追従しません。

## カスタマイズ
- 魚のドット絵は `src/index.html` の `SPECIES` 配列に、文字マップで定義しています。`'1'`=本体、`'2'`=影、`p`=目 などをパレットで色付けしています。
- 配色・背景の青は `drawBackground()` の `bands` で変えられます。
