# Autoresearch: quantum_ecc secp256k1 point-add Toffoli reduction

## Objective
Reduce the **average executed Toffoli count** of the reversible secp256k1 affine point-add circuit built in `src/point_add.rs`, while preserving harness correctness and keeping qubits within the current regime.

Committed best on `autoresearch/2026-04-20`:
- avg_toffoli: 4,672,931 (Karatsuba 1-level everywhere + 2-level at between-pair site)
- avg_clifford: 24,152,002
- qubits: 3507
- emitted_ops: 34,863,147

Target (Google paper): 2.1M Toffoli @ 1425 qubits (low-gate) or 2.7M Toffoli @ 1175 qubits (low-qubit).

## Metrics
- **Primary**: `avg_toffoli` (lower is better)
- **Secondary**: `avg_clifford`, `qubits`, `emitted_ops`, `correctness_ok`.

## How to Run
`./autoresearch.sh`. Writes `METRIC ...=...` lines.

## Files in Scope
- `src/point_add.rs` — the only project source file allowed to change.
- `autoresearch.md`, `autoresearch.sh`, `autoresearch.checks.sh`, `autoresearch.ideas.md`, `autoresearch.note`.

## Off Limits
- Everything except `src/point_add.rs` and the autoresearch session files.
- No new dependencies.

## Constraints
- `cargo run --release` must finish with `=== experiment OK ===` on both the benchmark run and the checks rerun.
- Peak qubits ≤ 3700 hard cap (program.md).
- `cargo build --release` must succeed.

## Known Cost Breakdown (at current best ~4.67M)

Rough per-subroutine cost budget (approx):
- 2× Kaliski (400 iters each, ~12n CCX/iter) ≈ 2.4M
- 4 muls × ~66k (Karatsuba 1-level) ≈ 265k
- 2 squarings × ~130k ≈ 260k
- Scale correction loops (400 halvings + 400 doublings, ~512 CCX each) ≈ 410k
- Solinas reductions in mul output (5 Cuccaros per mul) ≈ 500k
- Misc (constant add/sub qb, neg, cswaps, step-4 overhead) ≈ 800k

The two biggest levers left:
1. **Kaliski dominates at ~50% of total.** Eliminating one Kaliski pass or halving cost-per-iter is the biggest possible lever.
2. **Scale-correction loops (halvings/doublings) cost ~410k.** Eliminating them alone gives a ~10% Toffoli reduction.

## Paper Research Summary

Read (in `/tmp/`): Litinski 2023 (50M Toffoli single-point-add), HRSL 2020 (Improved ECDLP circuits), RNSL 2017 (original Microsoft estimates), Gidney 2019 (Windowed quantum arithmetic), Ragavan-Gidney 2025 (Optimized windowed mod arithmetic), Google/Babbush 2026 (the "Google paper").

Core findings:
- Google's 2.1M/2.7M is for **single point addition**, same as our benchmark. No batching, no state reuse — those are *additional* factor-of-5 gains on top of the single-point-add cost.
- Kickmix gate set = our gate set. MBUC = measurement-based uncomputation; we already use it throughout.
- Litinski: breaks the single-point-add into ~60 fundamental subroutines; Montgomery-mul step is 9n+28 Toffoli per step with n/4 steps per full mul ⇒ ~148k Toffoli per mul. Our Karatsuba 1-level is competitive (~66k per mul).
- HRSL: swap-based Kaliski round (Figure 6b) — one sub + one add + one halve + one double per iter with swaps selecting register roles. Our current Kaliski STEP 4 already does one cond-sub + one cond-add per iter; the structural savings must be elsewhere.
- HRSL uses Montgomery mul + windowed add-by-p (lookup of t*p where t is small-bit address). Helps when p is dense. For secp256k1 (Solinas p = 2^256 - 2^32 - 977), our sparse-c Solinas is already cheap.
- Windowed quantum arithmetic (Gidney 2019) helps quantum × classical multiplication, not quantum × quantum. In our flow, the Kaliski scale-correction (multiply lam by 2^{-(2n-K)} mod p) is quantum × classical and *could* be windowed.

## Most Promising Structural Paths

1. **Montgomery batched inversion (replace 2 Kaliski with 1)**
   - Algebra: `N = dy² - (Px+2Qx)·dx²`; invert `c = dx·N` once; recover `dx⁻¹ = c⁻¹·N`, `(Rx-Qx)⁻¹ = c⁻¹·dx³`
   - Gross save: 1 Kaliski pass ≈ 1.2M Toffoli
   - Added cost: ~3-5 extra muls + 1 squaring ≈ 300-500k Toffoli
   - Net expected: 700k-900k save
   - Historical scaffolding: commit 333a3c1 (`compute_montgomery_n`, etc.) — validated algebraically in isolation at 64/64 shots.
   - Risk: full round-trip may nearly break even; seed-tax may bite; qubit footprint may exceed 3700.

2. **Windowed classical-constant multiplication to replace halving/doubling loops**
   - Replace the 400-iteration halving loop (≈204k Toffoli) with one windowed classical-const mul (estimated 20-40k). 800-ish halvings/doublings total ⇒ ~400k → ~80k, saving ~300-400k.
   - Needs a new windowed-const-mul primitive; doable, localized change.

3. **Eliminate the scale correction entirely by changing the algebra**
   - Pair1: `lam = ty·inv_raw` leaves `lam = λ·2^(2n-K)`. Pair1's mul2 currently needs unscaled `lam`. Restructure so mul2 operates on the scaled `lam` and compensates on `tx` side via a classical scalar multiply.
   - Net: saves both halving AND doubling loops (~400k Toffoli), at the cost of two extra classical-const muls (~40-80k windowed). Net save ~300-350k.

4. **Swap Kaliski for Bernstein-Yang safegcd / divstep**
   - B-Y produces `x⁻¹` directly (no `2^k` factor), eliminating scale correction. Historical analysis said "+45% cost" in this codebase; may not apply after windowed const-mul.
   - Larger-scope rewrite.

5. **MBUC-compress `m_hist` to classical bits**
   - Save ~400 qubits (per pass), 0 Toffoli.
   - Makes room in the qubit budget to try more aggressive 2-level Karatsuba or other memory-hungry wins.

## Plan order

Tackle **path 2 first** (windowed-classical-const-mul to kill halving loops): localized, concrete 300-400k Toffoli target. If successful, combine with **path 1** (Montgomery batched inversion) for a potential ~1M cumulative saving. Then evaluate path 5 for qubit reductions that unlock path-3/path-1 memory headroom.

## Determinism diagnosis

Verified `build()` produces identical op counts and Toffoli counts across consecutive binary invocations. Only `avg_clifford` varies slightly — caused by per-run randomness in HMR (measurement-based uncompute) phase feedback, which feeds back through the XOF to different `executed_shots` for Clifford stats. Zero implication for correctness or the primary metric.

Profiling experiment (pair 2 disabled): ops drop from 34,863,147 → 19,519,706, i.e. **pair 2 contributes ~15.3M ops (44% of the circuit)**. Projected Toffoli saving from eliminating one Kaliski pass: **~2M Toffoli**, exactly the Google gap.

## Structural-change attempts (this session)

| Attempt | Result | Why |
|---|---|---|
| 1-level Karatsuba (all 4 muls) | -247k ✓ kept | Established baseline of add-subtract value |
| 2-level Karatsuba (all 4 muls) | qubits 3765 > 3700 | Reverted; z1_inner registers too expensive |
| 2-level Karatsuba @ between-pair only | -8k ✓ kept | Tiny win at non-Kaliski site |
| Litinski add-subtract schoolbook inside Karatsuba | -334k ✓ kept | Biggest single-change win; half the per-row Toffoli |
| Litinski addsub in 2-level middle mul | -32k ✓ kept | Mild additional gain |
| 2-level Karatsuba at all 4 sites | checks_failed | Qubit cap + seed tax at tighter Kaliski iters |
| Kaliski STEP 4 MBUC of add_f to classical bit | crash | Misuse of HMR semantics (bit is randomized, not a deterministic copy) |
| Full Montgomery batched inversion (single Kaliski on c=dx·N) | crash | Peak 5662 qubits, classical mismatch; approach needs m_hist compression + careful debugging |

## Why the remaining 2M Toffoli is hard

- Eliminating one Kaliski saves ~2M Toffoli. But adding the Montgomery batched identity adds ~4–6 extra multiplications and 2 extra squarings, plus preserving dx_copy and dy_copy across the closure costs 512 qubits. Combined with the Kaliski state's m_hist (400 qubits) it hits ~5.6k qubits, way past the 3700 cap.
- Google's 2.1M target likely depends on:
  - Measurement-based compression of `m_hist` into classical bits (saves ~400 qubits, no Toffoli)
  - Full Montgomery representation throughout (saves Solinas reduction per mul)
  - Windowed constant multiplication (replaces halving/doubling loops with one const-mul ~15k)
  - The Montgomery batched single-Kaliski identity (saves 1 Kaliski pass ~1M)
  - Aggressive register-reuse / pebbling (m_hist → bits; temporary scratch reused across Kaliski iters)
- Individually each is a moderate change; combined they likely reach 2.1M. None of them fit cleanly into a single-session edit.

## What's Been Tried (this session)

- 1-level Karatsuba on all 4 mod_muls — **-246,760 Toffoli** ✓
- 2-level Karatsuba everywhere — qubits 3765 > 3700 cap; reverted.
- 2-level only at the non-Kaliski-peak mul site, Kaliski iters restored to 400/400 — **-7,993 Toffoli** ✓
- Kaliski iter probing: 397/397 fails; 398/397 OK; 397/398 fails.
- Karatsuba seed tax: changing the op stream shifts the Fiat-Shamir seed and forces Kaliski iters higher; tuning per-pair iters is mandatory.
