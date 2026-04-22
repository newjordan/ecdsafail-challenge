# HMR sequence fix findings

A direct HMR diagnostic showed that the specialized bulk-prefix step originally
emitted a different HMR sequence from the generic step-0 circuit:
- same classical state evolution,
- but fewer HMRs and a different operand order.

A targeted fix was then applied to `kaliski_iteration_bulk_prefix3` to restore
matching HMR **count** for step 0.

## Immediate effect
This eliminated the strict phase bug for the small failing case `k = 4`:
- before fix: `phase-garbage batches = 1`, `classical mismatches = 0`
- after fix:  `phase-garbage batches = 0`, `classical mismatches = 0`

So the HMR mismatch was a real part of the phase problem.

## New frontier after the fix
After this change, the strict harness behavior moved:

Passing examples:
- `k = 4, 16, 72, 80, 112`

Failing examples:
- `k = 8` (classical mismatch)
- `k = 24, 32, 40, 64` (phase)
- `k = 96` (classical mismatch)
- `k = 128` (phase + classical)

## Interpretation
This is a real phase-bug fix, but not the final one:
- matching the HMR count was enough to repair at least one strict phase failure,
- but because the operand sequence still differs, and because later `k` values
  still fail, there is at least one more incompatibility left.

So the current bug is now more specific:
- the original phase bug did include an HMR-history mismatch,
- fixing that improved the frontier,
- the remaining failures are now a mix of residual phase incompatibility and
  classical mismatch at larger `k`.
