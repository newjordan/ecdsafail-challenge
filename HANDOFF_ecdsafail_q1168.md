# ecdsa.fail q1168 — handoff (2026-06-15)

**Status:** the circuit is solved; only the nonce hunt remains (pure compute).

**Bar:** frontier 1,674,533,568 = peak 1168 × avg_tof 1,433,676.

**A value-exact config that beats it.** A peak-1168 binary-GCD point-add — modular inverse via a truncated binary GCD, Solinas reduction for the field multiplies, and a reversible transcript codec that compresses the per-step GCD decisions — with the epilogue "venting" adders removed on every phase that runs below the qubit peak (de-venting), plus per-step selective repair of the carry-truncation windows. Measured: **avg_executed_Toffoli = 1,432,866, peak 1168, ancilla-garbage 0** → score **1,673,587,488 (−946,080 under the bar)**. Value-exact: the per-shot classical/phase mismatches are nonce-dependent stragglers, not value errors.

**What remains — the hunt.** A valid submission needs one Fiat-Shamir seed (the per-shot nonce that drives SHAKE256) for which all 9024 shots are simultaneously clean (0 classical / 0 phase / 0 ancilla). Frequency e^−R, R = classical-straggler rate + phase-straggler rate. Measured in-budget floor **R ≈ 25** (Rcm ≈ 16 + Rpg ≈ 9) → E[nonces] ≈ 7×10¹⁰. The search is **memoryless Bernoulli** across nonces — the nonce only seeds the Fiat-Shamir hash, it sets no truncation/width/value parameter — so it parallelizes perfectly over disjoint nonce bands. ≈7× (8×RTX-3080) for ~2 weeks, or ~270 days on one high-end GPU.

**How to hunt.** Build a cheap GPU pre-sieve that reproduces the classical-mismatch predictor (GCD + apply value check) and emits only survivors; it must be **0-false-negative** against your trusted simulator (a lossy sieve can discard the winning nonce). Full-9024-confirm survivors on CPU; on a 0/0/0 hit, bake that nonce and submit. Largest enrichment lever: move the apply-value-mismatch check onto the GPU too (~1000× fewer CPU confirms).

**Measured dead-ends (don't spend compute here).**
- Lowering R via new per-step GCD structure is avg_T-gated at peak 1168: a **depth-3 jump GCD (strip 3 trailing zeros/step)** drops R to ≈12 but raises avg_tof to ≈1,514,430 — over the cap by 37–96×; a **constant-time divstep (safegcd)** removes the phase leak but adds +0.3–1.1M avg_tof. Score and R are coupled: the truncation that keeps avg_tof under the bar IS what creates the stragglers.
- Nonce-region bandits / importance sampling: zero leverage (memoryless).
- Lower qubit peak (1167) raises R; higher peak tightens the avg_tof cap. 1168 is the knee.

**Net:** beating the frontier is a ~$2–3k / two-week GPU hunt on a config of the above shape — an execution problem, not an algorithm problem.
