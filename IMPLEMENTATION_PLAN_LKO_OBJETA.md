# Pharmakon 実装計画 — LKO / objeta 知見転用

2026-05-22 | **STATUS: ✅ 全 Phase 実装完了 (81 tests passing)**

LKO（MLX 推論最適化研究）と objeta（LLM 推論 OS）の全知見から、Pharmakon のエージェント信頼性・安定性に直結する改善を抽出・計画化したもの。

---

## 背景

LKO と objeta は、LLM 推論を「動的リソース割り当て問題」として捉え、entropy / hysteresis / 競合制御ループ / 停滞検出の各技法を確立した。Pharmakon のエージェントループは既にエントロピー監視・健康状態機械・バジェット制御を持つが、これらの精緻化によって以下の効果が期待できる：

- エージェントのループ検出精度の向上（早期介入）
- 偽陽性 termination の削減（hysteresis 導入）
- ツール多様性と予算のバランス最適化（競合制御ループ）
- 微細な停滞の早期検出（iteration 間 cosine 監視）

---

## Phase 1: マルチティア・エントロピー応答（推定 2h）

### 現状

`agent.rs` L952-990 — 単一閾値構成：
- entropy > 0.8 → 警告メッセージ注入
- entropy > `PHARMAKON_MAX_ENTROPY`（デフォルト 0.95）→ hard-terminate

問題点：0.8 と 0.95 の間に段階的応答がない。0.81 でも 0.94 でも同じ警告メッセージが出るだけ。

### 変更内容

#### 1-a: 3 段階エントロピーレスポンス（`agent.rs`）

```
Tier 1: entropy > 0.5  →  ツール多様性強制（serendipity injection 数を 3→6 に増加）
Tier 2: entropy > 0.7  →  戦略再考プロンプト注入 + モデル温度 0.7→1.0 に一時変更
Tier 3: entropy > 0.85 →  フォールバックモデルへの自動切替 + 最終警告
Tier 4: entropy > 0.95 →  hard-terminate（既存）
```

#### 1-b: 環境変数化

```
PHARMAKON_ENTROPY_TIER1   デフォルト 0.50
PHARMAKON_ENTROPY_TIER2   デフォルト 0.70
PHARMAKON_ENTROPY_TIER3   デフォルト 0.85
PHARMAKON_MAX_ENTROPY     デフォルト 0.95（既存）
```

#### 1-c: エントロピーヒステリシス（objeta Scheduler の hysteresis パターン）

現在の実装では、entropy が 0.8 を超えるたびに毎回警告メッセージが注入される。objeta の Scheduler はヒステリシス（不感帯）を導入し、状態振動（class oscillation）を 15→3/run に削減した。

```
Tier N に進入: 即時応答
Tier N から降格: entropy が「進入閾値 - 0.05」を下回るまで待機（不感帯 0.05）
```

これにより、entropy が 0.79↔0.81 の境界を振動しても応答が乱高下しない。

### 影響ファイル

| ファイル | 変更内容 |
|---------|---------|
| `crates/core/src/agent.rs` | エントロピーチェックブロック（L952-990）を 4 段階に拡張。Tier1/2 ではプロンプト注入と serendipity 増加。Tier3 でモデル切替。ヒステリシス状態追跡用フィールド追加 |
| `crates/core/src/orchestration/budget.rs` | `ProgressTracker` に `entropy_tier: Option<u8>` フィールド追加。`check_entropy()` が TieredEntropySignal を返すよう拡張 |
| `crates/core/src/event_log.rs` | `EventKind::EntropyAlert` に `tier: u8` フィールド追加 |

---

## Phase 2: 競合制御ループ Governor（推定 3h）

### 現状

Pharmakon には以下の独立した制御機構がある：

- `ToolPolicyEngine`（クールダウン、理由必須）
- `ExplorationBudget`（ファイル数・深さ・トークン制限）
- `AttentionScheduler`（ファイル注目度スコア）
- `ToolGovernor`（既に存在するが内部未確認）

これらは独立に動作し、競合時の優先度仲裁がない。

### 変更内容

#### 2-a: Governor パターンの導入（`orchestration/governor.rs` の拡張）

objeta の Governor アーキテクチャを参考に、3 つの制御ループを統合：

```rust
pub struct IntegratedGovernor {
    // ループ1: 安全保護（最優先）
    pub safety_guard: SafetyGuard,
    // ループ2: 品質保護
    pub quality_guard: QualityGuard,
    // ループ3: リソース保護（最低優先）
    pub resource_guard: ResourceGuard,
}
```

| ループ | トリガー | 応答 | 優先度 |
|--------|---------|------|--------|
| SafetyGuard | 破壊的コマンド検出、ポリシー違反 | 即時ブロック | 1（最高） |
| QualityGuard | エントロピー上昇、loop 検出、停滞 | ツール多様性強制、モデル切替 | 2 |
| ResourceGuard | トークン枯渇、ディスク圧迫 | レスポンス圧縮、ツール制限 | 3（最低） |

**仲裁原則**（objeta Governor から）: QualityGuard（知性保護）が ResourceGuard（コスト保護）に勝つ。SafetyGuard は両方に無条件で勝つ。

#### 2-b: DynamicLambda パターン（objeta governor.py から）

objeta の `DynamicLambda` は per-token の制御パラメータを動的に調整する：

```
λ = base × class_multiplier × thrash_multiplier × collapse_multiplier
```

Pharmakon では `ExplorationBudget` の制限値（max_files, max_tokens, max_depth）を動的スケーリングする形で導入：

```rust
impl IntegratedGovernor {
    pub fn dynamic_budget_scale(&self, entropy: f32, stall_count: usize) -> f32 {
        let base = 1.0;
        let entropy_factor = if entropy > 0.7 { 1.5 } else { 1.0 };
        let stall_factor = 1.0 + (stall_count as f32 * 0.2).min(1.0);
        base * entropy_factor * stall_factor
    }
}
```

エントロピー高 + 停滞時 → バジェット拡大（より多くの探索を許可）。正常時 → バジェット縮小（コスト抑制）。

### 影響ファイル

| ファイル | 変更内容 |
|---------|---------|
| `crates/core/src/orchestration/governor.rs` | `IntegratedGovernor` 実装。3 ループ + 仲裁ロジック + DynamicLambda |
| `crates/core/src/orchestration/tool_scheduler.rs` | `ToolScheduler` が Governor を参照し、予算制限を動的調整するよう変更 |
| `crates/core/src/agent.rs` | Agent 構造体の `governor` フィールドを `IntegratedGovernor` に差し替え。ループ内で `governor.evaluate()` を呼び出す |

---

## Phase 3: イテレーション間 Cosine 停滞検出（推定 1.5h）

### 現状

`budget.rs` の `ProgressTracker::measure_delta()` は、前回より `successful_tool_calls` が増えたかどうかだけを進捗指標としている。これは非常に粗い。

LKO Adaptive Runtime の `hidden_cos > 0.96 → IDENTITY` パターンを転用し、**iteration 間のツール呼び出しパターンの cosine 類似度**で微細な停滞を検出する。

### 変更内容

#### 3-a: ツール呼び出し埋め込みの導入（`budget.rs`）

```rust
/// 各イテレーションのツール呼び出しパターンを低次元特徴ベクトルに変換。
/// [tool_name_hash, args_entropy, success_rate, latency_p50]
pub struct IterationEmbedding {
    pub features: [f32; 4],
}

impl IterationSnapshot {
    pub fn to_embedding(&self) -> IterationEmbedding { ... }
}
```

#### 3-b: Cosine ベースの停滞検出

```rust
impl ProgressTracker {
    pub fn cosine_stagnation(&self, current: &IterationEmbedding) -> f32 {
        let Some(prev) = self.embeddings.back() else { return 0.0 };
        cosine_similarity(&prev.features, &current.features)
    }
}
```

停滞判定:
- `cos > 0.98` → ほぼ同一パターン → 即時介入（モデル切替または戦略変更プロンプト）
- `cos > 0.95` → 微細な停滞 → ツール探索範囲拡大
- `cos < 0.90` → 活発な状態変化 → 通常運用

#### 3-c: ProgressBased ポリシーの精緻化

現在の `ProgressBased{stall_threshold}` は単純な連続停滞カウントだが、cosine 値を組み合わせることで、より早期の介入が可能になる：

```rust
// 旧: N 回連続で successful_tool_calls 不変 → stall
// 新: (stall_count >= threshold) || (cosine_stagnation > 0.98 が 2 回連続) → early intervention
```

### 影響ファイル

| ファイル | 変更内容 |
|---------|---------|
| `crates/core/src/orchestration/budget.rs` | `IterationEmbedding`、`cosine_similarity()`、`cosine_stagnation()` 追加。`record()` 内で cosine チェック追加 |
| `crates/core/src/agent.rs` | `IterationSnapshot` 生成時に embedding 計算用のデータ（ツール成功率、レイテンシ等）を収集 |

---

## Phase 4: デッドエンドカタログ（推定 0.5h）

### 現状

Pharmakon のコードベース・設計文書には「うまくいかないアプローチ」の集約された記録がない。

### 変更内容

LKO/objeta が実証的に「死んだ」と判定したアプローチを `PHARMAKON.md` にセクション追加：

- **Hidden state caching**: h は急速に回転（cos≈0）。隠れ状態の再利用は原理的に困難
- **Koopman multi-step prediction**: A^n が合成しない。多段予測は無効
- **FFN low-rank rotation**: 22 層 rollout で cos=0.17。低ランク回転による近似は崩壊
- **Temperature scaling on load-balanced routing**: 一様 logits は一様のまま。ルーティング多様化に温度は無効

また、Pharmakon 自身の内部で試みられたが無効だったアプローチがあれば追記する。

### 影響ファイル

| ファイル | 変更内容 |
|---------|---------|
| `PHARMAKON.md` | `## Dead-End Catalog` セクション追加 |
| `ARCHITECTURE.md` | 設計判断の根拠としてデッドエンド参照を追加 |

---

## Phase 5: クロスセッション・トピッククラスタリング（推定 3h）

### 現状

`KnowledgeNexus` は access-aware decay を持つが、セッション間で共通トピックの知識を明示的に共有する機構がない。objeta の L3 cross-request cache（トピッククラスタリング 92-100% カバレッジ）は、Pharmakon の長期記憶に直接応用可能。

### 変更内容

#### 5-a: トピッククラスタリング層の追加（`weaver.rs`）

```rust
pub struct TopicCluster {
    pub centroid: Vec<f32>,       // 埋め込み重心
    pub members: Vec<String>,     // ノードID
    pub access_count: u64,
    pub last_accessed: DateTime<Utc>,
}
```

- 埋め込み空間上での k-means クラスタリング（定期的に実行、または閾値ベースでオンライン更新）
- 同一クラスタ内のノードは decay suppression を受け、access-aware decay の影響を緩和

#### 5-b: クロスセッション検索の強化

- `smart_search()` 実行時、現在セッションのトピッククラスタを同定
- 同一クラスタ内の他セッションノードを優先的に検索結果に含める
- ただしセッション隔離（`[Session: <id>]` プレフィックスフィルタ）は維持

### 影響ファイル

| ファイル | 変更内容 |
|---------|---------|
| `crates/memory/src/weaver.rs` | `TopicCluster` 構造体、`update_clusters()`、クラスタ考慮版 `smart_search()` |
| `crates/core/src/agent.rs` | `gather_context()` でトピッククラスタ検索を利用 |

---

## 優先度・工数・順序

| Phase | 内容 | 工数 | 依存 | 効果 |
|-------|------|------|------|------|
| **1** | マルチティア・エントロピー応答 + ヒステリシス | 2h | なし | ループ検出精度↑、偽陽性↓ |
| **2** | 競合制御ループ Governor | 3h | Phase 1 | ツール多様性・安全性バランス最適化 |
| **3** | Cosine 停滞検出 | 1.5h | なし | 早期介入精度↑ |
| **4** | デッドエンドカタログ | 0.5h | なし | 将来の無駄防止 |
| **5** | クロスセッション・トピッククラスタリング | 3h | なし | 長期記憶品質↑ |

### 推奨実施順序

```
Phase 4（準備）→ Phase 1（基盤）→ Phase 3（検出）→ Phase 2（統合）→ Phase 5（拡張）
```

Phase 1 と Phase 3 は独立しているため並行実装可能。Phase 2 は Phase 1 のエントロピー応答に依存するため後回し。

---

## 検証計画

各 Phase 完了時の検証ゲート：

### Phase 1
- 単体テスト: 各 Tier のエントロピー閾値で適切な応答が発動することを確認
- ヒステリシス: 境界振動（例: 0.79↔0.81 の 100 回振動）で応答が 1 回以下であることを確認
- 統合テスト: `test_entropy_high_for_loop` を拡張し、Tier 応答の段階的発動を検証

### Phase 2
- 単体テスト: 競合時に SafetyGuard > QualityGuard > ResourceGuard の順で仲裁されることを確認
- 単体テスト: DynamicLambda がエントロピー上昇時に予算を適切にスケーリングすることを確認

### Phase 3
- 単体テスト: 同一ツール呼び出し 3 回で cosine > 0.98 が検出されることを確認
- 単体テスト: 多様なツール呼び出しで cosine < 0.50 になることを確認

### Phase 5
- 単体テスト: トピッククラスタリング後の検索精度が baseline を上回ることを確認
- セッション隔離: 他セッションのノードが誤って漏洩しないことを確認

---

## 非対応項目（将来検討）

以下は今回の計画から意図的に除外した。理由を付記。

| 項目 | 除外理由 |
|------|---------|
| Non-normal transport theory の形式的導入 | 理論的価値は高いが、直接的な実装改善に結びつかない。設計文書・論文の参照として保留 |
| Phase diagram のエージェント行動分類 | セッション位相分類は興味深いが、まずは基本のエントロピー監視を強化すべき |
| 100% replay determinism の EventLog 検証 | EventLog/SnapshotStore の単体テストで十分カバー可能。objeta の TokenTrace のような完全決定論的再生は、Pharmakon のユースケースでは過剰 |
| Expert offloading / KV cache 最適化 | Pharmakon は LLM 推論そのものを行わない（外部 API 依存）。LKO の推論最適化技法は適用対象外 |

---

## 実装完了サマリ（2026-05-22）

### 実装された改善

| Phase | 内容 | 主な変更ファイル |
|-------|------|-----------------|
| 1 | マルチティア・エントロピー応答 + ヒステリシス | `budget.rs`, `event_log.rs`, `agent.rs` |
| 2 | 競合制御 Governor + DynamicLambda | `governor.rs`, `tool_scheduler.rs`, `agent.rs` |
| 3 | Cosine 停滞検出 | `budget.rs`, `agent.rs` |
| 4 | デッドエンドカタログ | `PHARMAKON.md` |
| 5 | クロスセッション・トピッククラスタリング | `weaver.rs`, `agent.rs` |

### 副次的修正

- `registry.rs`: DeepSeek モデルが perplexity ブロック内に誤ってネストしていたのを修正
- `tui.rs`: 未閉じデリミタ + 借用競合を修正
- `agent.rs` / `model_router.rs`: モック判定を文字列比較から `is_mock()` にリファクタリング
- `agent_types.rs`: `AgentModel` trait に `is_mock()` を追加
- 全テストファイル: ローカルモックに `is_mock()` を追加

### 検証結果

```bash
cargo check --workspace  # 0 errors
cargo test -p pharmakon-core  # 81 passed, 2 ignored
cargo test -p pharmakon-memory  # 4 passed
```
