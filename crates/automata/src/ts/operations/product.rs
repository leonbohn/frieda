use crate::core::{Show, alphabet::Alphabet};
use crate::ts::predecessors::PredecessorIterable;
use crate::ts::{EdgeExpression, IndexType, IsEdge, StateIndex, SymbolOf};
use crate::{Pointed, TransitionSystem};
use itertools::Itertools;

/// Trait that abstracts the product operation for transition systems.
pub trait Product: TransitionSystem + Sized {
    /// The type of output that is produced by the product operation.
    type Output<Ts: TransitionSystem<Alphabet = Self::Alphabet>>: TransitionSystem<Alphabet = Self::Alphabet, StateColor = (Self::StateColor, Ts::StateColor)>;
    /// Perform a product operation on two transition systems.
    fn ts_product<Ts: TransitionSystem<Alphabet = Self::Alphabet>>(
        self,
        other: Ts,
    ) -> Self::Output<Ts>;
}

impl<Lts: TransitionSystem + Sized> Product for Lts {
    type Output<Ts: TransitionSystem<Alphabet = Self::Alphabet>> = MatchingProduct<Self, Ts>;

    fn ts_product<Ts: TransitionSystem<Alphabet = Self::Alphabet>>(
        self,
        other: Ts,
    ) -> Self::Output<Ts> {
        MatchingProduct(self, other)
    }
}

/// Index type for product transition systems.
#[derive(Copy, Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ProductIndex<L, R>(pub L, pub R);

impl<L, R> From<ProductIndex<L, R>> for (L, R) {
    fn from(value: ProductIndex<L, R>) -> Self {
        (value.0, value.1)
    }
}

impl<L: Show, R: Show> Show for ProductIndex<L, R> {
    fn show(&self) -> String {
        format!("({} | {})", self.0.show(), self.1.show())
    }

    fn show_collection<'a, I>(iter: I) -> String
    where
        Self: 'a,
        I: IntoIterator<Item = &'a Self>,
        I::IntoIter: DoubleEndedIterator,
    {
        format!("{{{}}}", iter.into_iter().map(|x| x.show()).join(", "))
    }
}

impl<L: std::fmt::Display, R: std::fmt::Display> std::fmt::Display for ProductIndex<L, R> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}", self.0, self.1)
    }
}

/// A product of two transition systems, which only permits transitions on matching symbols, while
/// ignoring all others.
#[derive(Copy, Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct MatchingProduct<L, R>(pub L, pub R);

impl<L, R> TransitionSystem for MatchingProduct<L, R>
where
    L: TransitionSystem,
    R: TransitionSystem,
    R::Alphabet: Alphabet<Symbol = SymbolOf<L>, Expression = EdgeExpression<L>>,
    L::StateColor: Clone,
    R::StateColor: Clone,
{
    type StateIndex = ProductIndex<L::StateIndex, R::StateIndex>;
    type EdgeColor = (L::EdgeColor, R::EdgeColor);
    type StateColor = (L::StateColor, R::StateColor);
    type EdgeRef<'this>
        = ProductTransition<
        'this,
        L::StateIndex,
        R::StateIndex,
        EdgeExpression<L>,
        L::EdgeColor,
        R::EdgeColor,
    >
    where
        Self: 'this;
    type EdgesFromIter<'this>
        = ProductEdgesFrom<'this, L, R>
    where
        Self: 'this;
    type StateIndices<'this>
        = ProductStatesIter<'this, L, R>
    where
        Self: 'this;
    type Alphabet = L::Alphabet;
    fn alphabet(&self) -> &Self::Alphabet {
        self.0.alphabet()
    }

    fn state_indices(&self) -> Self::StateIndices<'_> {
        ProductStatesIter::new(&self.0, &self.1)
    }

    fn state_color(&self, state: StateIndex<Self>) -> Option<Self::StateColor> {
        let ProductIndex(l, r) = state;
        let left = self.0.state_color(l)?;
        let right = self.1.state_color(r)?;
        Some((left, right))
    }

    fn edges_from(&self, state: StateIndex<Self>) -> Option<Self::EdgesFromIter<'_>> {
        ProductEdgesFrom::new(&self.0, &self.1, state)
    }

    fn maybe_initial_state(&self) -> Option<Self::StateIndex> {
        Some(ProductIndex(
            self.0.maybe_initial_state()?,
            self.1.maybe_initial_state()?,
        ))
    }
}

impl<L, R> Pointed for MatchingProduct<L, R>
where
    L: Pointed,
    R: Pointed<Alphabet = L::Alphabet>,
{
    fn initial(&self) -> Self::StateIndex {
        ProductIndex(self.0.initial(), self.1.initial())
    }
}

/// Iterator over the state indices of a product transition system.
pub struct ProductStatesIter<'a, L: TransitionSystem, R: TransitionSystem> {
    left: L::StateIndices<'a>,
    right: R::StateIndices<'a>,
    left_state: Option<L::StateIndex>,
    _lts: &'a L,
    rts: &'a R,
}

impl<L: TransitionSystem, R: TransitionSystem> Iterator for ProductStatesIter<'_, L, R> {
    type Item = ProductIndex<L::StateIndex, R::StateIndex>;

    fn next(&mut self) -> Option<Self::Item> {
        match self.left_state {
            None => None,
            Some(p) => {
                if let Some(q) = self.right.next() {
                    Some(ProductIndex(p, q))
                } else {
                    self.left_state = self.left.next();
                    self.right = self.rts.state_indices();
                    self.next()
                }
            }
        }
    }
}

impl<'a, L: TransitionSystem, R: TransitionSystem> ProductStatesIter<'a, L, R> {
    /// Create a new iterator over the state indices of a product transition system.
    pub fn new(lts: &'a L, rts: &'a R) -> Self {
        let mut lit = lts.state_indices();
        Self {
            left_state: lit.next(),
            left: lit,
            right: rts.state_indices(),
            rts,
            _lts: lts,
        }
    }
}

/// Type that encapsulates a transition in a product transition system.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct ProductTransition<'a, LI, RI, E, LC, RC> {
    expression: &'a E,
    source: ProductIndex<LI, RI>,
    target: ProductIndex<LI, RI>,
    color: (LC, RC),
}

#[allow(missing_docs)]
impl<'a, LI, RI, E, LC, RC> ProductTransition<'a, LI, RI, E, LC, RC> {
    pub fn new(
        source: ProductIndex<LI, RI>,
        expression: &'a E,
        color: (LC, RC),
        target: ProductIndex<LI, RI>,
    ) -> Self {
        Self {
            source,
            expression,
            target,
            color,
        }
    }
}

impl<'a, Idx, Jdx, E, C, D> IsEdge<'a, E, ProductIndex<Idx, Jdx>, (C, D)>
    for ProductTransition<'a, Idx, Jdx, E, C, D>
where
    C: Clone,
    D: Clone,
    Idx: IndexType,
    Jdx: IndexType,
{
    fn source(&self) -> ProductIndex<Idx, Jdx> {
        ProductIndex(self.source.0, self.source.1)
    }

    fn target(&self) -> ProductIndex<Idx, Jdx> {
        self.target
    }

    fn color(&self) -> (C, D) {
        self.color.clone()
    }

    fn expression(&self) -> &'a E {
        self.expression
    }
}

/// Iterator over the edges of a product transition system.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProductEdgesFrom<'a, L: TransitionSystem, R: TransitionSystem> {
    left: &'a L,
    right: &'a R,
    cur: Option<L::EdgeRef<'a>>,
    it: L::EdgesFromIter<'a>,
    right_edges: Vec<R::EdgeRef<'a>>,
    position: usize,
}

#[allow(missing_docs)]
impl<'a, L: TransitionSystem, R: TransitionSystem> ProductEdgesFrom<'a, L, R> {
    pub fn new(
        left: &'a L,
        right: &'a R,
        index: ProductIndex<L::StateIndex, R::StateIndex>,
    ) -> Option<Self> {
        let mut it = left.edges_from(index.0)?;
        Some(Self {
            left,
            right_edges: right.edges_from(index.1)?.collect(),
            right,
            position: 0,
            cur: it.next(),
            it,
        })
    }
}

impl<'a, L, R> Iterator for ProductEdgesFrom<'a, L, R>
where
    L: TransitionSystem,
    R: TransitionSystem,
    R::Alphabet: Alphabet<Symbol = SymbolOf<L>, Expression = EdgeExpression<L>>,
{
    type Item = ProductTransition<
        'a,
        L::StateIndex,
        R::StateIndex,
        EdgeExpression<L>,
        L::EdgeColor,
        R::EdgeColor,
    >;

    #[allow(clippy::question_mark)]
    fn next(&mut self) -> Option<Self::Item> {
        if self.position >= self.right_edges.len() {
            self.position = 0;
            self.cur = self.it.next();
        }
        match &self.cur {
            None => return None,
            Some(l) => {
                while let Some(r) = self.right_edges.get(self.position) {
                    self.position += 1;
                    if l.expression() == r.expression() {
                        return Some(ProductTransition::new(
                            ProductIndex(l.source(), r.source()),
                            l.expression(),
                            (l.color(), r.color()),
                            ProductIndex(l.target(), r.target()),
                        ));
                    }
                }
            }
        }
        self.next()
    }
}

/// Analogous to [`ProductTransition`]; encapsulates a pre-transition in a product transition system.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProductPreTransition<'a, LI, RI, E, LC, RC> {
    source: ProductIndex<LI, RI>,
    expression: &'a E,
    color: (LC, RC),
    target: ProductIndex<LI, RI>,
}

impl<'a, LI, RI, E, LC, RC> IsEdge<'a, E, ProductIndex<LI, RI>, (LC, RC)>
    for ProductPreTransition<'a, LI, RI, E, LC, RC>
where
    LC: Clone,
    RC: Clone,
    LI: IndexType,
    RI: IndexType,
{
    fn color(&self) -> (LC, RC) {
        self.color.clone()
    }

    fn source(&self) -> ProductIndex<LI, RI> {
        self.source
    }

    fn target(&self) -> ProductIndex<LI, RI> {
        self.target
    }

    fn expression(&self) -> &'a E {
        self.expression
    }
}

#[allow(missing_docs)]
impl<'a, LI, RI, E, LC, RC> ProductPreTransition<'a, LI, RI, E, LC, RC> {
    pub fn new(
        source: ProductIndex<LI, RI>,
        expression: &'a E,
        color: (LC, RC),
        target: ProductIndex<LI, RI>,
    ) -> Self {
        Self {
            target,
            source,
            expression,
            color,
        }
    }
}

/// Iterator over the predecessors of a product transition system.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProductEdgesTo<'a, L: PredecessorIterable, R: PredecessorIterable> {
    left: &'a L,
    right: &'a R,
    cur: Option<L::PreEdgeRef<'a>>,
    it: L::EdgesToIter<'a>,
    right_edges: Vec<R::PreEdgeRef<'a>>,
    position: usize,
}

impl<'a, L, R> Iterator for ProductEdgesTo<'a, L, R>
where
    L: PredecessorIterable,
    R: PredecessorIterable,
    R::Alphabet: Alphabet<Symbol = SymbolOf<L>, Expression = EdgeExpression<L>>,
{
    type Item = ProductPreTransition<
        'a,
        L::StateIndex,
        R::StateIndex,
        EdgeExpression<L>,
        L::EdgeColor,
        R::EdgeColor,
    >;

    #[allow(clippy::question_mark)]
    fn next(&mut self) -> Option<Self::Item> {
        if self.position >= self.right_edges.len() {
            self.position = 0;
            self.cur = self.it.next();
        }
        match &self.cur {
            None => return None,
            Some(left) => {
                while let Some(right) = self.right_edges.get(self.position) {
                    self.position += 1;
                    if left.expression() == right.expression() {
                        return Some(ProductPreTransition::new(
                            ProductIndex(left.source(), right.source()),
                            left.expression(),
                            (left.color(), right.color()),
                            ProductIndex(left.target(), right.target()),
                        ));
                    }
                }
            }
        }
        self.next()
    }
}

#[allow(missing_docs)]
impl<'a, L: PredecessorIterable, R: PredecessorIterable> ProductEdgesTo<'a, L, R> {
    pub fn new(
        left: &'a L,
        right: &'a R,
        index: ProductIndex<L::StateIndex, R::StateIndex>,
    ) -> Option<Self> {
        let mut it = left.predecessors(index.0)?;
        Some(Self {
            left,
            right,
            cur: it.next(),
            it,
            right_edges: right.predecessors(index.1)?.collect(),
            position: 0,
        })
    }
}

#[cfg(test)]
mod tests {
    use crate::DTS;
    use crate::automaton::MealyMachine;
    use crate::ts::Deterministic;
    use crate::ts::operations::Product;
    use automata_core::Void;

    #[test]
    fn product_subalphabet() {
        let l: MealyMachine = DTS::builder()
            .default_color(Void)
            .with_transitions([(0, 'a', 0, 0), (0, 'b', 0, 0)])
            .into_mealy(0);
        let r: MealyMachine = DTS::builder()
            .default_color(Void)
            .with_transitions([(0, 'a', 0, 0)])
            .into_mealy(0);
        assert!((&l).ts_product(&l).reached_state_index("b").is_some());
        let prod = l.ts_product(r);
        assert!(prod.reached_state_index("a").is_some());
        assert!(prod.reached_state_index("b").is_none());
    }
}
