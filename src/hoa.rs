#![allow(missing_docs)]
pub mod input;
pub mod output;

use std::cell::RefCell;

use crate::{prelude::*, Set};
use biodivine_lib_bdd::{
    Bdd, BddPartialValuation, BddSatisfyingValuations, BddValuation, BddVariable,
};
use hoars::{HoaAutomaton, ALPHABET, MAX_APS, VARS};
use itertools::Itertools;

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct HoaString(pub(crate) String);

/// A propositional alphabet, where a symbol is a valuation of all propositional variables.
///
/// # Example
/// Assume we have a propositional alphabet over the atomic propositions `a`, `b` and `c`.
///
/// Then a **symbol** in this alphabet is a valuation of these variables, e.g. `a & !b & c`. This is used to label
/// transitions in a [`crate::transition_system::TransitionSystem`].
///
/// An **expression** on the other hand is used to label edges and it is a boolean expression over
/// the atomic propositions, e.g. `(a | b) & c`. Such an expression is matched by
/// a symbol if the symbol satisfies the expression, i.e. if the expression evaluates to `true` under the given
/// valuation. The expression from above, for example, would be matched by the symbol given above (`a & !b & c`),
/// but not by the symbols `a & b & !c` or `!a & !b & c`.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct HoaAlphabet {
    apnames: Vec<String>,
    expressions: RefCell<Set<HoaExpression>>,
}

impl HoaAlphabet {
    pub fn size(&self) -> usize {
        2u32.saturating_pow(self.apnames.len() as u32)
            .try_into()
            .expect("Cannot fit value into usize")
    }

    pub fn apnames(&self) -> &[String] {
        &self.apnames
    }
    pub fn apnames_len(&self) -> usize {
        self.apnames.len()
    }

    pub fn from_hoa_automaton(aut: &HoaAutomaton) -> Self {
        let apnames = aut.aps().clone();
        assert!(apnames.len() < MAX_APS);
        assert!(!apnames.is_empty());

        Self {
            apnames,
            expressions: RefCell::new(Set::default()),
        }
    }
    pub fn top(&self) -> Bdd {
        ALPHABET.mk_true()
    }
    pub fn bot(&self) -> Bdd {
        ALPHABET.mk_false()
    }
    pub fn var(&self, n: usize) -> Bdd {
        ALPHABET.mk_var(self.nth_variable(n))
    }
    pub fn not_var(&self, n: usize) -> Bdd {
        ALPHABET.mk_not_var(self.nth_variable(n))
    }
    pub fn nth_variable(&self, n: usize) -> BddVariable {
        assert!(n < MAX_APS);
        assert!(n < self.apnames.len());
        VARS[n]
    }
}

pub(crate) fn bdd_valuation_to_hoa_symbol(valuation: &BddValuation) -> HoaSymbol {
    let aps: usize = valuation.num_vars().into();
    assert!(
        aps <= MAX_APS,
        "We have {aps}, which is more than the maximum of {MAX_APS}"
    );
    let mut repr = 0;
    for i in 0..MAX_APS {
        if valuation.value(VARS[i]) {
            repr |= 1 << i;
        }
    }
    HoaSymbol { repr, aps }
}

pub(crate) fn hoa_symbol_to_hoa_expression(symbol: &HoaSymbol) -> HoaExpression {
    let bdd: Bdd = symbol.as_bdd_valuation().into();
    HoaExpression {
        bdd,
        aps: symbol.aps,
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Hash, Copy)]
pub struct HoaSymbol {
    repr: u8,
    aps: usize,
}

impl HoaSymbol {
    fn as_bdd_valuation(&self) -> BddValuation {
        let mut valuation = BddValuation::all_false(MAX_APS.try_into().unwrap());
        for i in 0..self.aps {
            let val = ((1 << i) & self.repr) > 0;
            valuation.set_value(VARS[i], val);
        }
        valuation
    }
    pub fn to_char(&self) -> char {
        assert!(self.aps <= MAX_APS, "Too many APs");
        (('a' as u8) + (self.repr & 0b1111)) as char
    }
    pub fn from_char(value: char, aps: usize) -> Self {
        assert!(aps <= MAX_APS);
        let val = value as u8;
        assert!(('a' as u8) < val);
        assert!(val <= ('p' as u8));

        Self {
            repr: (val - ('a' as u8)) & 0b1111,
            aps,
        }
    }
}

impl PartialOrd for HoaSymbol {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for HoaSymbol {
    fn cmp(&self, _other: &Self) -> std::cmp::Ordering {
        todo!()
    }
}
impl Show for HoaSymbol {
    fn show(&self) -> String {
        (0..self.aps)
            .map(|i| {
                if ((1 << i) & self.repr) > 0 {
                    i.to_string()
                } else {
                    format!("!{i}")
                }
            })
            .join(" & ")
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub struct HoaExpression {
    bdd: Bdd,
    aps: usize,
}

impl HoaExpression {
    pub fn new(bdd: Bdd, aps: usize) -> Self {
        Self { bdd, aps }
    }
}

impl std::ops::BitAnd for HoaExpression {
    type Output = HoaExpression;

    fn bitand(self, rhs: Self) -> Self::Output {
        assert_eq!(self.aps, rhs.aps);
        HoaExpression {
            aps: self.aps,
            bdd: self.bdd.and(&rhs.bdd),
        }
    }
}

impl std::ops::Not for HoaExpression {
    type Output = HoaExpression;

    fn not(self) -> Self::Output {
        HoaExpression {
            aps: self.aps,
            bdd: self.bdd.not(),
        }
    }
}

impl std::ops::BitOr for HoaExpression {
    type Output = HoaExpression;
    fn bitor(self, rhs: Self) -> Self::Output {
        HoaExpression {
            aps: self.aps,
            bdd: self.bdd.or(&rhs.bdd),
        }
    }
}

impl std::ops::BitAndAssign for HoaExpression {
    fn bitand_assign(&mut self, rhs: Self) {
        assert_eq!(self.aps, rhs.aps);
        self.bdd = self.bdd.and(&rhs.bdd);
    }
}

impl std::ops::BitOrAssign for HoaExpression {
    fn bitor_assign(&mut self, rhs: Self) {
        assert_eq!(self.aps, rhs.aps);
        self.bdd = self.bdd.or(&rhs.bdd);
    }
}
impl PartialOrd for HoaExpression {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for HoaExpression {
    fn cmp(&self, _other: &Self) -> std::cmp::Ordering {
        todo!()
    }
}
impl Show for HoaExpression {
    fn show(&self) -> String {
        let convert_clause = |clause: BddPartialValuation| {
            if clause.is_empty() {
                return "t".to_string();
            }
            clause
                .to_values()
                .into_iter()
                .map(|(var, b)| {
                    if b {
                        var.to_index().to_string()
                    } else {
                        format!("!{}", var.to_index())
                    }
                })
                .join(" & ")
        };

        let dnf = self.bdd.to_dnf();

        if dnf.is_empty() {
            panic!("Decide on how to deal with this!");
        }

        if dnf.len() == 1 {
            return convert_clause(dnf.into_iter().next().expect("length is 1"));
        }

        dnf.into_iter()
            .map(|clause| format!("({})", convert_clause(clause)))
            .join(" | ")
    }
}

pub struct HoaExpressionIter<'a> {
    iter: BddSatisfyingValuations<'a>,
    #[allow(unused)]
    aps: usize,
}

impl<'a> HoaExpressionIter<'a> {
    pub fn new(expr: &'a HoaExpression) -> Self {
        Self {
            iter: expr.bdd.sat_valuations(),
            aps: expr.aps,
        }
    }
}

impl<'a> Iterator for HoaExpressionIter<'a> {
    type Item = HoaSymbol;
    fn next(&mut self) -> Option<Self::Item> {
        let val = self.iter.next()?;
        Some(bdd_valuation_to_hoa_symbol(&val))
    }
}

impl Expression<HoaSymbol> for HoaExpression {
    type SymbolsIter<'this> = HoaExpressionIter<'this> where Self: 'this;

    fn symbols(&self) -> Self::SymbolsIter<'_> {
        HoaExpressionIter::new(self)
    }

    fn matches(&self, symbol: HoaSymbol) -> bool {
        self.bdd.eval_in(&symbol.as_bdd_valuation())
    }
}

pub struct HoaUniverse {
    aps: u8,
    current: u8,
}

impl Iterator for HoaUniverse {
    type Item = HoaSymbol;
    fn next(&mut self) -> Option<Self::Item> {
        if (self.current as u16) < (2u32.saturating_pow(self.aps as u32) as u16) {
            let out = self.current;
            self.current += 1;
            Some(HoaSymbol {
                repr: out,
                aps: (self.aps as usize),
            })
        } else {
            None
        }
    }
}

impl HoaUniverse {
    pub fn new(aps: usize) -> Self {
        assert!(aps < MAX_APS);
        Self {
            aps: aps.try_into().unwrap(),
            current: 0,
        }
    }
}

impl Alphabet for HoaAlphabet {
    type Symbol = HoaSymbol;

    type Expression = HoaExpression;

    fn size(&self) -> usize {
        2u32.saturating_pow(self.apnames.len() as u32)
            .try_into()
            .expect("Cannot fit value into usize")
    }

    type Universe<'this> = HoaUniverse
    where
        Self: 'this;

    fn universe(&self) -> Self::Universe<'_> {
        let aps = self.apnames.len();
        assert!(aps < MAX_APS);
        HoaUniverse::new(self.apnames.len())
    }

    fn contains(&self, symbol: Self::Symbol) -> bool {
        for i in (0..MAX_APS).rev() {
            if (symbol.repr & (1 << i)) > 0 {
                return i < self.apnames.len();
            }
        }
        true
    }

    fn matches(&self, expression: &Self::Expression, symbol: Self::Symbol) -> bool {
        expression.matches(symbol)
    }

    fn make_expression(&self, symbol: Self::Symbol) -> Self::Expression {
        hoa_symbol_to_hoa_expression(&symbol)
    }
}

#[cfg(test)]
mod tests {
    use crate::{hoa::input::hoa_to_ts, TransitionSystem};

    #[test]
    fn parse_generated_hoa() {
        let hoa = r#"HOA: v1
        States: 10
        Start: 0
        AP: 3 "2" "a" "b"
        acc-name: parity min even 5
        Acceptance: 5 Inf(0) | (Fin(1) & (Inf(2) | (Fin(3) & Inf(4))))
        properties: trans-labels explicit-labels trans-acc complete
        properties: deterministic
        --BODY--
        State: 0
        [0&1] 4
        [0&!1] 8
        [!0] 6 {0}
        State: 1
        [!0&1&2] 7 {0}
        [!0&1&!2] 3 {4}
        [!0&!1&2] 4 {0 4}
        [!0&!1&!2] 9 {0}
        [0] 5 {0 4}
        State: 2
        [0&1&2] 2 {0 4}
        [0&1&!2] 6 {3}
        [0&!1&2] 1 {1}
        [0&!1&!2] 4 {1}
        [!0] 7
        State: 3
        [0&1] 0 {2}
        [0&!1] 2
        [!0&1] 4 {3 4}
        [!0&!1] 6 {1 2}
        State: 4
        [0&1] 0 {3}
        [0&!1] 6 {1}
        [!0&1] 7 {0}
        [!0&!1] 4
        State: 5
        [0&1] 9
        [0&!1] 2
        [!0] 4
        State: 6
        [0&1] 9 {4}
        [0&!1] 0 {2}
        [!0&1] 6 {2 3}
        [!0&!1] 4 {0}
        State: 7
        [0&!1&2] 8 {4}
        [0&!1&!2] 1 {4}
        [0&1] 2 {1}
        [!0] 4 {2}
        State: 8
        [!0&1] 2
        [!0&!1] 6
        [0] 4
        State: 9
        [0] 5
        [!0] 3
        --END--
        "#;
        let start = std::time::Instant::now();
        let auts = hoa_to_ts(hoa);
        println!("Took {}ms", start.elapsed().as_millis());
        assert_eq!(auts.len(), 1);
        let aut = &auts[0];
        assert_eq!(aut.size(), 10);
    }
}
