# Scyrox V6 Control (Windows / Rust)

Scyrox V6 ワイヤレスゲーミングマウス向けの非公式 Windows ネイティブ制御アプリ（純 Rust 製・`eframe`/`egui` + `hidapi`）です。  
公式 Web ドライバー（S-Center）や Electron ランタイムを使わず、軽量な単一 `.exe` として動作します。

## 主な機能

- **自動デバイス検出（Windows ネイティブ HID）**:
  - VID `0x3554` / PID `0xF5F7`（8K ドングル）・`0xF5F4`（標準ドングル）・`0xF5F6`（USB-C 有線）、Usage Page `0xFF02` / Usage `0x0002` を自動検出します（Zadig 等のドライバ置換不要）。
- **バッテリー監視**:
  - Compx/Nordic nRF52840 プロトコルの `0x04` コマンドにより、残量（%）・充電/有線状態・セル電圧（mV）を取得します。
- **DPI 設定**:
  - PixArt PAW3950 向け 50〜30,000 DPI（50 刻み）、最大 8 ステージ、アクティブステージ切替、ステージ LED カラー設定。
- **パフォーマンス・センサー設定**:
  - ポーリングレート（125 / 250 / 500 / 1000 / 2000 / 4000 / 8000 Hz、接続方式の上限に自動制限）、LOD（0.7 / 1.0 / 2.0 mm）、デバウンス（0〜30 ms）、センサーモード（LP / HP / 20K FPS）、MotionSync / Angle Snapping / Ripple Control / Long Range Mode、スリープ時間。
- **プロファイル管理**:
  - オンボード 4 プロファイル切替、JSON 形式での設定保存・読込。
- **安全設計（読み取り専用デフォルト & HID 診断）**:
  - 起動時は **書き込みロック有効**（読み取り専用）です。
  - **HID 診断・フラッシュダンプ**タブで `0x1D`（ドングル照会）・`0x01`（ハンドシェイク）・`0x08`（フラッシュ読み出し Hex ダンプ）・カスタム 17 バイトパケット送信（`0x55` チェックサム自動計算）を実行できます。
  - フラッシュアドレスは `config/scyrox_v6_map.json` で外部変更できるため、ファームウェア差異があっても再コンパイルなしで調整可能です。
  - マウス未接続でも UI と動作を確認できる **デモモード** を搭載しています。

## ビルド方法（Windows 10 / 11）

### 前提環境

1. [Rust (rustup)](https://rustup.rs/)（`x86_64-pc-windows-msvc` ツールチェーン）
2. Visual Studio Build Tools 2022（「C++ によるデスクトップ開発」ワークロード）

### テストとビルド

```powershell
# 単体テスト（17バイトHIDフレーム・0x55チェックサム・バッテリーパース検証）
cargo test

# デバッグ実行（コンソール出力あり）
cargo run

# リリースビルド（最適化済み単一EXEを生成）
cargo build --release
```

生成された実行ファイルは `target\release\scyrox-v6-control.exe` に出力されます。

## 安全上の注意（重要）

- 本アプリは非公式ツールです。公式ソフト（S-Center）と同時に起動しないでください。
- 初回利用時は **書き込みロックを解除する前** に、「HID診断・フラッシュダンプ」タブで `0x1D` / `0x01` / `0x04` および `0x08` ダンプが正常に応答することを確認してください。
- ファームウェアリビジョンにより EEPROM/フラッシュオフセットが異なる場合は、S-Center で 1 項目変更した前後の `0x08` ダンプを比較し、`config/scyrox_v6_map.json` のアドレスを合わせてください。
- 次世代モデル（Scyrox V6S / V8S、VID `0x395A` の 64 バイトプロトコル）は別プロトコルのため対象外です。

## プロトコル参照元

- [sktdr-chan/sktdr-mouse](https://github.com/sktdr-chan/sktdr-mouse) — Compx 17 バイト HID プロトコル（VID `0x3554`, MID=2, コマンド `0x01`/`0x04`/`0x07`/`0x08`/`0x0A`/`0x1D`）
- [Fan4Metal/mouse_tray (`nordic52.py`)](https://github.com/Fan4Metal/mouse_tray/blob/master/mouse_tray/drivers/chipset/nordic52.py) — Usage Page `0xFF02` / Usage `0x0002`、17 バイト `0x55` チェックサム、バッテリー応答フォーマット
- [Olek97/Redragon_M913_Web_Configurator](https://github.com/Olek97/Redragon_M913_Web_Configurator) — `0x07` / `0x08` EEPROM 読み書きフレーム構造
- [TechPowerUp — SCYROX V6 Review](https://www.techpowerup.com/review/scyrox-v6/5.html) / [OZO Keyboards — Scyrox V6](https://www.ozokeyboards.com/ja/products/scyrox-v6-8k-wireless-gaming-mouse) — PAW3950 センサー・ポーリングレート・LOD 仕様

## ライセンス

MIT License
