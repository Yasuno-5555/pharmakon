# Pharmakon 実装計画書 — Phase 7 → 8

**作成日**: 2026-05-09
**Phase 7 complete**: Cognitive Compiler (Plan Compiler, Skill Compression, Sim abolition, Structured Output, Plan Cache, Self-Healing, Token-Aware, All Channels)

---

## Ⅰ. 完了済み (Phase 0–7)

<details>
<summary>Phase 0–6 (36項目 ✅) — クリックで展開</summary>

### Phase 0: Foundation
- [x] ToolMetaRegistry + BM25 Search, EventLog & SnapshotStore, ExecutionProfile Classification
### Phase 1: Control Plane
- [x] Entropy Monitor, Atomic Rollback, Cognitive Scheduler
### Phase 2: Intelligence Layer
- [x] Capability Abstraction, Causal Memory Edges, Swarm Return Channel
### Phase 3: Advanced Features
- [x] CodeAct Hybrid Mode, Constitutional PolicyEngine, Durable Task Runtime
### Phase 4: Self-Evolving Intelligence
- [x] Skill Genome, Primitive Darwinism, Dream Mode, Model Auto-Routing, Swarm Economy, Plugin SDK v3, DeepSeek V4, DSGE Economics Engine, Skill Crystallization
### Phase 5: World Model & Integration
- [x] World Model Agent, Dynamic max_tokens, Codex Serendipity, Skill Library wiring, Cron, DB migration
### Phase 6: Robustification (全8セクション ✅)
- [x] World Model V2, Constraint Validation, Bayesian Scoring, Receptionist+Worker, CodeAct制限, Gemini fix, Snapshot戦略, Model Router強化
</details>

### Phase 7: Cognitive Compiler (全8セクション ✅)

| セクション | 内容 | 状態 |
|-----------|------|------|
| 7-1 | Simulation完全廃止 + Constraint Validation 一本化 | ✅ |
| 7-2 | Structured Output — Tool Use API スキーマ強制 | ✅ |
| 7-3 | PlanCompiler — Dead Step Elimination, Step Fusion, Parallel Discovery, Verify Placement | ✅ |
| 7-4 | Plan Cache with Semantic Dedup (Jaccard + Trigram, cache warming) | ✅ |
| 7-5 | Skill Compression Pipeline — trajectory→CompressedPattern→Rust candidate | ✅ |
| 7-6 | Receptionist + Worker — Discord, CLI, 横断Event Bus | ✅ |
| 7-7 | Token-Aware Plan Selection — cost ceiling, cheapest-first | ✅ |
| 7-8 | Self-Healing Failure Recovery — CompileError→auto-fix, Timeout→retry×2, HallucinatedPath→nearest match | ✅ |

---

## Ⅱ. Phase 8: Cognitive Runtime — 「思考の産業化」

**コアテーゼ**: Phase 7 で「コンパイル可能なプラン」を得た。Phase 8 では「プランが実行時に自らを最適化するランタイム」を作る。

**期間**: 2026年6月
**成功指標**:
- プラン再利用によるLLM呼び出し削減率 > 60%
- 投機的実行によるレイテンシ短縮 > 30%
- クロスタスクパターン検出数 > 10/week
- トークン単価あたりのタスク完了数 (Cognitive ROI) が Phase 7 比 2倍

### 8-1: Speculative Execution Engine 🟢

**現状**: 最良プラン(A)を本番実行しつつ、次点プラン(B)を別スレッド/サンドボックスで並列投機実行し、レイテンシと成功率を最適化。

**Phase 8 で達成したこと**:
- [x] `SpeculativeExecutor` — 最良プラン(A)を本番実行しつつ、次点プラン(B)をdry-run/サンドボックスで並列実行するスレッドディスパッチャの実装
- [x] **Shared Snapshot Isolation**: BのサンドボックスはAと同じSnapshotベースラインから開始。Aが成功したらBの結果はクリーンアップ廃棄
- [x] **Failover within 500ms**: Aが失敗した瞬間、スレッドをキャンセルし、Bのdry-run/サンドボックス実行結果を本番に即座にpromote
- [x] **Lightweight DryRun Option**: pure dry-run 時はディスクI/OとSnapshot生成を完全にスキップし、超低オーバーヘッドで動作

**ファイル**: `crates/core/src/orchestration/speculative.rs` 🟢 (実装完了, ユニットテスト完了)

**データフロー**:
```
Task → PlanGenerator → [Plan A (score=0.92), Plan B (score=0.78), Plan C (score=0.65)]
    ↓
SpeculativeExecutor:
  Thread 1: Plan A → Snapshot₁ → Execute → Verify
  Thread 2: Plan B → Snapshot₁ (shared) → Dry-run / Sandbox → Verify
  Thread 3: Plan C → Standby (no execution yet)
    ↓
  A succeeds → Promote A, discard B, keep C cached
  A fails → Promote B (already verified), activate C as new backup
```

### 8-2: Incremental Replanning Engine 🟢

**現状**: プラン実行中にノードが失敗した際、全体を捨てて再計画するのではなく、失敗したサブツリーのみを特定して差分置換（動的自己修復）。

**Phase 8 で達成したこと**:
- [x] `IncrementalPlanner` — 実行中に失敗したPlanNodeのサブツリー部分を正確に特定し、影響度を計算してピンポイントで差分再計画
- [x] **AST Subtree Replacer**: 計画全体（Sequence, Parallel, Conditional）のASTノードを再帰走査し、ターゲットノードを代替のセルフヒーリングノードへ安全に置換
- [x] **State Continuation**: 前段の成功したノード状態やSnapshot履歴はそのまま維持し、部分的な修正実行のみで完了

**ファイル**: `crates/core/src/orchestration/replan.rs` 🟢 (実装完了, ユニットテスト完了)

### 8-3: Cross-Task Pattern Mining 🟢

**現状**: 成功した全プランのASTを横断分析し、共通の構造サブツリーを抽出。パラメータ化されたテンプレート（例: `"add logging to {param_0}"`）としてライブラリ化し、新規タスクに対してLLMを介さずにASTを自動再生・高速代入（インスタンシエーション）するコグニティブ最適化エンジン。

**Phase 8 で達成したこと**:
- [x] `PatternMiner` — 成功した全プランのASTを横断分析し、構造（Sequence, Parallel等）が一致する共通サブツリーを自動抽出
- [x] **Template Extraction**: タスク文字列間の差分ワード（ファイル名やモジュール名）を自動検出して `{param_0}` などへプレースホルダー化し、動的な正規表現マッチング条件を自動生成
- [x] **Pattern Scoring**: 出現頻度 × 成功率 × freshness（鮮度減衰）による厳格なパターン評価スコアリングシステム
- [x] **Template Instantiation**: 新規タスクがテンプレートの正規表現に合致した際、変数部分を代入してASTをミリ秒で生成・即座に世界モデルに供給する仕組みの確立
- [x] **Pattern Library**: `~/.pharmakon/pattern_library.json` への読み書きと、実行成功時のバックグラウンド自動マイニング・保存フックの統合

**ファイル**: `crates/core/src/orchestration/pattern_miner.rs` 🟢 (実装完了, ユニットテスト完了)

### 8-4: Token Economy v2 — Bank of Pharmakon 🟢

**現状**: トークン消費のトラッキングと制限にとどまらず、タスクごとのコグニティブROI（投資対効果）を数理的に算出し、動的な予算配分と投資回収トラッキングを行う自律的金融管理システム。

**Phase 8 で達成したこと**:
- [x] **Cognitive ROI 計算式の実装**:
  ```rust
  pub fn calculate_roi(investment: &ResearchInvestment, cost_tokens: u64, impact_factor: f64) -> f64 {
      let weight = match investment {
          ResearchInvestment::Crystallization => 1.5,
          ResearchInvestment::Indexing => 1.2,
          ResearchInvestment::Inference => 1.0,
      };
      (impact_factor * weight) / (cost_tokens as f64).max(1.0)
  }
  ```
- [x] **ROI-driven Budget Allocation**: 高ROIタスクに予算を優先配分。低ROIタスクは Deferred または Rejected するスマートスケジューリング
- [x] **Token Reserve Policy**: `Reserve` / `Burst` / `Investment` からなる3層構造のトークンリザーブポリシー（`TokenReserve`）
- [x] **Investment Tracking**: `ResearchInvestment` カテゴリ（Crystallization, Indexing, Inference）ごとにモデル投資とトークン削減効果をトラッキング
- [x] **Bank of Pharmakon 統合**: `Agent` 内部に `BankOfPharmakon` インスタンスを統合し、タスク実行時の自律予算割り当て・投資判断フックを整備

**ファイル**: `crates/core/src/orchestration/economy_v2.rs` 🟢 (実装完了, ユニットテスト完了)

### 8-5: Plan AOT Compilation (Ahead-of-Time) 🟢

**現状**: 高頻度・高成功率の安定したプランテンプレートを、JITコンパイルやLLMパースを一切介さず、ミリ秒でロード可能なシリアライズド・バイナリに事前コンパイル（AOT化）してディスクキャッシュに永続化、およびネイティブRustソースコードとして「結晶化」出力する高速実行サブシステム。

**Phase 8 で達成したこと**:
- [x] **AOT Compiler**: 成功率が高く頻繁に実行されるプランテンプレートを抽出し、バイナリ形式にコンパイルする AotCompiler の開発
- [x] **Native Codegen**: AST構造からRustの構文ツリーコードを完全に自動生成（結晶化：`generate_crystallized_rust`）するジェネレータの実装
- [x] **Hot-reload**: `AotHotReloader` によるディスクキャッシュからのミリ秒以下の超高速動的復元ロードの統合
- [x] **Compilation Cache**: コンパイル済みバイナリ（`.bin`）を `~/.pharmakon/compiled/` へ安全にシリアライズ保存・バージョン管理する機構の構築

**ファイル**: `crates/core/src/orchestration/aot.rs` 🟢 (実装完了, ユニットテスト完了)

### 8-6: Distributed Execution Fabric 🟢

**現状**: シングルマシンの制約を超え、複数の分散計算ノード（Mac mini, ThinkPad, Android AVF等）にサブエージェントや speculative な検証プランをディスパッチして負荷分散・協調実行を行うクラスター連携ファブリック。

**Phase 8 で達成したこと**:
- [x] **Remote Agent Protocol**: リモートエンドポイントへ AST プランペイロード（`PlanNode`）を非同期に転送・ディスパッチする通信プロトコル
- [x] **Capability Advertisement**: 各ノードのハードウェア資源（GPU有無, RAM容量）やコンパイル・ツールチェーン（`cargo`, `clang`等）の能力広報（Capabilities Schema）
- [x] **Load-adaptive Routing**: タスクの最小RAM/GPU要件とツール要求を満たすオンラインノードをフィルタリングし、最も低負荷（`active_load_score` 最小）のノードへ自動配備するルーティングアルゴリズム
- [x] **Result Aggregation**: 遠隔実行の成功可否、コンパイル出力、レイテンシ時間等の統計を構造化オブジェクト `RemoteTaskResult` として親ノードに集約・返送する機構

**ファイル**: `crates/core/src/orchestration/fabric.rs` 🟢 (実装完了, ユニットテスト完了)

### 8-7: Causal Graph Agent Memory 🟢

**現状**: 実行履歴から「どの決定・操作が成功/失敗を誘発したか」の因果的な繋がりをDAGグラフ構造に落とし込み、根本原因究明や仮説推論を可能にする高レベル推論機構。

**Phase 8 で達成したこと**:
- [x] **Causal Graph Construction**: プラン・実行・検証とそれらの結果を結ぶ因果DAGを構築する CausalGraph の実装
- [x] **Counterfactual Reasoning**: 「もし別ルートを実行していたら？」の成功確率を過去の統計から算定する反事実推定エンジンの実装
- [x] **Root Cause Analysis**: 失敗ノードからトポロジを逆引きし、エラーを誘発した根本原因アクション（RCA）をバックトラック特定する機構
- [x] **Causal Policy**: 各タスクの因果パスを比較し、最も成功率の高い実行戦略を `recommend_policy` するプラン最適化連携の追加

**ファイル**: `crates/memory/src/causal_graph.rs` 🟢 (実装完了, ユニットテスト完了)

### 8-8: Continuous Self-Benchmarking 🟢

**現状**: プラットフォーム性能や推論コストの測定を規格化し、自動的なデグレード検知（回帰分析）や統計的有意差検定を組み込んだ高度クオリティ・アシュアランス（QA）サブシステム。

**Phase 8 で達成したこと**:
- [x] **Benchmark Harness**: 成功率・平均処理レイテンシ・トークン消費量を周期計測して履歴永続化（`~/.pharmakon/benchmarks.json`）するテストハーネス `BenchmarkHarness`
- [x] **Regression Detection**: 前回のテスト実行比で成功率が 10% 以上低下した場合にリアルタイムの重大デグレード警告を発行する `detect_regression` 検知器
- [x] **A/B Testing**: 異なるモデル選定やSpeculative並列等のプラン戦略（Variants）に対し、統計学的有意水準（信頼度 90%、Z値 1.645 以上）での Z検定比率判定を行い、改善実績を有意に確認した新戦略を自動採用する実験評価システム
- [x] **Performance Dashboard**: `pharmakon status` や管理画面でリアルタイムに傾向表示が可能な構造化テレメトリ収集

**ファイル**: `crates/core/src/orchestration/benchmark.rs` 🟢 (実装完了, ユニットテスト完了)

---

## Ⅲ. Phase 8 ファイル別実装規模

| ファイル | 操作 | 規模 | 主な変更 |
|---------|------|------|---------|
| `speculative.rs` | 新規 | +400行 | SpeculativeExecutor, shared snapshot isolation, failover, DSGE連携 |
| `economy_v2.rs` | 新規 | +350行 | Cognitive ROI計算, Budget Allocation, Token Reserve, Investment Tracking |
| `fabric.rs` | 新規 | +400行 | Remote Agent Protocol, Capability Advertisement, Load Routing |
| `pattern_miner.rs` | 新規 | +300行 | AST横断分析, Template Extraction, Pattern Scoring |
| `causal_graph.rs` | 新規 | +300行 | Causal Graph構築, Counterfactual Reasoning, Root Cause Analysis |
| `replan.rs` | 新規 | +250行 | IncrementalPlanner, Affected Node Detection, State Continuation |
| `benchmark.rs` | 新規 | +250行 | Benchmark Harness, Regression Detection, A/B Testing |
| `aot.rs` | 新規 | +200行 | AOT Compiler, Native Codegen, Hot-reload, Compilation Cache |
| `compiler.rs` | 変更 | +50行 | AOT統合, pattern_miner連携 |
| `agent.rs` | 変更 | +50行 | SpeculativeExecutor統合, fabric dispatcher |
| `dsge_integration.rs` | 変更 | +30行 | ROI計算式, Investment ROI tracking |

**総追加**: ~2,580行 / **ネット**: +2,630行

---

## Ⅳ. Phase 8 優先順位マップ

```
Week 1-2 (P0 - アーキテクチャの核 - 済):
  8-1: Speculative Execution Engine (並列実行 of 基盤) 🟢
  8-4: Token Economy v2 (経済的意思決定 of 基盤) 🟢
  8-2: Incremental Replanning (実行時適応 of 基盤) 🟢

Week 3-4 (P1 - 知能 of 産業化 - 済):
  8-3: Cross-Task Pattern Mining (知識 of 横断活用) 🟢
  8-5: Plan AOT Compilation (実行 of 高速化) 🟢
  8-7: Causal Graph Agent Memory (因果推論) 🟢

Week 5-6 (P2 - 分散化と計測 - 済):
  8-6: Distributed Execution Fabric (マルチノード展開) 🟢
  8-8: Continuous Self-Benchmarking (品質保証) 🟢
```

---

## Ⅴ. フェーズ総括

| Phase | 名前 | 期間 | 主な成果 | コード規模 |
|-------|------|------|---------|-----------|
| 0 | Foundation | 済 | ToolMetaRegistry, EventLog, SnapshotStore, ExecutionProfile | +1500行 |
| 1 | Control Plane | 済 | Entropy Monitor, Atomic Rollback, Cognitive Scheduler | +800行 |
| 2 | Intelligence Layer | 済 | Capability Abstraction, Causal Memory, Swarm Channel | +1200行 |
| 3 | Advanced Features | 済 | CodeAct, Constitutional Engine, Durable Tasks | +1000行 |
| 4 | Self-Evolving | 済 | Skill Genome, Dream Mode, Model Router, DSGE, Swarm Economy | +2500行 |
| 5 | World Model | 済 | World Model Agent, Dynamic max_tokens, Codex Serendipity | +800行 |
| 6 | Robustification | 済 | World Model V2, Constraint Validation, Bayesian Scoring, Receptionist | +600行 |
| 7 | Cognitive Compiler | 済 | PlanCompiler, Structured Output, Skill Compression, Self-Healing | +760行 |
| 8 | Cognitive Runtime | 済 | Speculative Exec, Token Economy v2, Incremental Replan, Pattern Mining, AOT Compilation, Causal Memory, Distributed Fabric, Benchmarks | +2620行 |
| **合計** | | | | **~13,600行** |

---

## Ⅵ. Phase 9+ 展望: Autonomous Evolution

Phase 8 が完了した時点で、Pharmakonは「自律的に学習・最適化・分散実行するコグニティブランタイム」として動作する。その先にあるのは:

### 9-1: Self-Modifying Agent
- コンパイル済みプラン(AOT)を自身のバイナリに組み込み
- 使用頻度の低いプランを自動削除（コードベースのガベージコレクション）
- 実行時プロファイリングに基づくホットパスの自動最適化

### 9-2: Inter-Agent Protocol
- 複数のPharmakonインスタンス間でのタスク分散
- 専門化されたサブエージェントのP2Pマーケットプレイス
- エージェント間での学習成果（プラン、パターン、因果グラフ）の共有

### 9-3: Full Autonomy Mode
- 人間の介入ゼロでの長期自律運用
- 自己診断 → 自己修復 → 自己改善の完全閉ループ
- Constitutional Engine による安全性の保証（不変）
