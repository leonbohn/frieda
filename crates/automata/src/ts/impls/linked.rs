use std::{collections::BTreeSet, fmt::Debug, hash::Hash};

use crate::ts::{
    DefaultIdType, Deterministic, EdgeColor, EdgeExpression, EdgeTuple, ForAlphabet, IntoEdgeTuple,
    IsEdge, ScalarIndexType, Shrinkable, Sproutable, StateColor, StateIndex, TSBuilder,
};
use itertools::Itertools;
use owo_colors::OwoColorize;

mod linked_state;
pub use linked_state::{
    LinkedListTransitionSystemEdgesToIter, LinkedListTransitionSystemState, NTSEdgesFromIter,
};

mod linked_edge;
use crate::TransitionSystem;
use crate::core::{
    Color, Show, Void,
    alphabet::{Alphabet, CharAlphabet, Expression, Matcher},
    math,
};
use crate::ts::predecessors::PredecessorIterable;
pub use linked_edge::LinkedListTransitionSystemEdge;
use tracing::{trace, warn};

/// Represents a non-deterministic transition system. It stores an [`Alphabet`], a list of [`LinkedListTransitionSystemState`]s and a list of [`LinkedListTransitionSystemEdge`]s.
#[derive(Clone)]
pub struct LinkedListTransitionSystem<
    A: Alphabet = CharAlphabet,
    Q = Void,
    C = Void,
    const DET: bool = true,
> {
    alphabet: A,
    states: Vec<LinkedListTransitionSystemState<Q>>,
    edges: Vec<LinkedListTransitionSystemEdge<A::Expression, C>>,
    vacant: Option<usize>,
}

impl<A: Alphabet, Q: Color, C: Color, const DET: bool> LinkedListTransitionSystem<A, Q, C, DET> {
    pub fn zip_state_indices(self) -> LinkedListTransitionSystem<A, (DefaultIdType, Q), C, DET> {
        LinkedListTransitionSystem {
            alphabet: self.alphabet,
            edges: self.edges,
            states: self
                .states
                .into_iter()
                .enumerate()
                .map(|(i, s)| s.zip_prepend(ScalarIndexType::from_usize(i)))
                .collect(),
            vacant: self.vacant,
        }
    }

    pub fn linked_map_states<QQ: Color, StateF>(
        self,
        state_f: StateF,
    ) -> LinkedListTransitionSystem<A, QQ, C, DET>
    where
        StateF: Fn(StateIndex<Self>, Q) -> QQ,
    {
        self.linked_map(state_f, |_q, a: <A as Alphabet>::Expression, c, _p| (a, c))
    }

    pub fn linked_map_edges<CC: Color, EdgeF>(
        self,
        edge_f: EdgeF,
    ) -> LinkedListTransitionSystem<A, Q, CC, DET>
    where
        EdgeF: Fn(
            StateIndex<Self>,
            EdgeExpression<Self>,
            EdgeColor<Self>,
            StateIndex<Self>,
        ) -> (EdgeExpression<Self>, CC),
    {
        self.linked_map(|_idx, c| c, edge_f)
    }

    pub fn linked_map<QQ: Color, CC: Color, StateF, EdgeF>(
        self,
        state_f: StateF,
        edge_f: EdgeF,
    ) -> LinkedListTransitionSystem<A, QQ, CC, DET>
    where
        StateF: Fn(StateIndex<Self>, Q) -> QQ,
        EdgeF: Fn(
            StateIndex<Self>,
            EdgeExpression<Self>,
            EdgeColor<Self>,
            StateIndex<Self>,
        ) -> (EdgeExpression<Self>, CC),
    {
        LinkedListTransitionSystem {
            alphabet: self.alphabet,
            states: self
                .states
                .into_iter()
                .enumerate()
                .map(|(i, s)| s.map_occupied(|c| state_f(ScalarIndexType::from_usize(i), c)))
                .collect(),
            edges: self
                .edges
                .into_iter()
                .map(|e| {
                    e.map(|q, e, c, p| {
                        edge_f(
                            ScalarIndexType::from_usize(q),
                            e,
                            c,
                            ScalarIndexType::from_usize(p),
                        )
                    })
                })
                .collect(),
            vacant: self.vacant,
        }
    }

    /// This removes a state from the NTS and returns the color of the removed state.
    /// This method also removes any incoming edges to this state.
    pub(crate) fn linked_remove_state(&mut self, state: usize) -> Option<Q> {
        if state >= self.states.len() {
            warn!("tried to remove state {state} which is out of range");
            return None;
        }
        if self.states[state].is_vacant() {
            warn!("cannot remove vacant state {state}");
            return None;
        }

        // TODO: can we do this more efficiently?
        while let Some(out_edge) = self.first_out_edge(state) {
            self.swap_remove_edge(out_edge);
        }
        while let Some(in_edge) = self.first_in_edge(state) {
            self.swap_remove_edge(in_edge);
        }

        self.vacate_state_index(state)
    }

    fn vacate_state_index(&mut self, state: usize) -> Option<Q> {
        assert_eq!(self.first_in_edge(state), None);
        assert_eq!(self.first_out_edge(state), None);

        let out = self.states[state].vacate(self.vacant, None);
        if let Some(prev) = self.vacant {
            self.states[prev].set_next_vacancy(Some(state));
        }
        self.vacant = Some(state);

        out
    }

    fn swap_remove_edge(&mut self, id: usize) -> Option<EdgeTuple<Self>> {
        if id >= self.edges.len() {
            warn!("tried to remove edge {id} which is not in rante");
            return None;
        }

        if let Some(in_prev) = self.edges[id].in_prev {
            self.edges[in_prev].in_next = self.edges[id].in_next;
        } else {
            // there is no predecessor, so we should update the first_in_edge
            assert_eq!(self.states[self.edges[id].target].first_in_edge(), Some(id));
            self.states
                .get_mut(self.edges[id].target)
                .unwrap()
                .set_first_in_edge(self.edges[id].in_next);
        }
        if let Some(in_next) = self.edges[id].in_next {
            self.edges[in_next].in_prev = self.edges[id].in_prev;
        }

        if let Some(out_prev) = self.edges[id].out_prev {
            self.edges[out_prev].out_next = self.edges[id].out_next;
        } else {
            // there is no predecessor, so we should update the first_out_edge
            assert_eq!(
                self.states[self.edges[id].source].first_out_edge(),
                Some(id)
            );
            self.states
                .get_mut(self.edges[id].source)
                .unwrap()
                .set_first_out_edge(self.edges[id].out_next);
        }

        if let Some(out_next) = self.edges[id].out_next {
            self.edges[out_next].out_prev = self.edges[id].out_prev;
        }

        let swapped_in = self.edges.len() - 1;
        if swapped_in != id {
            // fix pointers if edge that is swapped in was first in list
            if let Some(prev) = self.edges[swapped_in].in_prev {
                self.edges[prev].in_next = Some(id);
            } else {
                let q = self.edges[swapped_in].target;
                assert!(q < self.states.len());
                self.states[q].set_first_in_edge(Some(id));
            }
            if let Some(prev) = self.edges[swapped_in].out_prev {
                self.edges[prev].out_next = Some(id)
            } else {
                let q = self.edges[swapped_in].source;
                assert!(q < self.states.len());
                self.states[q].set_first_out_edge(Some(id));
            }

            if let Some(next) = self.edges[swapped_in].in_next {
                if self.edges[next].in_prev.is_none() {
                    panic!(
                        "unacceptable state of linked list, no in_prev stored for {next}\n{self:?}"
                    );
                }
                self.edges[next].in_prev = Some(id)
            }
            if let Some(next) = self.edges[swapped_in].out_next {
                if self.edges[next].out_prev.is_none() {
                    panic!(
                        "unacceptable state of linked list, no out_prev stored for {next}\n{self:?}"
                    );
                }
                self.edges[next].out_prev = Some(id)
            }
        }

        Some(self.edges.swap_remove(id).into_tuple())
    }

    /// Applies the given function `f` to every edge and removes those for which `true` is
    /// returned.
    pub fn filter_swap_remove_edges<F>(&mut self, f: F) -> Vec<EdgeTuple<Self>>
    where
        F: Fn(StateIndex<Self>, &EdgeExpression<Self>, &EdgeColor<Self>, StateIndex<Self>) -> bool,
    {
        let mut i = 0;
        let mut removed = vec![];
        while i < self.edges.len() {
            let (q, e, c, p) = self.edges[i].borrowed_tuple();
            if !f(q, e, c, p) {
                // we keep the edge and go to next one
                i += 1;
                continue;
            }
            // we do not keep the edge. swap remove it and do not increment
            // i as the swapped in edge needs to be checked as well.
            removed.push(
                self.swap_remove_edge(i)
                    .expect("should be able to remove edge that exists"),
            );
        }
        removed
    }

    fn out_edge_position(
        &self,
        from: usize,
        matcher: impl Matcher<A::Expression>,
    ) -> Option<usize> {
        let mut idx = self.states.get(from)?.first_out_edge()?;
        loop {
            // FIXME: Make this be a match
            if matcher.matches((&self.edges[idx]).expression()) {
                return Some(idx);
            }
            idx = self.edges[idx].out_next?;
        }
    }

    fn in_edge_position(&self, matcher: impl Matcher<A::Expression>, to: usize) -> Option<usize> {
        let mut idx = self.states.get(to)?.first_in_edge()?;
        loop {
            // FIXME: Make this be a match
            if matcher.matches((&self.edges[idx]).expression()) {
                return Some(idx);
            }
            idx = self.edges[idx].in_next?;
        }
    }

    /// Builds a new non-deterministic transition system for the given alphabet with a specified capacity.
    pub fn with_capacity(alphabet: A, cap: usize) -> Self {
        Self {
            alphabet,
            states: Vec::with_capacity(cap),
            edges: Vec::with_capacity(cap),
            vacant: None,
        }
    }

    /// Turns `self` into a deterministic transition system. Panics if `self` is not deterministic.
    pub fn into_deterministic(self) -> LinkedListTransitionSystem<A, Q, C> {
        match self.try_into_deterministic() {
            Ok(ts) => ts,
            Err(ts) => {
                tracing::error!(
                    "Tried to convert non-deterministic transition system to deterministic one\n{:?}",
                    ts
                );
                panic!("This transition system is not deterministic");
            }
        }
    }

    /// Turns `self` into a deterministic transition system. Panics if `self` is not deterministic.
    pub fn try_into_deterministic(self) -> Result<LinkedListTransitionSystem<A, Q, C, true>, Self> {
        if DET {
            if !self.is_deterministic() {
                tracing::error!(
                    "Tried to convert non-deterministic transition system to deterministic one\n{:?}",
                    self
                );
                panic!("This transition system is not deterministic");
            }
            Ok(recast(self))
        } else if self.is_deterministic() {
            Ok(recast(self))
        } else {
            Err(self)
        }
    }

    pub fn into_nondeterministic(self) -> LinkedListNondeterministic<A, Q, C> {
        recast(self)
    }

    fn first_out_edge(&self, idx: usize) -> Option<usize> {
        self.states.get(idx)?.first_out_edge()
    }

    fn first_in_edge(&self, idx: usize) -> Option<usize> {
        self.states.get(idx)?.first_in_edge()
    }

    fn last_out_edge(&self, idx: usize) -> Option<usize> {
        let mut current = self.states.get(idx)?.first_out_edge()?;
        loop {
            assert!(
                current < self.edges.len(),
                "Edge with id {current} does not exist"
            );
            if let Some(x) = self.edges[current].out_next {
                current = x;
            } else {
                return Some(current);
            }
        }
    }

    fn last_in_edge(&self, idx: usize) -> Option<usize> {
        let mut current = self.states.get(idx)?.first_in_edge()?;
        loop {
            assert!(
                current < self.edges.len(),
                "Edge with id {current} does not exist"
            );
            if let Some(x) = self.edges[current].in_next {
                current = x;
            } else {
                return Some(current);
            }
        }
    }

    /// Builds a new [`crate::NTS`] from its constituent parts.
    ///
    /// This assumes that no vacant entries exist!
    pub fn from_parts(
        alphabet: A,
        states: Vec<LinkedListTransitionSystemState<Q>>,
        edges: Vec<LinkedListTransitionSystemEdge<A::Expression, C>>,
    ) -> Self {
        debug_assert!(states.iter().all(|s| s.is_occupied()));
        Self {
            alphabet,
            states,
            edges,
            vacant: None,
        }
    }

    /// Decomposes `self` into a tuple of its constituents.
    pub fn into_parts(self) -> IntoLinkedListNondeterministic<Self> {
        (self.alphabet, self.states, self.edges)
    }

    #[cfg(debug_assertions)]
    pub(crate) fn verify_state(&self) {
        for i in 0..self.states.len() {
            if let Some((_, first_out, first_in)) = self.states[i].occupation() {
                if let Some(first_out) = first_out {
                    assert!(first_out < self.edges.len());
                }
                if let Some(first_in) = first_in {
                    assert!(first_in < self.edges.len());
                }
            }
        }
        for i in 0..self.edges.len() {
            if let Some(in_prev) = self.edges[i].in_prev {
                assert!(in_prev < self.edges.len());
                assert_eq!(self.edges[in_prev].in_next, Some(i));
                assert_eq!(self.edges[in_prev].target, self.edges[i].target);
            }
            if let Some(out_prev) = self.edges[i].out_prev {
                assert!(out_prev < self.edges.len());
                assert_eq!(self.edges[out_prev].out_next, Some(i));
                assert_eq!(self.edges[out_prev].source, self.edges[i].source);
            }
            if let Some(in_next) = self.edges[i].in_next {
                assert!(in_next < self.edges.len());
                assert_eq!(self.edges[in_next].in_prev, Some(i));
                assert_eq!(self.edges[in_next].target, self.edges[i].target);
            }
            if let Some(out_next) = self.edges[i].out_next {
                assert!(out_next < self.edges.len());
                assert_eq!(self.edges[out_next].out_prev, Some(i));
                assert_eq!(self.edges[out_next].source, self.edges[i].source);
            }
        }
    }
    #[cfg(not(debug_assertions))]
    pub(crate) fn verify_state(&self) {}
}

impl<A: Alphabet, Q: Color, C: Color, const DET: bool, X: Color>
    LinkedListTransitionSystem<A, (X, Q), C, DET>
{
    pub fn unzip_state_color(self) -> LinkedListTransitionSystem<A, Q, C, DET> {
        LinkedListTransitionSystem {
            alphabet: self.alphabet,
            edges: self.edges,
            states: self.states.into_iter().map(|q| q.unzip()).collect(),
            vacant: self.vacant,
        }
    }

    pub fn position_by_first(self, first: X) -> Option<usize> {
        self.states
            .iter()
            .position(|s| s.is_occupied() && s.occupation().unwrap().0.0.eq(&first))
    }

    pub fn old_to_new(&self, left: X) -> Option<StateIndex<Self>> {
        self.state_indices_with_color()
            .find_map(|(i, c)| if c.0.eq(&left) { Some(i) } else { None })
    }

    pub fn new_to_old(&self, right: StateIndex<Self>) -> Option<X> {
        self.state_color(right).map(|c| c.0)
    }
}

/// Type alias for the constituent parts of an [`crate::NTS`] with the same associated types as the
/// transition sytem `D`.
pub type IntoLinkedListNondeterministic<D> = (
    <D as TransitionSystem>::Alphabet,
    Vec<LinkedListTransitionSystemState<StateColor<D>>>,
    Vec<LinkedListTransitionSystemEdge<EdgeExpression<D>, EdgeColor<D>>>,
);

pub type LinkedListNondeterministic<A = CharAlphabet, Q = Void, C = Void> =
    LinkedListTransitionSystem<A, Q, C, false>;
pub type LinkedListDeterministic<A = CharAlphabet, Q = Void, C = Void> =
    LinkedListTransitionSystem<A, Q, C, true>;

/// Type alias to create a deterministic transition with the same alphabet, state and edge color
/// as the given [`Ts`](`crate::TransitionSystem`).
pub type CollectLinkedList<Ts, const DET: bool = true> = LinkedListTransitionSystem<
    <Ts as TransitionSystem>::Alphabet,
    <Ts as TransitionSystem>::StateColor,
    <Ts as TransitionSystem>::EdgeColor,
    DET,
>;

impl<A: Alphabet, Q: Color, C: Color> Deterministic for LinkedListTransitionSystem<A, Q, C> {}

impl<A: Alphabet, Q: Color, C: Color, const DET: bool> Sproutable
    for LinkedListTransitionSystem<A, Q, C, DET>
{
    fn add_state(&mut self, color: StateColor<Self>) -> Self::StateIndex {
        let id = self.states.len();
        let state = LinkedListTransitionSystemState::new(color);
        self.states.push(state);
        id.try_into().unwrap()
    }
    fn set_state_color(&mut self, index: StateIndex<Self>, color: StateColor<Self>) {
        if index >= self.states.len().try_into().unwrap() {
            panic!(
                "Index {index} is out of bounds, there are only {} states",
                self.states.len()
            );
        }
        self.states[index.into_usize()].set_color(color);
    }

    fn add_edge<E>(&mut self, t: E) -> Option<crate::ts::EdgeTuple<Self>>
    where
        E: IntoEdgeTuple<Self>,
    {
        let (q, a, c, p) = t.into_edge_tuple();

        let mut out = None;
        if DET && let Some(pos) = self.out_edge_position(q.into_usize(), &a) {
            trace!("found previously existing edge {pos} in deterministic automaton");
            out = Some(self.swap_remove_edge(pos).unwrap());
        }

        let mut edge = LinkedListTransitionSystemEdge::new(q, a, c, p);
        let edge_id = self.edges.len();

        let q = q.try_into().unwrap();
        let p = p.try_into().unwrap();
        assert!(
            q < self.states.len(),
            "cannot add edge from non-existent state {q:?}"
        );
        assert!(
            p < self.states.len(),
            "cannot add edge to non-existent state {q:?}"
        );
        if let Some(first_out) = self.first_out_edge(q) {
            assert!(first_out < self.edges.len());
            assert!(self.edges[first_out].out_prev.is_none());

            self.edges[first_out].out_prev = Some(edge_id);
            edge.out_next = Some(first_out);
        } else {
            assert!(self.states[q].first_out_edge().is_none());
        }
        self.states[q].set_first_out_edge(Some(edge_id));

        if let Some(first_in) = self.first_in_edge(p) {
            assert!(first_in < self.edges.len());
            assert!(self.edges[first_in].in_prev.is_none());

            self.edges[first_in].in_prev = Some(edge_id);
            edge.in_next = Some(first_in);
        } else {
            assert!(self.states[p].first_in_edge().is_none());
        }
        self.states[p].set_first_in_edge(Some(edge_id));

        self.edges.push(edge);
        out
    }
}

impl<A: Alphabet, Q: Color, C: Color, const DET: bool> Shrinkable
    for LinkedListTransitionSystem<A, Q, C, DET>
{
    fn remove_edges_from_matching(
        &mut self,
        source: StateIndex<Self>,
        matcher: impl Matcher<EdgeExpression<Self>>,
    ) -> Option<Vec<crate::ts::EdgeTuple<Self>>> {
        let id = source.into_usize();
        if id >= self.states.len() || self.states[id].is_vacant() {
            trace!("cannot remove edges from state that does not exist");
            return None;
        }
        Some(self.filter_swap_remove_edges(|q, e, _, _| q == source && matcher.matches(e)))
    }
    fn remove_state(&mut self, state: StateIndex<Self>) -> Option<Self::StateColor> {
        self.linked_remove_state(state.into_usize())
    }

    fn remove_edges_between_matching(
        &mut self,
        source: StateIndex<Self>,
        target: StateIndex<Self>,
        matcher: impl Matcher<EdgeExpression<Self>>,
    ) -> Option<Vec<crate::ts::EdgeTuple<Self>>> {
        let id = source.into_usize();
        if id >= self.states.len() || self.states[id].is_vacant() {
            trace!("cannot remove edges from state that does not exist");
            return None;
        }
        let id = target.into_usize();
        if id >= self.states.len() || self.states[id].is_vacant() {
            trace!("cannot remove edges to state that does not exist");
            return None;
        }
        Some(self.filter_swap_remove_edges(|q, e, _, p| {
            q == source && p == target && matcher.matches(e)
        }))
    }

    fn remove_edges_between(
        &mut self,
        source: StateIndex<Self>,
        target: StateIndex<Self>,
    ) -> Option<Vec<crate::ts::EdgeTuple<Self>>> {
        let id = source.into_usize();
        if id >= self.states.len() || self.states[id].is_vacant() {
            trace!("cannot remove edges from state that does not exist");
            return None;
        }
        let id = target.into_usize();
        if id >= self.states.len() || self.states[id].is_vacant() {
            trace!("cannot remove edges to state that does not exist");
            return None;
        }
        Some(self.filter_swap_remove_edges(|q, _, _, p| q == source && p == target))
    }

    fn remove_edges_from(
        &mut self,
        source: StateIndex<Self>,
    ) -> Option<Vec<crate::ts::EdgeTuple<Self>>> {
        let id = source.into_usize();
        if id >= self.states.len() || self.states[id].is_vacant() {
            trace!("cannot remove edges from state that does not exist");
            return None;
        }
        Some(self.filter_swap_remove_edges(|q, _, _, _| q == source))
    }

    fn remove_edges_to(
        &mut self,
        target: StateIndex<Self>,
    ) -> Option<Vec<crate::ts::EdgeTuple<Self>>> {
        let id = target.into_usize();
        if id >= self.states.len() || self.states[id].is_vacant() {
            trace!("cannot remove edges to state that does not exist");
            return None;
        }
        Some(self.filter_swap_remove_edges(|_, _, _, p| p == target))
    }
}

impl<Q, C, const DET: bool> LinkedListTransitionSystem<CharAlphabet, Q, C, DET> {
    /// Returns a transition system builder for a non-deterministic transition system. This should be the main method for the
    /// construction of non-deterministic transition systems on the fly.
    ///
    /// # Example
    ///
    /// We want to create a DFA with two states 0 and 1 over the alphabet `['a', 'b']`. We want to add the following transitions:
    /// - From state 0 to state 0 on symbol 'a'
    /// - From state 0 to state 1 on symbol 'b'
    /// - From state 1 to state 1 on symbol 'a'
    /// - From state 1 to state 0 on symbol 'b'
    ///
    /// Further, state 0 should be initial and colored `true` and state 1 should be colored `false`. This can be done as follows
    /// ```
    /// use automata::{core::Void, ts::TSBuilder};
    ///
    /// let ts = TSBuilder::default()
    ///     .with_state_colors([true, false]) // colors given in the order of the states
    ///     .with_transitions([(0, 'a', Void, 0), (0, 'b', Void, 1), (1, 'a', Void, 1), (1, 'b', Void, 0)])
    ///     .into_dfa(0); // 0 is the initial state
    /// ```
    pub fn builder() -> TSBuilder<Q, C> {
        TSBuilder::default()
    }
}

impl<A: Alphabet, Q: Color, C: Color, const DET: bool> TransitionSystem
    for LinkedListTransitionSystem<A, Q, C, DET>
{
    type StateIndex = DefaultIdType;

    type StateColor = Q;

    type EdgeColor = C;

    type EdgeRef<'this>
        = &'this LinkedListTransitionSystemEdge<A::Expression, C>
    where
        Self: 'this;

    type EdgesFromIter<'this>
        = NTSEdgesFromIter<'this, A::Expression, C>
    where
        Self: 'this;

    type StateIndices<'this>
        = LinkedStateIndices<'this, Q>
    where
        Self: 'this;

    type Alphabet = A;

    fn alphabet(&self) -> &Self::Alphabet {
        &self.alphabet
    }

    fn state_indices(&self) -> Self::StateIndices<'_> {
        LinkedStateIndices(&self.states, 0)
    }

    fn edges_from(&self, state: StateIndex<Self>) -> Option<Self::EdgesFromIter<'_>> {
        Some(NTSEdgesFromIter::new(
            &self.edges,
            self.first_out_edge(state.into_usize()),
        ))
    }

    fn state_color(&self, state: StateIndex<Self>) -> Option<Self::StateColor> {
        if state.into_usize() >= self.states.len() {
            panic!(
                "index {state} is out of bounds, there are only {} states",
                self.states.len()
            );
        }
        assert!(state.into_usize() < self.states.len());
        self.states
            .get(state.into_usize())
            .map(|x| x.color().unwrap().clone())
    }
}

pub struct LinkedStateIndices<'ts, Q: Color>(&'ts [LinkedListTransitionSystemState<Q>], usize);

impl<Q: Color> Iterator for LinkedStateIndices<'_, Q> {
    type Item = DefaultIdType;
    fn next(&mut self) -> Option<Self::Item> {
        while let Some(state) = self.0.get(self.1) {
            self.1 += 1;
            if state.is_occupied() {
                return Some(ScalarIndexType::from_usize(self.1 - 1));
            }
        }
        None
    }
}

impl<A: Alphabet, Q: Color, C: Color, const DET: bool> ForAlphabet<A>
    for LinkedListTransitionSystem<A, Q, C, DET>
{
    fn for_alphabet(alphabet: A) -> Self {
        Self {
            alphabet,
            states: vec![],
            edges: vec![],
            vacant: None,
        }
    }

    fn for_alphabet_size_hint(from: A, size_hint: usize) -> Self {
        Self {
            alphabet: from,
            states: Vec::with_capacity(size_hint),
            edges: vec![],
            vacant: None,
        }
    }
}

impl<A: Alphabet, Q: Color, C: Color, const DET: bool> PredecessorIterable
    for LinkedListTransitionSystem<A, Q, C, DET>
{
    type PreEdgeRef<'this>
        = &'this LinkedListTransitionSystemEdge<A::Expression, C>
    where
        Self: 'this;

    type EdgesToIter<'this>
        = LinkedListTransitionSystemEdgesToIter<'this, A::Expression, C, DET>
    where
        Self: 'this;

    fn predecessors(&self, state: StateIndex<Self>) -> Option<Self::EdgesToIter<'_>> {
        assert!(state.into_usize() < self.states.len());

        Some(LinkedListTransitionSystemEdgesToIter::new(
            self,
            state.into_usize(),
        ))
    }
}

impl<A: Alphabet + PartialEq, Q: Hash + Debug + Eq, C: Hash + Debug + Eq, const DET: bool> PartialEq
    for LinkedListTransitionSystem<A, Q, C, DET>
{
    fn eq(&self, other: &Self) -> bool {
        if self.alphabet != other.alphabet {
            trace!("alphabets do not match");
            return false;
        }
        if self.states.len() != other.states.len() {
            trace!("number of states does not match");
            return false;
        }

        self.states.iter().zip(other.states.iter()).all(|(x, y)| {
            if x.is_vacant() || y.is_vacant() {
                return false;
            }
            if x.color() != y.color() {
                return false;
            }
            let edges = x
                .edges_from_iter(&self.edges)
                .expect("the state exists")
                .map(|e| (&e.expression, &e.color, e.target))
                .collect::<math::Set<_>>();

            y.edges_from_iter(&other.edges)
                .expect("state exists")
                .all(|e| edges.contains(&(&e.expression, &e.color, e.target)))
        })
    }
}

impl<A: Alphabet + PartialEq, Q: Hash + Debug + Eq, C: Hash + Debug + Eq, const DET: bool> Eq
    for LinkedListTransitionSystem<A, Q, C, DET>
{
}

impl<A: Alphabet + std::fmt::Debug, Q: Color, C: Color, const DET: bool> std::fmt::Debug
    for LinkedListTransitionSystem<A, Q, C, DET>
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for q in self.state_indices() {
            write!(f, "{q} : {:?} |", self.state_color(q).unwrap())?;
            let Some(it) = self.edges_from(q) else {
                continue;
            };
            for e in it {
                write!(
                    f,
                    " {}/{:?}->{}",
                    e.expression().show(),
                    IsEdge::color(&e),
                    e.target()
                )?;
            }
            writeln!(f)?;
        }
        Ok(())
    }
}

fn recast<A: Alphabet, Q: Color, C: Color, const DET: bool, const OUT_DET: bool>(
    ts: LinkedListTransitionSystem<A, Q, C, DET>,
) -> LinkedListTransitionSystem<A, Q, C, OUT_DET> {
    if !DET && OUT_DET && !ts.is_deterministic() {
        panic!("cannot convert non-deterministic transition system to deterministic");
    }
    let LinkedListTransitionSystem {
        alphabet,
        states,
        edges,
        vacant,
    } = ts;
    LinkedListTransitionSystem {
        alphabet,
        states,
        edges,
        vacant,
    }
}

#[cfg(test)]
mod tests {
    use crate::ts::{Deterministic, ForAlphabet, Sproutable, TSBuilder};
    use crate::{TransitionSystem, ts::LinkedListDeterministic};
    use automata_core::alphabet::CharAlphabet;

    #[test]
    fn dts_equality() {
        let first = TSBuilder::default()
            .with_state_colors([0, 1])
            .with_edges([
                (0, 'a', 100u32, 0),
                (0, 'b', 1000, 1),
                (1, 'a', 200, 0),
                (1, 'b', 2000, 1),
            ])
            .into_dts();
        let second = TSBuilder::default()
            .with_state_colors([0, 1])
            .with_edges([
                (0, 'b', 1000, 1),
                (1, 'b', 2000, 1),
                (0, 'a', 100u32, 0),
                (1, 'a', 200, 0),
            ])
            .into_dts();
        assert!(first == second, "equality should disregard order of edges");
    }

    #[test]
    fn remove_edge() {
        let mut ts = LinkedListDeterministic::for_alphabet(CharAlphabet::of_size(2));

        ts.add_state(0);
        ts.add_state(1);
        ts.add_edge((0, 'a', 1));
        ts.add_edge((1, 'a', 0));
        ts.add_edge((0, 'b', 0));
        ts.add_edge((1, 'b', 1));

        assert!(ts.swap_remove_edge(0).is_some());
        assert_eq!(ts.successor_index(0, 'a'), None);

        assert_eq!(ts.add_edge((0, 'a', 0)), None);
        assert_eq!(
            ts.reachable_state_indices_from(0).collect::<Vec<_>>(),
            vec![0]
        );
    }
}
