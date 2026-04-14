//! Circuit builder: thin wrapper that accumulates zenodo `Op`s into a Vec
//! and manages qubit/bit/register allocation with a free-qubit pool.
//!
//! Stable harness — not edited by the research loop. The editable
//! `point_add.rs` uses this API to emit gates.

use crate::circuit::{BitId, Op, OperationType, QubitId, RegisterId};

pub struct Builder {
    pub ops: Vec<Op>,
    next_qubit: u32,
    next_bit: u32,
    next_register: u32,
    free_qubits: Vec<u32>,
    peak_live_qubits: u32,
    live_qubits: u32,
}

impl Builder {
    pub fn new() -> Self {
        Self {
            ops: Vec::new(),
            next_qubit: 0,
            next_bit: 0,
            next_register: 0,
            free_qubits: Vec::new(),
            peak_live_qubits: 0,
            live_qubits: 0,
        }
    }

    fn push(&mut self, op: Op) { self.ops.push(op); }

    pub fn alloc_qubit(&mut self) -> QubitId {
        let id = if let Some(q) = self.free_qubits.pop() {
            QubitId(q)
        } else {
            let q = self.next_qubit;
            self.next_qubit += 1;
            QubitId(q)
        };
        self.live_qubits += 1;
        if self.live_qubits > self.peak_live_qubits {
            self.peak_live_qubits = self.live_qubits;
        }
        id
    }

    pub fn alloc_qubits(&mut self, n: usize) -> Vec<QubitId> {
        (0..n).map(|_| self.alloc_qubit()).collect()
    }

    pub fn alloc_bit(&mut self) -> BitId {
        let b = self.next_bit;
        self.next_bit += 1;
        BitId(b)
    }

    pub fn alloc_bits(&mut self, n: usize) -> Vec<BitId> {
        (0..n).map(|_| self.alloc_bit()).collect()
    }

    /// Assert a qubit is back to |0>, then return it to the pool.
    ///
    /// BANNED: using this on a qubit that is NOT already |0> (across all
    /// 64 shots). The harness enforces this via `strict_apply` in main.rs
    /// AND a forward∘reverse identity check. A "dirty free" — relying on
    /// the simulator's R gate to unconditionally zero — is a correctness
    /// bug, not an optimization. It destroys reversibility and would
    /// entangle ancillas with input superpositions under real execution,
    /// breaking Shor's algorithm. You MUST emit a proper inverse gate
    /// sequence to uncompute the ancilla back to |0> before calling this.
    pub fn assert_zero_and_free(&mut self, q: QubitId) {
        self.r(q);
        self.free_qubits.push(q.0);
        self.live_qubits -= 1;
    }

    pub fn assert_zero_and_free_vec(&mut self, qs: &[QubitId]) {
        for &q in qs { self.assert_zero_and_free(q); }
    }

    pub fn declare_qubit_register(&mut self, qs: &[QubitId]) -> RegisterId {
        let r = RegisterId(self.next_register);
        self.next_register += 1;
        for &q in qs {
            let mut op = Op::empty();
            op.kind = OperationType::AppendToRegister;
            op.q_target = q;
            op.r_target = r;
            self.push(op);
        }
        let mut op = Op::empty();
        op.kind = OperationType::Register;
        op.r_target = r;
        self.push(op);
        r
    }

    pub fn declare_bit_register(&mut self, bs: &[BitId]) -> RegisterId {
        let r = RegisterId(self.next_register);
        self.next_register += 1;
        for &b in bs {
            let mut op = Op::empty();
            op.kind = OperationType::AppendToRegister;
            op.c_target = b;
            op.r_target = r;
            self.push(op);
        }
        let mut op = Op::empty();
        op.kind = OperationType::Register;
        op.r_target = r;
        self.push(op);
        r
    }

    pub fn comment(&mut self, _s: &str) { /* no-op; Op IR has no comment kind */ }

    // ── Gate emitters ──────────────────────────────────────────────────────

    pub fn x(&mut self, q: QubitId) {
        let mut op = Op::empty();
        op.kind = OperationType::X;
        op.q_target = q;
        self.push(op);
    }

    pub fn z(&mut self, q: QubitId) {
        let mut op = Op::empty();
        op.kind = OperationType::Z;
        op.q_target = q;
        self.push(op);
    }

    pub fn cx(&mut self, ctrl: QubitId, tgt: QubitId) {
        let mut op = Op::empty();
        op.kind = OperationType::CX;
        op.q_control1 = ctrl;
        op.q_target = tgt;
        self.push(op);
    }

    pub fn cz(&mut self, a: QubitId, b: QubitId) {
        let mut op = Op::empty();
        op.kind = OperationType::CZ;
        op.q_control1 = a;
        op.q_target = b;
        self.push(op);
    }

    pub fn ccx(&mut self, c1: QubitId, c2: QubitId, tgt: QubitId) {
        let mut op = Op::empty();
        op.kind = OperationType::CCX;
        op.q_control2 = c1;
        op.q_control1 = c2;
        op.q_target = tgt;
        self.push(op);
    }

    pub fn ccz(&mut self, c1: QubitId, c2: QubitId, tgt: QubitId) {
        let mut op = Op::empty();
        op.kind = OperationType::CCZ;
        op.q_control2 = c1;
        op.q_control1 = c2;
        op.q_target = tgt;
        self.push(op);
    }

    pub fn swap(&mut self, a: QubitId, b: QubitId) {
        let mut op = Op::empty();
        op.kind = OperationType::Swap;
        op.q_control1 = a;
        op.q_target = b;
        self.push(op);
    }

    pub fn r(&mut self, q: QubitId) {
        let mut op = Op::empty();
        op.kind = OperationType::R;
        op.q_target = q;
        self.push(op);
    }

    pub fn x_if(&mut self, q: QubitId, cond: BitId) {
        let mut op = Op::empty();
        op.kind = OperationType::X;
        op.q_target = q;
        op.c_condition = cond;
        self.push(op);
    }

    pub fn cx_if(&mut self, ctrl: QubitId, tgt: QubitId, cond: BitId) {
        let mut op = Op::empty();
        op.kind = OperationType::CX;
        op.q_control1 = ctrl;
        op.q_target = tgt;
        op.c_condition = cond;
        self.push(op);
    }

    pub fn ccx_if(&mut self, c1: QubitId, c2: QubitId, tgt: QubitId, cond: BitId) {
        let mut op = Op::empty();
        op.kind = OperationType::CCX;
        op.q_control2 = c1;
        op.q_control1 = c2;
        op.q_target = tgt;
        op.c_condition = cond;
        self.push(op);
    }

    pub fn push_condition(&mut self, cond: BitId) {
        let mut op = Op::empty();
        op.kind = OperationType::PushCondition;
        op.c_condition = cond;
        self.push(op);
    }

    pub fn pop_condition(&mut self) {
        let mut op = Op::empty();
        op.kind = OperationType::PopCondition;
        self.push(op);
    }

    pub fn peak_qubits(&self) -> u32 { self.peak_live_qubits }
    pub fn total_qubits(&self) -> u32 { self.next_qubit }
}

/// Register layout returned by `point_add::build`. Indices match the
/// zenodo `program` interface: 0=target_x, 1=target_y, 2=offset_x, 3=offset_y.
pub struct Layout {
    pub target_x: RegisterId,
    pub target_y: RegisterId,
    pub offset_x: RegisterId,
    pub offset_y: RegisterId,
}
