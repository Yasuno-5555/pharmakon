# Pharmakon 実装計画書 — Phase 5 → 6

**作成日**: 2026-05-09
**Phase 5 complete**: World Model Agent, Dynamic max_tokens, Codex Serendipity, Skill Library wiring, Cron, DB migration

---

## Ⅰ. 完了済み (Phase 0–5)

### Phase 0: Foundation
- [x] ToolMetaRegistry + BM25 Search (65+ tools, deferred hydration)
- [x] EventLog & SnapshotStore (append-only JSONL, content-addressed file snapshots)
- [x] ExecutionProfile Classification (SideEffectLevel, FilesystemScope, Reversibility)

### Phase 1: Control Plane
- [x] Entropy Monitor (4-factor inline scoring, hard-terminate >0.95)
- [x] Atomic Rollback (rollback_to_snapshot, rollback_to_event)
- [x] Cognitive Scheduler (Simple/Standard/Deep, LLM classify)

### Phase 2: Intelligence Layer
- [x] Capability Abstraction (65 tools → 10 capabilities, ~90% token reduction)
- [x] Causal Memory Edges (caused_by, fixed_by, invalidated_by)
- [x] Swarm Return Channel (SpawnHandle with oneshot::Receiver)

### Phase 3: Advanced Features
- [x] CodeAct Hybrid Mode (Rhai → Python fallback, ToolCategory::Core)
- [x] Constitutional PolicyEngine (immutable safety rules)
- [x] Durable Task Runtime (suspend/resume with TaskSnapshot)

### Phase 4: Self-Evolving Intelligence
- [x] Skill Genome System (SkillGenome, CompositeSkill, CrystallizationCandidate)
- [x] Primitive Darwinism (experimental → stable → core → deprecated → removed)
- [x] AntiPattern Extraction (positive guidance injection)
- [x] Dream Mode (background self-play, decay cycle)
- [x] Model Auto-Routing (ModelMode::Auto, ModelPerformanceTracker)
- [x] Swarm Economy (GeneralEquilibrium.market_clearing)
- [x] Plugin SDK v3 (Tool/Plugin traits, ExecutionProfile, AgentErrorCode)
- [x] DeepSeek V4 (deepseek-v4-flash, deepseek-v4-pro)
- [x] DSGE Economics Engine (CognitiveBudget, BellmanPlanner, RegimeSwitcher, ProviderPortfolio — 6 injection points)
- [x] Skill Crystallization (suggest_crystallizations)

### Phase 5: World Model & Integration
- [x] World Model Agent (Plan Generator, MCTS Simulator, Commit & Rollback)
- [x] Dynamic max_tokens (model-aware: DeepSeek 16384, Gemini 8192, others 4096)
- [x] Codex Serendipity (random non-core tool injection, 3 per turn)
- [x] Skill Library wiring (labeled scripts from codeact, decay cycle, Dream Mode)
- [x] Cron scheduling (CronManager.list_jobs, cancel_job)
- [x] DB migration (name column added)

---

## Ⅱ. Phase 5 コードレビュー → 修正済み項目

2026-05-09 コードレビューで指摘を受けた問題とその修正状況：

| # | 指摘 | 状態 |
|---|------|------|
| 1 | JSONパース: `find('[')` のみで対応する `]` を探していない | ✅ 修正済み — `rfind(']')` でマッチ |
| 2 | EVPIがハードコード定数 (0.5, 0.7, 0.8) | ✅ 修正済み — `skill_library.query_few_shots(task, 3)` で実類似度計算 |
| 3 | shell/codeactのファイル変更がスナップショット漏れ | ✅ 修正済み — 全ツール実行前にworkspace全体 `snapshot_dir` |
| 4 | `run_rlfc_validation` と `run_cargo_check` が重複 | ✅ 修正済み — 重複削除、`run_cargo_check` に統一 |
| 5 | `.git` 除外によるシミュレーション即死トラップ | ⏳ P2 — `copy_workspace_lightweight` に `.git` 含めるオプション追加 |
| 6 | `simulate_plan` の未知ツール素通し | ⏳ P2 — シミュレーションを制約検証に置き換え（Phase 6 で対応） |
| 7 | 実行ループのモノリシック化 (Fat Function) | ⏳ P2 — Planner/Executor分離（Phase 6 で対応） |
| 8 | `score_plan` がヒューリスティック | ⏳ P2 — Bayesian推定に移行（Phase 6 で対応） |

---

## Ⅲ. Phase 6: Robustification Roadmap

**期間目標**: 2026年5月
**メイン指標**: `/plan` 成功率 > 80%, `/model auto` でタスク複雑度に応じた適切なモデル選択, 即死ゼロ

### 6-1. World Model: V1 → V2 完全移行 🔴 P0

**現状の V1 問題点**:
- Planner + Executor が同一関数内で結合 (Fat Function)
- Temp-dir simulation (「夢」) が現実と乖離
- スコアリングがヒューリスティック
- 未知ツールを素通し

**V2 アーキテクチャ**:
```
WorldModelPlanner (計画のみ生成)
    ↓
StaticVerifier (実行前に制約検証)
    ↓
Agent Loop (既存の安全実行基盤)
    ↓
FailureTaxonomy → Planner feedback (失敗から学習)
    ↓
CachedPlan (freshness decay で腐ったプランを淘汰)
```

**実装対象**:
- [ ] `PlanNode` AST (Sequence, Parallel, Conditional, Retry, Verify, Gate) ← 設計済み
- [ ] `StaticVerifier` (危険shell, 幻覚パス, リスク上限違反検出) ← 設計済み
- [ ] `CachedPlan` with freshness decay (半減期1週間, 環境フィンガープリント) ← 設計済み
- [ ] `FailureTaxonomy` (9種類, 回復可能性分類, Planner feedback) ← 設計済み
- [ ] `plan_and_execute()` → `execute_world_model()` の置換
- [ ] `/plan` コマンド配線

**ファイル**: `crates/core/src/orchestration/world.rs`

### 6-2. Constraint Validation → Simulation 置換 🟡 P1

**方針**: Temp-dir実行を廃止し、事前制約検証に寄せる。

**ConstraintChecker**:
- [x] Syntax validity (JSON salvage + retry 2回) ← 実装済み
- [x] File existence (Assertion::FileExists) ← 実装済み
- [x] Command availability (Assertion::CommandAvailable) ← 実装済み
- [ ] Dependency graph validity (AST/compile graph check)
- [ ] Patch applicability (dry-run apply_patch → verify syntax)
- [ ] Risk ceiling enforcement (RiskLevel > constraints.risk_ceiling → reject)

### 6-3. Bayesian Score Estimation 🟡 P1

**現状**: `score_plan(steps_ok, cargo_ok, estimated_tokens)` — ヒューリスティック

**あるべき姿**:
```rust
fn bayesian_score(plan: &Plan, library: &RhaiSkillLibrary, task: &str) -> f64 {
    let prior = library.success_rate_for_category(&plan.category);
    let similarity = genome_similarity(plan, task);
    let toolchain_risk = plan.steps.iter()
        .filter(|s| s.risk > RiskLevel::FileSystem).count() as f64 * 0.1;
    (prior * similarity - toolchain_risk).max(0.0)
}
```

### 6-4. Receptionist + Worker Agent — 全チャンネル展開 🟡 P1

**現状**: Telegramのみ対応済み。Discord, CLIも同様に。

**Telegram (実装済み)**:
- [x] コマンド (`/model`, `/new`, `/approve`, `/deny`, `/plan`) → receptionist即時応答
- [x] タスク → "🟢 Task dispatched to worker" → worker agent
- [x] Worker結果 → Telegramに送信

**未対応**:
- [ ] Discordチャンネル
- [ ] CLIインタラクティブモード

### 6-5. CodeAct 制限強化 🟢 P2

**指摘**: CodeActが mini-agent 化し、World Model 内で CodeAct → more planning → 再帰崩壊。

**対策**:
- [ ] System prompt: CodeAct は "macro skill" としてのみ使用可能
- [ ] 基本操作は primitive tools (shell, apply_patch, read_file, grep) を優先
- [ ] CodeAct 使用制限: 1タスクあたり最大2回
- [ ] World Model 内で CodeAct を plan step として使わせない（PlanGenerator prompt で明示）

### 6-6. Gemini 空レスポンス問題 🔴 P0

**現象**: "Gemini returned a candidate with no parts. Finish reason: STOP"
**原因**: `finish_reason == "STOP"` で no parts のケースを適切に処理できていない

**修正**:
- [ ] `finish_reason == "STOP"` && no parts → `content = Some(MessageContent::Text(String::new()))` に設定
- [ ] `finish_reason == "MAX_TOKENS"` → トークン上限に達したことを明示的にフィードバック
- [ ] 空レスポンス連続2回 → fallbackモデルに切り替え

### 6-7. スナップショット戦略の強化 🟡 P1

**指摘**: shell/codeact のファイル変更がスナップショット漏れ

**修正**:
- [x] `execute_world_model` の全ツール実行前に workspace 全体 `snapshot_dir` ← 実装済み
- [ ] Agent loop のツール実行前にも同様のworkspaceスナップショット
- [ ] `.git` ディレクトリをスナップショット対象に含める（シミュレーション精度向上）

### 6-8. モデルルーター強化 🔴 P0 (一部完了)

- [x] Deepタスク → 高出力モデル優先 (DeepSeek V4: +0.3 bonus) ← 実装済み
- [x] Simpleタスク → 安価モデル優先 (Groq/Llama: +0.2 bonus) ← 実装済み
- [ ] 出力トークン枯渇検知 → 次回呼び出し時により大容量モデルに自動切替
- [ ] モデル別 latency 追跡 → `recommend_max_tokens` にフィードバック

---

## Ⅳ. ファイル別実装規模

| ファイル | 現状行数 | 変更規模 | 主な変更 |
|---------|---------|---------|---------|
| `world.rs` | 560 | +400 | V2完全リライト (Plan Node AST, StaticVerifier, CachedPlan, FailureTaxonomy) |
| `agent.rs` | ~1600 | +50 | `/plan` コマンド, receptionist dispatching |
| `channels/telegram.rs` | ~280 | +10 | 既にworker spawn済み、微調整のみ |
| `channels/discord.rs` | ~100 | +80 | Receptionist + Worker パターン移植 |
| `providers/gemini.rs` | ~600 | +10 | 空レスポンス処理改善 |
| `skill_library.rs` | ~350 | +30 | Bayesian score helper |

---

## Ⅴ. 優先順位

```
P0 (即時):  Gemini空レスポンス, World Model V2, /plan コマンド
P1 (今週):  Constraint Validation, Bayesian Score, Discord Worker, スナップショット強化
P2 (来週):  CodeAct制限, .gitスナップショット, シミュレーション廃止
```

---

## Ⅵ. Phase 7+ 展望

### Simulation → Constraint Validation 完全移行 (Phase 7)
- 全プラン検証を制約充足問題として定式化
- Temp-dir実行の完全廃止
- SAT/SMT solver 的アプローチによる静的検証

### Skill Compression: Episodic → Procedural (Phase 7)
- 成功した10-step Plan → 1 CompositeSkill への圧縮
- SkillLibrary からの Plan 検索・再利用
- Rhai→Rust native crystallization の自動化

### Cognitive Runtime (Phase 8)
- Plan をコンパイル可能な中間表現に
- Static optimization (dead step elimination)
- Speculative execution with rollback
- Multi-plan competitive execution

### Token Economy v2 (Phase 8)
- 全ツール呼び出しに token cost メタデータ付与
- Cognitive ROI = ΔCapability / (TokenCost + Latency + Energy)
- API quota futures / outage insurance (Bank of Pharmakon)
