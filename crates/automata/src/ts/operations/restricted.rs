use crate::ts::predecessors::PredecessorIterable;
use crate::ts::{Deterministic, EdgeColor, EdgeExpression, IndexType, IsEdge, StateIndex};
use crate::{Congruence, Pointed, TransitionSystem};
use automata_core::alphabet::Matcher;
use automata_core::math::OrderedSet;
use std::marker::PhantomData;

/// Abstracts the filtering of a transition system's state indices. This trait is implemented by
/// functions which take a state index and return a boolean value indicating whether the state index
/// should be filtered out or not. It is also implemented by [`Vec`] and [`crate::core::math::Set`] which are used to
/// filter out state indices that are not contained in the vector or set.
pub trait StateIndexFilter<Idx: IndexType> {
    /// This method is called to check whether an index should be present in a filtered transition
    /// system or not. Any index for which the function returns `true`, will be present, while all those
    /// for which the function returns `false` are masked out.
    fn is_unmasked(&self, idx: Idx) -> bool;

    /// The counterpart to [`Self::is_unmasked`]. This method is called to check whether an index
    /// should be masked out or not. Any index for which the function returns `true`, will be masked
    /// out, while all those for which the function returns `false` are present.
    fn is_masked(&self, idx: Idx) -> bool {
        !self.is_unmasked(idx)
    }
}

impl<Idx, F> StateIndexFilter<Idx> for F
where
    Idx: IndexType,
    F: Fn(Idx) -> bool,
{
    #[inline(always)]
    fn is_unmasked(&self, idx: Idx) -> bool {
        (self)(idx)
    }
}

impl<Idx> StateIndexFilter<Idx> for Vec<Idx>
where
    Idx: IndexType,
{
    #[inline(always)]
    fn is_unmasked(&self, idx: Idx) -> bool {
        self.contains(&idx)
    }
}

impl<Idx> StateIndexFilter<Idx> for OrderedSet<Idx>
where
    Idx: IndexType,
{
    #[inline(always)]
    fn is_unmasked(&self, idx: Idx) -> bool {
        self.contains(&idx)
    }
}

/// Restricts a transition system to a subset of its state indices, which is defined by a filter
/// function.
#[derive(Debug, Clone)]
pub struct RestrictByStateIndex<Ts: TransitionSystem, F> {
    ts: Ts,
    filter: F,
}

impl<Ts: TransitionSystem, F> TransitionSystem for RestrictByStateIndex<Ts, F>
where
    F: StateIndexFilter<Ts::StateIndex>,
{
    type StateIndex = Ts::StateIndex;
    type EdgeColor = Ts::EdgeColor;
    type StateColor = Ts::StateColor;
    type EdgeRef<'this>
        = Ts::EdgeRef<'this>
    where
        Self: 'this;
    type EdgesFromIter<'this>
        = RestrictedEdgesFromIter<'this, Ts, F>
    where
        Self: 'this;
    type StateIndices<'this>
        = RestrictedStateIndices<'this, Ts, F>
    where
        Self: 'this;

    type Alphabet = Ts::Alphabet;

    fn alphabet(&self) -> &Self::Alphabet {
        self.ts().alphabet()
    }
    fn state_indices(&self) -> Self::StateIndices<'_> {
        RestrictedStateIndices {
            it: self.ts().state_indices(),
            filter: &self.filter,
        }
    }

    fn state_color(&self, state: StateIndex<Self>) -> Option<Self::StateColor> {
        if (self.filter()).is_masked(state) {
            return None;
        }
        self.ts().state_color(state)
    }

    fn edges_from(&self, state: StateIndex<Self>) -> Option<Self::EdgesFromIter<'_>> {
        if !(self.filter()).is_unmasked(state) {
            return None;
        }
        self.ts()
            .edges_from(state)
            .map(|iter| RestrictedEdgesFromIter::new(iter, self.filter()))
    }

    fn maybe_initial_state(&self) -> Option<Self::StateIndex> {
        let initial = self.ts().maybe_initial_state()?;
        if self.filter().is_unmasked(initial) {
            Some(initial)
        } else {
            None
        }
    }
}

/// Iterator over the state indices of a transition system that are restricted by a filter function.
pub struct RestrictByStateIndexIter<'a, Ts: TransitionSystem + 'a, F> {
    filter: &'a F,
    it: Ts::StateIndices<'a>,
}

impl<Ts, F> Iterator for RestrictByStateIndexIter<'_, Ts, F>
where
    Ts: TransitionSystem,
    F: StateIndexFilter<Ts::StateIndex>,
{
    type Item = Ts::StateIndex;
    fn next(&mut self) -> Option<Self::Item> {
        self.it.find(|idx| self.filter.is_unmasked(*idx))
    }
}

impl<'a, Ts: TransitionSystem, F> RestrictByStateIndexIter<'a, Ts, F> {
    /// Creates a new iterator over the state indices of a transition system that are restricted by a
    /// filter function.
    pub fn new(filter: &'a F, it: Ts::StateIndices<'a>) -> Self {
        Self { filter, it }
    }
}

impl<Ts: TransitionSystem + Pointed, F> Pointed for RestrictByStateIndex<Ts, F>
where
    F: StateIndexFilter<Ts::StateIndex>,
{
    fn initial(&self) -> Self::StateIndex {
        let initial = self.ts.initial();
        assert!(
            (self.filter).is_unmasked(initial),
            "initial state is filtered out"
        );
        initial
    }
}

#[allow(missing_docs)]
impl<Ts: TransitionSystem, F> RestrictByStateIndex<Ts, F> {
    pub fn new(ts: Ts, filter: F) -> Self {
        Self { ts, filter }
    }
    pub fn into_parts(self) -> (Ts, F) {
        (self.ts, self.filter)
    }

    pub fn filter(&self) -> &F {
        &self.filter
    }

    pub fn ts(&self) -> &Ts {
        &self.ts
    }
}

/// Adapts an iterator of state indices to filter out those that are masked
/// by a filter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RestrictedStateIndices<'a, Ts: TransitionSystem + 'a, F> {
    filter: &'a F,
    it: Ts::StateIndices<'a>,
}

impl<'a, Ts: TransitionSystem + 'a, F> Iterator for RestrictedStateIndices<'a, Ts, F>
where
    F: StateIndexFilter<Ts::StateIndex>,
{
    type Item = StateIndex<Ts>;
    fn next(&mut self) -> Option<Self::Item> {
        self.it.find(|q| self.filter.is_unmasked(*q))
    }
}

/// Iterator over the edges of a transition system that are restricted by a filter function.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RestrictedEdgesFromIter<'a, Ts: TransitionSystem + 'a, F> {
    filter: &'a F,
    it: Ts::EdgesFromIter<'a>,
}

#[allow(missing_docs)]
impl<'a, Ts: TransitionSystem + 'a, F> RestrictedEdgesFromIter<'a, Ts, F> {
    pub fn new(it: Ts::EdgesFromIter<'a>, filter: &'a F) -> Self {
        Self { filter, it }
    }
}

impl<'a, Ts: TransitionSystem + 'a, F> Iterator for RestrictedEdgesFromIter<'a, Ts, F>
where
    F: StateIndexFilter<Ts::StateIndex>,
{
    type Item = Ts::EdgeRef<'a>;
    fn next(&mut self) -> Option<Self::Item> {
        self.it
            .by_ref()
            .find(|edge| (self.filter).is_unmasked(edge.target()))
    }
}

/// Iterator over the predecessors in a transition system that are restricted by a filter function.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RestrictedEdgesToIter<'a, Ts: PredecessorIterable + 'a, F> {
    filter: &'a F,
    it: Ts::EdgesToIter<'a>,
}

impl<'a, Ts: PredecessorIterable + 'a, F> Iterator for RestrictedEdgesToIter<'a, Ts, F>
where
    F: StateIndexFilter<Ts::StateIndex>,
{
    type Item = Ts::PreEdgeRef<'a>;
    fn next(&mut self) -> Option<Self::Item> {
        self.it
            .by_ref()
            .find(|edge| (self.filter).is_unmasked(edge.source()))
    }
}

#[allow(missing_docs)]
impl<'a, Ts: PredecessorIterable + 'a, F> RestrictedEdgesToIter<'a, Ts, F> {
    pub fn new(it: Ts::EdgesToIter<'a>, filter: &'a F) -> Self {
        Self { filter, it }
    }
}

/// Takes a transition system and restricts the the possible edge colors. For this, we assume that the colors
/// can be ordered and we are given a minimal and maximal allowed color.
#[derive(Clone, Debug)]
pub struct EdgeColorRestricted<D: TransitionSystem> {
    ts: D,
    min: D::EdgeColor,
    max: D::EdgeColor,
}

impl<D: Congruence> Pointed for EdgeColorRestricted<D>
where
    EdgeColor<D>: Ord,
{
    fn initial(&self) -> Self::StateIndex {
        self.ts.initial()
    }
}

impl<D: TransitionSystem> TransitionSystem for EdgeColorRestricted<D>
where
    EdgeColor<D>: Ord,
{
    type StateIndex = D::StateIndex;

    type StateColor = D::StateColor;

    type EdgeColor = D::EdgeColor;

    type EdgeRef<'this>
        = D::EdgeRef<'this>
    where
        Self: 'this;

    type EdgesFromIter<'this>
        = ColorRestrictedEdgesFrom<'this, D>
    where
        Self: 'this;

    type StateIndices<'this>
        = D::StateIndices<'this>
    where
        Self: 'this;

    type Alphabet = D::Alphabet;

    fn alphabet(&self) -> &Self::Alphabet {
        self.ts().alphabet()
    }
    fn state_indices(&self) -> Self::StateIndices<'_> {
        self.ts().state_indices()
    }

    fn edges_from(&self, state: StateIndex<Self>) -> Option<Self::EdgesFromIter<'_>> {
        let min = self.min.clone();
        let max = self.max.clone();
        Some(ColorRestrictedEdgesFrom {
            min,
            max,
            _phantom: PhantomData,
            it: self.ts().edges_from(state)?,
        })
    }

    fn state_color(&self, state: StateIndex<Self>) -> Option<Self::StateColor> {
        self.ts().state_color(state)
    }
}

impl<D: PredecessorIterable<EdgeColor = usize>> PredecessorIterable for EdgeColorRestricted<D> {
    type PreEdgeRef<'this>
        = D::PreEdgeRef<'this>
    where
        Self: 'this;

    type EdgesToIter<'this>
        = ColorRestrictedEdgesTo<'this, &'this D>
    where
        Self: 'this;

    fn predecessors(&self, state: StateIndex<Self>) -> Option<Self::EdgesToIter<'_>> {
        Some(ColorRestrictedEdgesTo::new(
            self.ts().predecessors(state)?,
            self.min,
            self.max,
        ))
    }
}

impl<D> Deterministic for EdgeColorRestricted<D>
where
    D: Deterministic,
    EdgeColor<D>: Ord,
{
    fn edge(
        &self,
        state: StateIndex<Self>,
        matcher: impl Matcher<EdgeExpression<Self>>,
    ) -> Option<Self::EdgeRef<'_>> {
        self.ts()
            .edge(state, matcher)
            .filter(|t| t.color() <= self.max && self.min <= t.color())
    }
}

/// Adapted iterator giving the edges from a state in a transition system that are restricted by a
/// color range. See [`EdgeColorRestricted`] for more information.
pub struct ColorRestrictedEdgesFrom<'a, D: TransitionSystem> {
    _phantom: PhantomData<&'a D>,
    it: D::EdgesFromIter<'a>,
    min: D::EdgeColor,
    max: D::EdgeColor,
}

impl<'a, D: TransitionSystem> Iterator for ColorRestrictedEdgesFrom<'a, D>
where
    EdgeColor<D>: Ord,
{
    type Item = D::EdgeRef<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        self.it
            .find(|t| t.color() <= self.max && self.min <= t.color())
    }
}

/// Adapted iterator giving the edges to a state in a transition system that are restricted by a
/// color range. See [`EdgeColorRestricted`] for more information.
pub struct ColorRestrictedEdgesTo<'a, D: PredecessorIterable> {
    _phantom: PhantomData<&'a D>,
    it: D::EdgesToIter<'a>,
    min: D::EdgeColor,
    max: D::EdgeColor,
}

impl<'a, D: PredecessorIterable> ColorRestrictedEdgesTo<'a, D> {
    /// Creates a new instance of the iterator.
    pub fn new(it: D::EdgesToIter<'a>, min: D::EdgeColor, max: D::EdgeColor) -> Self {
        Self {
            _phantom: PhantomData,
            it,
            min,
            max,
        }
    }
}

impl<'a, D: PredecessorIterable> Iterator for ColorRestrictedEdgesTo<'a, D>
where
    EdgeColor<D>: Ord,
{
    type Item = D::PreEdgeRef<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        self.it
            .find(|t| t.color() <= self.max && self.min <= t.color())
    }
}

impl<D: TransitionSystem> EdgeColorRestricted<D> {
    /// Returns a reference to the underlying transition system.
    pub fn ts(&self) -> &D {
        &self.ts
    }
    /// Creates a new instance for a given transition system and a color range (as specified by the `min` and `max`
    /// allowed color)
    pub fn new(ts: D, min: D::EdgeColor, max: D::EdgeColor) -> Self {
        Self { ts, min, max }
    }
}

#[cfg(test)]
mod tests {
    use crate::TransitionSystem;
    use crate::representation::IntoTs;
    use crate::ts::TSBuilder;

    #[test]
    fn restrict_ts_by_state_index() {
        let dfa = TSBuilder::without_edge_colors()
            .with_state_colors([false, false, true])
            .with_edges([
                (0, 'a', 1),
                (0, 'b', 0),
                (1, 'a', 2),
                (1, 'b', 1),
                (2, 'a', 0),
                (2, 'b', 2),
            ])
            .into_dfa(0);

        assert!(dfa.accepts("aa"));

        let restricted = dfa.restrict_state_indices(|idx| idx != 2);
        assert!(!restricted.into_dfa().accepts("aa"));
    }
}
