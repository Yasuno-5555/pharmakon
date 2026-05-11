# Pharmakon IDE — Cursor-like GUI 実装計画

現在の egui IDE (`pharmakon gui`) を Cursor ライクな開発環境に昇華する。

---

## 現状

```
レイアウト:     Cursor風（左ファイルツリー + 中央エディタ + 右チャット）
エディタ:      egui TextEdit (code_editor) — シンタックスハイライトなし
ファイルツリー: フラットリスト、📁絵文字のみ、ネスト非対応
チャットパネル: メッセージ履歴のみ、インラインコード表示なし
差分:          なし
```

---

## Phase 1: コードエディタの強化（3h）

### 1-a: シンタックスハイライト (`syntect`)

```bash
cargo add syntect -p pharmakon-gateway
```

```rust
use syntect::parsing::SyntaxSet;
use syntect::highlighting::ThemeSet;
use syntect::html::highlighted_html_for_string;

struct HighlightedEditor {
    syntax_set: SyntaxSet,
    theme_set: ThemeSet,
    // ファイル拡張子 → SyntaxReference のキャッシュ
    syntax_cache: HashMap<String, SyntaxReference>,
}
```

- `SyntaxSet::load_defaults_newlines()` で全言語対応
- ファイル保存時に拡張子からシンタックスを自動選択
- egui の `custom_painter` でハイライト描画（文字色だけ変える）

### 1-b: 行番号表示

```rust
// 各行の左側に行番号を描画
ui.horizontal(|ui| {
    // 行番号カラム（右寄せ、グレー）
    ui.add_sized([40.0, editor_height], |ui: &mut egui::Ui| {
        for i in 1..=line_count {
            ui.label(format!("{:>4}", i));
        }
    });
    // コード本文
    ui.add_sized([code_width, editor_height], egui::TextEdit::multiline(&mut content));
});
```

### 1-c: ファイル種別アイコン + タブバー

```
📘 main.rs          📗 types.rs          📙 Cargo.toml     [+]
```
- 開いているファイルをタブで管理
- 拡張子ごとにアイコン変更（rs→🦀, toml→📋, md→📝）
- タブクリックで切り替え、✕ で閉じる

---

## Phase 2: ファイルツリーの階層化（2h）

### 現在の問題

```rust
// app.rs: フラットリスト
if let Ok(entries) = std::fs::read_dir(&ws) {
    for e in entries.flatten() {
        let prefix = if e.path().is_dir() { "📁 " } else { "📄 " };
        file_tree.push(format!("{}{}", prefix, n));
    }
}
```

### 2-a: ツリーデータ構造

```rust
struct FileNode {
    name: String,
    path: PathBuf,
    is_dir: bool,
    children: Vec<FileNode>,
    expanded: bool,  // 開閉状態
}

fn build_tree(root: &Path, depth: usize) -> Vec<FileNode> {
    // .gitignore パターンを無視
    // target/ node_modules/ をスキップ
    // 深さ10まで再帰
}
```

### 2-b: .gitignore 対応

`gitignore` クレートで `.gitignore` パターンをパースし、無視ファイルをツリーから除外。

```rust
use gitignore::File as GitIgnore;

let gi = GitIgnore::new(&workspace_root.join(".gitignore")).ok();
// ...
if let Some(ref gi) = gi && gi.is_ignored(&path) { continue; }
```

### 2-c: 開閉アニメーション

```rust
egui::collapsing_header::CollapsingState::load(...)
    .show_header(|ui| { ui.label("📁 src"); })
    .body(|ui| { /* 子ノード */ });
```

---

## Phase 3: 差分プレビュー（2h）

### 3-a: ファイル編集時の変更検出

```rust
struct FileEditorState {
    original_content: String,
    current_content: String,
    path: PathBuf,
}

impl FileEditorState {
    fn has_changes(&self) -> bool { self.original_content != self.current_content }
    fn diff(&self) -> Vec<DiffLine> { /* diffy::create_patch */ }
}
```

### 3-b: インライン差分表示

```rust
enum DiffLine {
    Unchanged { text: String, line_no: usize },
    Added { text: String, line_no: usize },    // 緑背景
    Removed { text: String, line_no: usize },  // 赤背景
}
```

エディタ内で差分行を色分け表示。`original_content` と比較して変更行を自動検出。

### 3-c: 保存確認ダイアログ

```rust
if editor.has_changes() && ui.button("Save 💾").clicked() {
    // 確認ダイアログ
    // "4 lines changed. Are you sure? [Save] [Cancel] [Diff]"
}
```

---

## Phase 4: チャットパネルの強化（1.5h）

### 現状の問題

```
- コードブロックが整形されずに生テキスト表示
- ファイルツリーと連携なし
- メッセージが多くなると重い
```

### 4-a: コードブロックの視覚的改善

```rust
// チャットメッセージ内の ```code``` を検出して枠付き表示
struct ChatMessage {
    role: String,
    segments: Vec<MessageSegment>,
}

enum MessageSegment {
    Text(String),
    CodeBlock { language: String, code: String },
}
```

### 4-b: Apply-to-editor ボタン

```rust
// コードブロックに "Apply to Editor" ボタンを表示
if ui.button("△ Apply to Editor").clicked() && self.data.selected_file.is_some() {
    // コードブロックの内容をエディタの現在位置に挿入
    self.data.file_content = code.clone();
}
```

### 4-c: メッセージの仮想スクロール

現在は全メッセージを毎フレーム描画。100件超えると重くなる。`egui::ScrollArea` で表示範囲だけ描画するよう最適化（egui は自動でやるので大きな問題ではないが確認）。

---

## Phase 5: AI インライン候補（3h）

### 5-a: インラインゴーストテキスト

Cursor のような「次に書きそうなコード」のグレー表示。

```rust
struct InlineSuggestion {
    ghost_text: String,
    position: usize,  // カーソル位置
}

// エディタのカーソル位置にゴーストテキストを描画
if let Some(suggestion) = &self.inline_suggestion {
    let painter = ui.painter();
    painter.text(ghost_pos, egui::Align::LEFT, &suggestion.ghost_text,
        egui::TextStyle::Monospace.resolve(ui.style()), egui::Color32::from_gray(100));
}
```

### 5-b: Tab で確定

```rust
if ui.input(|i| i.key_pressed(egui::Key::Tab)) && self.inline_suggestion.is_some() {
    // ゴーストテキストを実際のコードに挿入
    self.data.file_content.insert_str(cursor_pos, &suggestion.ghost_text);
    self.inline_suggestion = None;
}
```

### 発火条件

- ユーザーの編集中に 500ms のデバウンス
- 変更範囲を含む関数全体を LLM に送信
- LLM が「次に来るコード」を予測
- 結果をゴースト表示

---

## Phase 6: ターミナルパネル（4h）

### 現状の問題

```
- ツール実行ログが単なるテキスト表示
- 実際のターミナルではない（疑似すらない）
- スクロールのみ、コマンド入力不可
```

### 6-a: Shell ツールの出力を専用ターミナルバッファに表示

```rust
struct TerminalBuffer {
    lines: VecDeque<TerminalLine>,
    max_lines: usize,
}

struct TerminalLine {
    text: String,
    is_input: bool,  // $ command か stdout か
    timestamp: String,
}
```

### 6-b: 埋め込みターミナル（コマンド入力可能）

egui のテキスト入力を利用して簡易ターミナルを実装。`shell` ツールにコマンドを送信し、結果を表示する。

```rust
// ターミナルパネル内でコマンド入力
ui.text_edit_singleline(&mut self.terminal_input);
if ui.button("Run") || enter_pressed {
    let agent = self.agent.clone();
    let cmd = self.terminal_input.clone();
    tokio::spawn(async move {
        let result = agent.chat(&format!("Run shell command: {}", cmd)).await;
        // 結果を terminal_lines に追加
    });
    self.terminal_input.clear();
}
```

---

## 優先順位マップ

```
今週やるべき（効果が大きい）:
  Phase 1-a  シンタックスハイライト     (1.5h)
  Phase 2    ファイルツリー階層化       (2h)
  Phase 3    差分プレビュー             (2h)

来週やるべき:
  Phase 1-b  行番号 + タブ             (1.5h)
  Phase 4    チャット強化 + Apply      (1.5h)
  Phase 5    AI インライン候補          (3h)  ← 工数に対して効果未知

余裕があれば:
  Phase 6    ターミナルパネル           (4h)
```

---

## ファイル別影響範囲

| ファイル | 内容 | 工数 |
|---|---|---|
| `gateway/src/ui/app.rs` | エディタ状態管理、DiffLine, FileNode, InlineSuggestion 追加 | — |
| `gateway/src/ui/mod.rs` | 描画ロジック全面改修 | — |
| `Cargo.toml` (root) | `syntect`, `gitignore` 依存追加 | — |
| `gateway/Cargo.toml` | 同上 | — |
