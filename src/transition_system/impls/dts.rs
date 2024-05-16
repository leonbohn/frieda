use std::{fmt::Debug, hash::Hash};

use crate::{prelude::*, Void};

use super::nts::{NTEdge, NTSEdgesFromIter, NTSEdgesToIter, NTSPartsFor, NTState};

/// A deterministic transition system. This is a thin wrapper around [`NTS`] and is only used to
/// enforce that the underlying NTS is deterministic.
#[derive(Clone)]
pub struct DTS<A: Alphabet = CharAlphabet, Q = Void, C = Void>(pub(crate) NTS<A, Q, C>);

/// Type alias that is used when we want a pair (tuple) consisiting of a DTS and its initial
/// state. This is mainly used to make clippy happy...
pub type DTSAndInitialState<A, Q, C> = (DTS<A, Q, C>, usize);

/// Type alias to create a deterministic transition with the same alphabet, state and edge color
/// as the given [`Ts`](`crate::prelude::TransitionSystem`).
pub type CollectDTS<Ts> = DTS<
    <Ts as TransitionSystem>::Alphabet,
    <Ts as TransitionSystem>::StateColor,
    <Ts as TransitionSystem>::EdgeColor,
>;

impl<A: Alphabet, Q: Clone, C: Clone> Deterministic for DTS<A, Q, C> {}

impl<A: Alphabet, Q: Clone, C: Clone> TransitionSystem for DTS<A, Q, C> {
    type StateIndex = usize;

    type StateColor = Q;

    type EdgeColor = C;

    type EdgeRef<'this> = &'this NTEdge<A::Expression, C>
    where
        Self: 'this;

    type EdgesFromIter<'this> = NTSEdgesFromIter<'this, A::Expression, C>
    where
        Self: 'this;

    type StateIndices<'this> = std::ops::Range<usize>
    where
        Self: 'this;

    type Alphabet = A;

    fn alphabet(&self) -> &Self::Alphabet {
        self.0.alphabet()
    }

    fn state_indices(&self) -> Self::StateIndices<'_> {
        self.0.state_indices()
    }

    fn edges_from<Idx: Indexes<Self>>(&self, state: Idx) -> Option<Self::EdgesFromIter<'_>> {
        self.0.edges_from(state.to_index(self)?)
    }
    fn state_color<Idx: Indexes<Self>>(&self, state: Idx) -> Option<Self::StateColor> {
        self.0.state_color(state.to_index(self)?)
    }
}

impl<A: Alphabet, Q: Clone, C: Clone> ForAlphabet for DTS<A, Q, C> {
    fn for_alphabet(from: A) -> Self {
        Self(NTS::for_alphabet(from))
    }
}

impl<A: Alphabet, Q: Clone, C: Clone> PredecessorIterable for DTS<A, Q, C> {
    type PreEdgeRef<'this> = &'this NTEdge<A::Expression, C>
    where
        Self: 'this;

    type EdgesToIter<'this> = NTSEdgesToIter<'this, A::Expression, C>
    where
        Self: 'this;

    fn predecessors<Idx: Indexes<Self>>(&self, state: Idx) -> Option<Self::EdgesToIter<'_>> {
        self.0.predecessors(state.to_index(self)?)
    }
}

impl<A: Alphabet, Q: Clone, C: Clone> Sproutable for DTS<A, Q, C> {
    fn add_state<X: Into<StateColor<Self>>>(&mut self, color: X) -> Self::StateIndex {
        self.0.add_state(color)
    }

    fn set_state_color<Idx: Indexes<Self>, X: Into<StateColor<Self>>>(
        &mut self,
        index: Idx,
        color: X,
    ) {
        let Some(index) = index.to_index(self) else {
            tracing::error!("cannot set color of state that does not exist");
            return;
        };
        self.0.set_state_color(index, color)
    }

    fn add_edge<E>(&mut self, t: E) -> Option<(Self::StateIndex, Self::EdgeColor)>
    where
        E: IntoEdgeTuple<Self>,
    {
        self.0.add_edge(t.into_edge_tuple())
    }

    fn remove_edges<X>(&mut self, from: X, on: <Self::Alphabet as Alphabet>::Expression) -> bool
    where
        X: Indexes<Self>,
    {
        from.to_index(self)
            .map(|idx| self.0.remove_edges(idx, on))
            .unwrap_or(false)
    }
}

impl<A: Alphabet, Q: Clone, C: Clone> DTS<A, Q, C> {
    /// Creates an empty [`DTS`] with the given alphabet and capacity for at least `cap` states.
    pub fn with_capacity(alphabet: A, cap: usize) -> Self {
        Self(NTS::with_capacity(alphabet, cap))
    }

    /// Decomposes and consumes `self` to build a tuple of the constituent parts.
    pub fn into_parts(self) -> NTSPartsFor<Self> {
        self.0.into_parts()
    }

    /// Constructs a new instance from the parts making up a [`NTS`], for more information see
    /// [`NTS::from_parts()`].
    pub fn from_parts(
        alphabet: A,
        states: Vec<NTState<Q>>,
        edges: Vec<NTEdge<A::Expression, C>>,
    ) -> Self {
        NTS::from_parts(alphabet, states, edges).into_deterministic()
    }
}

impl<A: Alphabet, Q: Clone, C: Clone> TryFrom<NTS<A, Q, C>> for DTS<A, Q, C> {
    type Error = ();

    fn try_from(value: NTS<A, Q, C>) -> Result<Self, Self::Error> {
        if !value.is_deterministic() {
            return Err(());
        }
        Ok(Self(value))
    }
}

impl<A: Alphabet, Q: Clone, C: Clone> TryFrom<&NTS<A, Q, C>> for DTS<A, Q, C> {
    type Error = ();

    fn try_from(value: &NTS<A, Q, C>) -> Result<Self, Self::Error> {
        if !value.is_deterministic() {
            return Err(());
        }
        Ok(Self(value.clone()))
    }
}

impl<A: Alphabet, Q: Clone, C: Clone> From<DTS<A, Q, C>> for NTS<A, Q, C> {
    fn from(value: DTS<A, Q, C>) -> Self {
        value.0
    }
}

impl<A: Alphabet + PartialEq, Q: Hash + Eq, C: Hash + Eq> PartialEq for DTS<A, Q, C> {
    fn eq(&self, other: &Self) -> bool {
        self.0.eq(&other.0)
    }
}

impl<A: Alphabet + PartialEq, Q: Hash + Eq, C: Hash + Eq> Eq for DTS<A, Q, C> {}

impl<A: Alphabet, Q: Clone + Debug, C: Clone + Debug> std::fmt::Debug for DTS<A, Q, C> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            self.build_transition_table(
                |q, c| format!("{q}|{c:?}"),
                |edge| format!("{:?}->{}", edge.color(), edge.target())
            )
        )
    }
}
