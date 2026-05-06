# Pharmakon 4.0: Deep Research Specification

## Overview
Deep Research enables autonomous, multi-step investigations by managing a recursive research tree and optimizing context window usage.

## 1. Context Management
### 1.1 Working Memory (WM)
- **Concept**: A high-speed, importance-weighted buffer (Top 10-15 units).
- **Mechanism**:
  - New findings are inserted into WM.
  - If WM is full, the least important item is evicted to LanceDB.
  - LLM input is always "packed" with current WM contents.

### 1.2 Research Notebook
- **State**: Persistent JSON tracking the investigation status.
- **Fields**:
  - `verified_facts`: Confirmed technical data.
  - `pending_questions`: Hypotheses to test.
  - `research_tree`: Log of paths explored and results.

## 2. Execution Loop
1. **Goal Setting**: Define the primary research objective.
2. **Strategy Selection**:
   - `Skim`: Fast overview.
   - `Exhaustive`: Recursive deep-dive.
3. **Step Execution**:
   - Call `smart_search` for internal context.
   - Call `web_fetch` for external data.
4. **Fact Extraction**: Filter noise and store only verified nuggets.
5. **Goal Verification**: Check if the objective is met.

## 3. Token Efficiency
- HTML/Text is stripped to markdown.
- Long documents are summarized before being added to WM.
- Vector search is used to find specific blocks within large files.
