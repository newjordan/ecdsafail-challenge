# Ground-up architecture under ~600 scratch qubits

User framing: Google low-qubit means roughly **600-660 non-data qubits** beyond
`tx,ty` (512 quantum input/output qubits). This document ignores local tuning
and asks what can possibly fit.

## 1. Budget arithmetic

At `n=256`:

```text
data registers: tx, ty = 512q
Google low-qubit total: 1175q
scratch beyond tx,ty: 663q
user mental model: ~600q
```

So a viable low-qubit point-add can have at most:

- two full n-bit scratch registers (`2n = 512`) plus ~90-150 small bits, or
- one full n-bit scratch register plus a compact inversion state, or
- heavy reuse of `tx,ty` as algorithmic work registers.

It **cannot** have three extra n-bit registers, and it definitely cannot have
current Kaliski's `u,v,r,s,m_hist` state.

Current peak has, beyond `tx,ty`:

| live object | qubits |
|---|---:|
| slope `lam` | 256 |
| Kaliski `u,v,r,s` | 1024 |
| `m_hist` | 403-407 |
| transients | 250-520 |
| **non-data total** | **~2200** |

We need to remove roughly **1500 non-data qubits**, not 50.

## 2. Consequence: treat data registers as part of the algorithm

A 600-scratch design cannot say "keep `tx,ty` pristine, compute everything in
ancilla, swap outputs, then uncompute". That Bennett pattern leaves old
`Px,Py` or `dx,dy` in fresh registers and immediately exceeds the budget.

The only plausible low-qubit pattern is:

1. Mutate `tx,ty` into useful intermediates (`dx,dy`, coefficient registers,
   accumulators).
2. Use at most two additional n-bit work registers.
3. Arrange the final inverse/cleanup so that running a reverse transform writes
   the desired output into `tx,ty` rather than restoring the input.

This is the right abstraction: **we need a reversible data transform, not a
Bennett-clean subroutine call.**

## 3. Inversion-state lower bound

Any Euclidean inverse needs, in some representation:

- a denominator state (`u/v` or equivalent), and
- coefficient information connecting the denominator to the inverse.

Current Kaliski stores this as `(u,v,r,s)` plus history. In 600 scratch, the
only way to keep Kaliski-like inversion alive is to fold at least two of those
four n-bit roles into `tx,ty`.

A minimal Kaliski-like layout would have to look like:

| role | storage |
|---|---|
| denominator input `v=dx` | `tx` or one scratch copy |
| other gcd register `u` | scratch A |
| coefficient/output register | scratch B or `ty` |
| second coefficient register | `ty` or eliminated |
| history | not stored, or <=~100 bits |

This is exactly why `m_hist` elimination alone is insufficient: even without
history, the four n-bit Kaliski roles already exceed 600 scratch unless they
are folded into the data registers.

## 4. New structural idea: use Kaliski's coefficient transform on `ty`

Instead of treating Kaliski as an ancilla subroutine, seed its coefficient
register with the data value `dy`.

Use a canonical-mod-p coefficient version of Kaliski. For a fixed denominator
`dx`, the coefficient-side update is a linear transform:

```text
(r_final, s_final)^T = T(dx) (r_initial, s_initial)^T
```

The test module `kaliski_linear_transform.rs` verifies empirically for the
current 407-iteration branch sequence that:

```text
T(dx) = [[ a(dx), k(dx) ],
         [ dx,    0     ]]

k(dx) * dx = -2^407  (mod p)
```

Therefore:

```text
T(dx) * (0, 1)  = (k, 0)          raw inverse
T(dx) * (0, dy) = (k*dy, 0)       scaled slope, ty consumed to zero
T(dx) * (1, 0)  = (a(dx), dx)     exposes dx in the second coefficient
```

This is the first genuinely low-qubit-looking Kaliski algebra found in this
repo: `ty` can be consumed into the coefficient transform instead of being kept
as an external data register plus a separate multiplication `dy * inv(dx)`.

### Why this matters

If we could finish the point-add while the coefficient transform is live, then
run Kaliski backward, `ty` could be written to an arbitrary target value.
Specifically, to finish backward with:

```text
r_initial = 0
s_initial = Ry
```

the state *before* backward must be:

```text
T(dx) * (0, Ry) = (k*Ry, 0)
```

But the dy-seeded forward naturally gives:

```text
(k*dy, 0)
```

So the exact structural subproblem is:

```text
add  k * (Ry - dy)  into r, with s=0, without a second inversion.
```

This is crisp. It replaces the vague "one-inversion cleanup obstruction" with
one algebraic target.

## 5. Current obstruction in the coefficient-transform frame

We know:

```text
k = -2^407 / dx
L = k*dy = scaled(lambda)
Ry = -lambda*(Rx-Qx) - Qy
```

Then:

```text
k*(Ry-dy)
  = -k*lambda*(Rx-Qx) - k*Qy - k*dy
```

The live dy-seeded state gives `L = k*dy`, but not `k` itself. The `k*Qy`
term is the sticking point: multiplying a classical `Qy` by raw `k` requires
access to `k`, i.e. the raw inverse, not just the scaled slope.

This explains why the usual one-inversion schedules leak a slope copy: they
have enough information for `lambda`, but not enough to rewrite the Kaliski
coefficient pair to make backward output `Ry`.

## 6. What would make this a breakthrough?

The coefficient-transform idea becomes a 600-scratch / SOTA route if we can do
one of the following:

1. **Expose both `k` and `k*dy` using the two coefficient registers.**
   Since `T(dx)*(1,0)=(a,dx)` and `T(dx)*(0,1)=(k,0)`, maybe a different
   initialization of `(r,s)` plus the already-live `tx=dx` can recover `k`
   or `k*Qy` without another full inverse.

2. **Choose a different y-coordinate convention so the `k*Qy` term vanishes.**
   Work with shifted `Y` coordinates, e.g. store `Y+Qy` or `Y-Qy`, so that the
   final backward target is `Ry+Qy` instead of `Ry`. If the benchmark output
   can be recovered by a final classical add/sub, this may remove the raw-`k`
   constant term.

3. **Use the `r_initial` channel deliberately.**
   We do not necessarily need backward to end with `r_initial=0`; it could end
   with a known classical constant and then be X-freed. This changes the target
   from `T*(0,Ry)` to `T*(C,Ry) = C*(a,dx)+Ry*(k,0)`, giving an additional
   live `dx*C` in `s_final` and maybe a way to absorb the constant term.

4. **Run a tiny second coefficient transform, not a second full inversion.**
   If only the `k*Qy` term is missing and `Qy` is classical, maybe a
   classical-seeded coefficient pass can be folded into the same branch history
   or a short replay. This would be far cheaper than a full second Kaliski if
   it reuses the branch sequence.

These are structural, not micro. Any one of them could delete the second
inversion and land near 2.5M Toffoli. If all fail, two-inversion SOTA must come
from jumped/windowed Kaliski instead.

## 7. Fast invalidation tasks

1. **Shifted-Y search**: algebraically test targets `Ry`, `Ry+Qy`, `Ry-Qy`,
   `Ry+Py`, etc. Find whether `k*(target - seed)` can be expressed using only
   `L=k*dy`, `dx`, `Rx`, and classical constants without raw `k`.

2. **Two-channel coefficient search**: allow initial `(r0,s0)` to be affine
   functions of `{dy, Qy, 1}` and final `(r1,s1)` to be affine functions of
   `{Ry, Qy, 1}` with `r1` freeable. Symbolically determine if the required
   final pair can be computed from the forward pair using <=2 q×q muls and no
   new inverse.

3. **Cost if successful**:
   - one Kaliski invocation: ~1.60M
   - delete `pair1_mul1`, `pair1_mul2`, second Kaliski: save ~1.7M
   - add coefficient modularity overhead in step4: likely +200-400k
   - add final coefficient rewrite: target <=300k
   - expected total if solved: **2.4-2.8M Toffoli**
   - qubits if folded into `ty` and history compressed: plausibly **1100-1500q**

This is now the main ground-up research direction alongside jumped Kaliski.
