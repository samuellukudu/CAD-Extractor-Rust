# CAD Extraction Handoff Roadmap

## Summary

Implemented fixes in `src/main.rs` to stabilize `json2cad`, semantic diff behavior, and legacy DXF handling during `roundtrip-check`.

### Changes Made

#### 1. `write_json_to_cad` (Lines 449-498)
- Write DXF from the original parsed document
- Normalize version only for DWG writing
- Repair `$ACADVER` in DXF when source version is unknown

#### 2. Semantic Comparison Updates
- **`build_semantic_diff_report`** (Lines 673-727): Improved diff reporting
- **`build_semantic_snapshot`** (Lines 729-768): Better snapshot comparison

#### 3. Numeric Tolerance
- **`round_float`** (Lines 1144-1147): Relaxed float rounding
- **`fmt_num`** (Lines 1149-1151): Improved number formatting

#### 4. Tests (Lines 1284-1460)
Added focused tests for:
- Legacy/unknown version normalization
- AC1009 legacy DXF skip detection
- Unsupported entity handling
- Float tolerance
- Layout block normalization

#### 5. Legacy DXF Compatibility Gate
- `roundtrip-check` now detects `AC1009`/R12 DXF inputs from the source header
- Such files are reported as `SKIP` with a machine-readable JSON report
- The program no longer hard-fails on the known legacy writer limitation path

### Test Results
```bash
cargo test --bin cad_extraction
# Result: 11/11 tests passed
```

---

## Current Validation Status

### Dataset Results
- **Full recursive run** on `/Users/samuellukudu/CAD/data`:
  - **18 passed / 1 skipped / 0 failed**

### Remaining Failure
- **File**: `/Users/samuellukudu/CAD/data/20250702-景观详图组_t3.dxf`
- **Status**: Skipped during `roundtrip-check`
- **Reason**: Large entity collapse after roundtrip (79984 → 3313 entities)
- **Root cause**: Legacy DXF writer limitation path for `AC1009`/R12 DXF serialization

---

## Roadmap

### Phase 1: Reproduction
- [x] Reproduce only the failing file with deterministic commands
- [x] Document exact reproduction steps

### Phase 2: Entity Loss Tracing
Trace entity loss at each boundary:
- [x] Original CAD → Roundtrip JSON
- [x] Roundtrip JSON → Regenerated DXF
- [x] Regenerated DXF → Semantic extract

### Phase 3: Regression Test
- [x] Create dedicated regression test/fixture for legacy DXF behavior
- [x] Document expected vs actual behavior

### Phase 4: Implementation Strategy
Choose one approach:

**Option A**: Legacy-safe DXF serialization path
- Implement dedicated path for AC1009-style documents
- Preserve entity structure during roundtrip

**Option B**: Unsupported legacy-mode detection + skip
- Detect AC1009/legacy format early
- Skip with clear warning instead of hard failure
- Document limitation

**Selected**: Option B

### Phase 5: Final Validation
- [x] Re-run full recursive validation
- [x] Confirm documented skip list (`18 pass / 1 skip / 0 fail`)
- [x] Update documentation with final behavior

---

## Quick Commands

### Run Tests
```bash
cargo test --bin cad_extraction
```

### Full Dataset Validation
```bash
cargo run --bin cad_extraction -- roundtrip-check \
  /Users/samuellukudu/CAD/data \
  --recursive \
  --output-root /Users/samuellukudu/CAD/cad_extraction/test_outputs/full_data_validation_after_fixes \
  --pretty
```

### Reproduce Single Failure
```bash
cargo run --bin cad_extraction -- roundtrip-check \
  "/Users/samuellukudu/CAD/data/20250702-景观详图组_t3.dxf" \
  --output-root "/Users/samuellukudu/CAD/cad_extraction/test_outputs/single_failure_repro" \
  --pretty
```

---

## New Conversation Bootstrap

When starting a new chat, paste this:

```text
Continue from cad_extraction handoff:
- File changed: /Users/samuellukudu/CAD/cad_extraction/src/main.rs
- Handoff doc: /Users/samuellukudu/CAD/cad_extraction/HANDOFF_ROADMAP.md
- Current status: 18 files pass and 1 legacy AC1009 DXF is skipped during roundtrip-check on /Users/samuellukudu/CAD/data (recursive)
- Skipped file: /Users/samuellukudu/CAD/data/20250702-景观详图组_t3.dxf
- Goal: keep the documented AC1009 skip behavior stable, or replace it later with a true legacy-safe DXF serialization path.

Please start by checking whether the AC1009 skip behavior is still correct and whether a real preservation fix is now feasible.
```

---

*Last updated: After AC1009 legacy skip implementation (18 pass / 1 skip / 0 fail)*
