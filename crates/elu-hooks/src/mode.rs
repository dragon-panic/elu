use crate::error::HookError;

/// Parsed mode specification — either octal or symbolic.
#[derive(Debug, Clone)]
pub struct ModeSpec {
    kind: ModeKind,
}

#[derive(Debug, Clone)]
enum ModeKind {
    Octal(u32),
    Symbolic(Vec<SymbolicClause>),
}

#[derive(Debug, Clone)]
struct SymbolicClause {
    who: u32,    // bitmask: 0o100 = user, 0o010 = group, 0o001 = other
    op: SymOp,
    bits: u32,   // rwx bits (0-7)
}

#[derive(Debug, Clone, Copy)]
enum SymOp {
    Add,
    Remove,
    Set,
}

impl ModeSpec {
    pub fn parse(s: &str) -> Result<Self, HookError> {
        let s = s.trim();
        if s.is_empty() {
            return Err(HookError::InvalidMode(s.to_string()));
        }
        // Octal: starts with 0 and all digits are octal
        if s.starts_with('0') && s.len() > 1 && s[1..].bytes().all(|b| b.is_ascii_digit()) {
            let val = u32::from_str_radix(&s[1..], 8)
                .map_err(|_| HookError::InvalidMode(s.to_string()))?;
            if val > 0o7777 {
                return Err(HookError::InvalidMode(s.to_string()));
            }
            return Ok(ModeSpec { kind: ModeKind::Octal(val) });
        }
        // Also accept plain octal without leading 0 if all digits
        if s.bytes().all(|b| b.is_ascii_digit()) {
            let val = u32::from_str_radix(s, 8)
                .map_err(|_| HookError::InvalidMode(s.to_string()))?;
            if val > 0o7777 {
                return Err(HookError::InvalidMode(s.to_string()));
            }
            return Ok(ModeSpec { kind: ModeKind::Octal(val) });
        }
        // Symbolic: [ugoa]*[+-=][rwx]+[,...]
        let mut clauses = Vec::new();
        for part in s.split(',') {
            let part = part.trim();
            if part.is_empty() {
                return Err(HookError::InvalidMode(s.to_string()));
            }
            clauses.push(parse_symbolic_clause(part, s)?);
        }
        Ok(ModeSpec { kind: ModeKind::Symbolic(clauses) })
    }

    /// Apply this mode to the current permission bits.
    /// For octal modes, replaces the lower 12 bits entirely.
    /// For symbolic modes, modifies the relevant bits.
    pub fn apply(&self, current: u32) -> u32 {
        match &self.kind {
            ModeKind::Octal(val) => (current & !0o7777) | val,
            ModeKind::Symbolic(clauses) => {
                let mut mode = current;
                for clause in clauses {
                    let shift_bits = |who_bit: u32, perm: u32| -> u32 {
                        match who_bit {
                            0o100 => perm << 6,
                            0o010 => perm << 3,
                            0o001 => perm,
                            _ => 0,
                        }
                    };
                    for who_bit in [0o100u32, 0o010, 0o001] {
                        if clause.who & who_bit != 0 {
                            let shifted = shift_bits(who_bit, clause.bits);
                            match clause.op {
                                SymOp::Add => mode |= shifted,
                                SymOp::Remove => mode &= !shifted,
                                SymOp::Set => {
                                    let mask = shift_bits(who_bit, 0o7);
                                    mode = (mode & !mask) | shifted;
                                }
                            }
                        }
                    }
                }
                mode
            }
        }
    }
}

fn parse_symbolic_clause(part: &str, full: &str) -> Result<SymbolicClause, HookError> {
    let bytes = part.as_bytes();
    let mut i = 0;

    // Parse who
    let mut who = 0u32;
    while i < bytes.len() {
        match bytes[i] {
            b'u' => who |= 0o100,
            b'g' => who |= 0o010,
            b'o' => who |= 0o001,
            b'a' => who |= 0o111,
            _ => break,
        }
        i += 1;
    }
    // Default to 'a' if no who specified
    if who == 0 {
        who = 0o111;
    }

    // Parse op
    if i >= bytes.len() {
        return Err(HookError::InvalidMode(full.to_string()));
    }
    let op = match bytes[i] {
        b'+' => SymOp::Add,
        b'-' => SymOp::Remove,
        b'=' => SymOp::Set,
        _ => return Err(HookError::InvalidMode(full.to_string())),
    };
    i += 1;

    // Parse perms
    let mut bits = 0u32;
    while i < bytes.len() {
        match bytes[i] {
            b'r' => bits |= 0o4,
            b'w' => bits |= 0o2,
            b'x' => bits |= 0o1,
            _ => return Err(HookError::InvalidMode(full.to_string())),
        }
        i += 1;
    }

    Ok(SymbolicClause { who, op, bits })
}
