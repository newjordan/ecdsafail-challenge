//! Experiment harness: builds the point-addition circuit defined in
//! `point_add.rs`, runs it against the zenodo `Simulator` with random
//! test shots, and reports Toffoli / Clifford / qubit counts.
//!
//! Research-loop contract: ONLY `point_add.rs` is edited by the loop.
//! This file, `builder.rs`, `circuit.rs`, `sim.rs`, and
//! `weierstrass_elliptic_curve.rs` are harness and must not be touched.
//!
//! Attribution: `circuit.rs`, `sim.rs`, and `weierstrass_elliptic_curve.rs`
//! are reused verbatim from the `zkp_ecc` Zenodo project under CC BY 4.0.
//! See `NOTICE` at the repository root for details.

#[allow(dead_code)]
mod circuit;
#[allow(dead_code)]
mod sim;
#[allow(dead_code)]
mod weierstrass_elliptic_curve;
#[allow(dead_code)]
mod builder;
mod point_add;

use alloy_primitives::U256;
use builder::{Builder, Layout};
use circuit::{Op, QubitOrBit, analyze_ops};
use sha3::{digest::{ExtendableOutput, Update, XofReader}, Shake256};
use sim::Simulator;
use weierstrass_elliptic_curve::WeierstrassEllipticCurve;

// ─── secp256k1 parameters ──────────────────────────────────────────────────

fn secp256k1() -> WeierstrassEllipticCurve {
    WeierstrassEllipticCurve {
        modulus: U256::from_str_radix("FFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFEFFFFFC2F", 16).unwrap(),
        a: U256::from(0),
        b: U256::from(7),
        gx: U256::from_str_radix("79BE667EF9DCBBAC55A06295CE870B07029BFCDB2DCE28D959F2815B16F81798", 16).unwrap(),
        gy: U256::from_str_radix("483ADA7726A3C4655DA4FBFC0E1108A8FD17B448A68554199C47D08FFB10D4B8", 16).unwrap(),
        order: U256::from_str_radix("FFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFEBAAEDCE6AF48A03BBFD25E8CD0364141", 16).unwrap(),
    }
}

// ─── Test runner ───────────────────────────────────────────────────────────

const NUM_TESTS: usize = 64;

fn run_tests(ops: &[Op], layout_regs: &[Vec<QubitOrBit>], total_qubits: u32, num_bits: u32)
    -> (bool, u64, u64)
{
    let curve = secp256k1();
    let mut hasher = Shake256::default();
    hasher.update(b"quantum_ecc-baseline-seed-v1");
    let mut xof = hasher.finalize_xof();

    // Generate random target/offset points as k*G.
    let mut targets = Vec::with_capacity(NUM_TESTS);
    let mut offsets = Vec::with_capacity(NUM_TESTS);
    let mut expected = Vec::with_capacity(NUM_TESTS);
    for _ in 0..NUM_TESTS {
        let mut rb = [[0u8; 32]; 2];
        xof.read(&mut rb[0]);
        xof.read(&mut rb[1]);
        let k1 = U256::from_le_bytes(rb[0]);
        let k2 = U256::from_le_bytes(rb[1]);
        let t = curve.mul(curve.gx, curve.gy, k1);
        let o = curve.mul(curve.gx, curve.gy, k2);
        // Avoid the doubling / inverse-pair cases the baseline doesn't handle.
        if t.0 == o.0 { continue; }
        if t.0.is_zero() && t.1.is_zero() { continue; }
        if o.0.is_zero() && o.1.is_zero() { continue; }
        let e = curve.add(t.0, t.1, o.0, o.1);
        targets.push(t);
        offsets.push(o);
        expected.push(e);
    }
    let n = targets.len();

    let mut rng_hasher = Shake256::default();
    rng_hasher.update(b"quantum_ecc-sim-rng-v1");
    let mut rng = rng_hasher.finalize_xof();

    let mut sim = Simulator::new(total_qubits as usize, num_bits as usize, &mut rng);
    let mut ok = true;

    let mut got = vec![(U256::ZERO, U256::ZERO); n];

    const BATCH: usize = 64;
    let num_batches = (n + BATCH - 1) / BATCH;
    for batch in 0..num_batches {
        let bs = BATCH.min(n - batch * BATCH);
        sim.clear_for_shot();
        for shot in 0..bs {
            let i = batch * BATCH + shot;
            sim.set_register(&layout_regs[0], targets[i].0, shot);
            sim.set_register(&layout_regs[1], targets[i].1, shot);
            sim.set_register(&layout_regs[2], offsets[i].0, shot);
            sim.set_register(&layout_regs[3], offsets[i].1, shot);
        }
        sim.apply(ops);
        for shot in 0..bs {
            let i = batch * BATCH + shot;
            let gx = sim.get_register(&layout_regs[0], shot);
            let gy = sim.get_register(&layout_regs[1], shot);
            got[i] = (gx, gy);
            if gx != expected[i].0 || gy != expected[i].1 {
                ok = false;
            }
        }
    }

    println!("  test points:");
    for i in 0..n {
        let mark = if got[i] == expected[i] { "OK  " } else { "FAIL" };
        println!("    [{i:02}] {mark}");
        println!("         T   =({:#x}, {:#x})", targets[i].0, targets[i].1);
        println!("         O   =({:#x}, {:#x})", offsets[i].0, offsets[i].1);
        println!("         got =({:#x}, {:#x})", got[i].0, got[i].1);
        println!("         exp =({:#x}, {:#x})", expected[i].0, expected[i].1);
    }

    let avg_cliff = sim.stats.clifford_gates / n.max(1) as u64;
    let avg_tof = sim.stats.toffoli_gates / n.max(1) as u64;
    (ok, avg_cliff, avg_tof)
}

fn main() {
    println!("=== quantum_ecc: secp256k1 point addition baseline ===\n");
    let curve = secp256k1();

    println!("-- building circuit --");
    let mut builder = Builder::new();
    let layout = point_add::build(&mut builder);
    let ops = builder.ops.clone();
    let peak = builder.peak_qubits();
    let total_alloc = builder.total_qubits();

    let (total_qubits, num_bits, _num_regs, regs) = analyze_ops(ops.iter().copied());

    // Sanity-check layout matches zenodo's program interface.
    assert!(regs.len() == 4, "expected 4 registers (target_x, target_y, offset_x, offset_y); got {}", regs.len());
    for (i, r) in regs.iter().enumerate() {
        assert_eq!(r.len(), 256, "register {i} should be 256 wide, got {}", r.len());
    }
    for q in &regs[0] { assert!(matches!(q, QubitOrBit::Qubit(_)), "register 0 must be qubits"); }
    for q in &regs[1] { assert!(matches!(q, QubitOrBit::Qubit(_)), "register 1 must be qubits"); }
    for q in &regs[2] { assert!(matches!(q, QubitOrBit::Bit(_)),   "register 2 must be bits"); }
    for q in &regs[3] { assert!(matches!(q, QubitOrBit::Bit(_)),   "register 3 must be bits"); }
    let _ = layout; // layout IDs are implicit in ordering

    println!("  total ops : {}", ops.len());
    println!("  qubits    : {} (peak live {}, total alloc {})", total_qubits, peak, total_alloc);
    println!("  bits      : {}", num_bits);

    println!("\n-- running correctness tests --");
    let (ok, avg_cliff, avg_tof) = run_tests(&ops, &regs, total_qubits, num_bits);
    if !ok {
        println!("\n!! correctness FAILED");
        std::process::exit(1);
    }
    println!("  all {} shots OK", NUM_TESTS);

    println!("\n=== circuit metrics (secp256k1, n=256) ===");
    println!("  Toffoli (CCX/CCZ) : {}", avg_tof);
    println!("  Clifford          : {}", avg_cliff);
    println!("  Total ops         : {}", ops.len());
    println!("  Qubits            : {}", total_qubits);

    println!("\n=== experiment OK ===");
}
