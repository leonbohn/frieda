use bit_set::BitSet;
use itertools::Itertools;
use tracing::warn;

use crate::{hoa::HoaAlphabet, prelude::*, Set};

use super::Initialized;

#[derive(Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct AcceptanceMask(BitSet);

impl AcceptanceMask {
    pub fn max(&self) -> Option<usize> {
        self.iter().max()
    }

    pub fn iter(&self) -> impl Iterator<Item = usize> + '_ {
        self.0.iter()
    }

    pub fn min(&self) -> Option<usize> {
        self.iter().min()
    }

    pub fn try_as_priority(&self) -> Option<usize> {
        let mut it = self.iter();
        let Some(priority) = it.next() else {
            return None;
        };
        if it.next().is_some() {
            warn!("more than one priority is set! {:?}", self.0);
        }
        Some(priority)
    }

    pub fn as_priority(&self) -> usize {
        self.try_as_priority().unwrap()
    }
}

impl Show for AcceptanceMask {
    fn show(&self) -> String {
        self.iter().map(|i| format!("{{{i}}}")).join(", ")
    }
}

impl From<&hoars::AcceptanceSignature> for AcceptanceMask {
    fn from(value: &hoars::AcceptanceSignature) -> Self {
        Self(BitSet::from_iter(value.iter().map(|&i| {
            i.try_into().expect("Could not cast {i} to usize")
        })))
    }
}

#[derive(Debug, Clone, Eq, Copy, PartialEq, Ord, PartialOrd)]
pub enum OmegaAcceptanceCondition {
    Parity(usize, usize),
    Buchi,
    Rabin,
    Streett,
    MaxParity,
    CoBuchi,
    Reachability,
    Safety,
}

impl OmegaAcceptanceCondition {
    pub fn satisfied(&self, infset: &Set<AcceptanceMask>) -> bool {
        match self {
            OmegaAcceptanceCondition::Parity(_low, _high) => infset
                .iter()
                .map(|x| x.as_priority())
                .min()
                .map(|x| x % 2 == 0)
                .unwrap_or(false),
            _ => unimplemented!(),
        }
    }
}

pub struct OmegaAutomaton<A: Alphabet> {
    pub(super) ts: Initialized<NTS<A, usize, AcceptanceMask>>,
    pub(super) acc: OmegaAcceptanceCondition,
}

pub struct DeterministicOmegaAutomaton<A: Alphabet> {
    pub(super) ts: Initialized<DTS<A, usize, AcceptanceMask>>,
    #[allow(unused)]
    pub(super) acc: OmegaAcceptanceCondition,
}

impl<A: Alphabet> OmegaAutomaton<A> {
    pub fn new(
        ts: Initialized<NTS<A, usize, AcceptanceMask>>,
        acc: OmegaAcceptanceCondition,
    ) -> Self {
        Self { ts, acc }
    }

    pub fn into_deterministic(self) -> Option<DeterministicOmegaAutomaton<A>> {
        self.try_into().ok()
    }
}

impl<A: Alphabet> DeterministicOmegaAutomaton<A> {
    pub fn new(
        ts: Initialized<DTS<A, usize, AcceptanceMask>>,
        acc: OmegaAcceptanceCondition,
    ) -> Self {
        Self { ts, acc }
    }

    pub fn into_dpa(self) -> DPA<A> {
        assert!(
            matches!(self.acc, OmegaAcceptanceCondition::Parity(_, _)),
            "Can only turn DPA into DPA for now"
        );

        let neutral = self
            .ts
            .edge_colors_unique()
            .filter_map(|mask| mask.try_as_priority())
            .max()
            .expect("Need at least one neutral color (the maximal one)");

        self.ts
            .map_edge_colors(|mask| mask.try_as_priority().unwrap_or(neutral))
            .collect_dpa()
    }
}

impl From<DeterministicOmegaAutomaton<HoaAlphabet>> for DeterministicOmegaAutomaton<CharAlphabet> {
    fn from(value: DeterministicOmegaAutomaton<HoaAlphabet>) -> Self {
        let ts = TSBuilder::default()
            .with_state_colors((0..value.size()).map(|i| value.state_color(i).unwrap()))
            .with_transitions(value.state_indices().flat_map(|q| {
                value.edges_from(q).unwrap().flat_map(|edge| {
                    edge.expression()
                        .symbols()
                        .map(move |sym| (edge.source(), sym.to_char(), edge.color(), edge.target()))
                })
            }))
            .into_deterministic_initialized(value.initial());
        DeterministicOmegaAutomaton::new(ts, value.acc)
    }
}

impl TryFrom<DeterministicOmegaAutomaton<CharAlphabet>>
    for DeterministicOmegaAutomaton<HoaAlphabet>
{
    /// For now, we allow this to error out in exactly one case: if the number of alphabet symbols
    /// is not a power of 2 and cannot be mapped immediately into AP combinations.
    type Error = ();
    fn try_from(value: DeterministicOmegaAutomaton<CharAlphabet>) -> Result<Self, Self::Error> {
        let alphabet_size = value.alphabet().size();
        if alphabet_size != alphabet_size.next_power_of_two() || alphabet_size == 0 {
            return Err(());
        }

        let aps = alphabet_size.ilog2() as usize;
        assert!(aps > 0, "We do not want this edge case");

        todo!()
    }
}

impl<A: Alphabet> TryFrom<OmegaAutomaton<A>> for DeterministicOmegaAutomaton<A> {
    /// The only way this can go wrong is if the given automaton is not deterministic.
    type Error = ();

    fn try_from(value: OmegaAutomaton<A>) -> Result<Self, Self::Error> {
        let dts = value.ts.try_into()?;
        Ok(Self::new(dts, value.acc))
    }
}

impl<A: Alphabet> TryFrom<&OmegaAutomaton<A>> for DeterministicOmegaAutomaton<A> {
    /// The only way this can go wrong is if the given automaton is not deterministic.
    type Error = ();

    fn try_from(value: &OmegaAutomaton<A>) -> Result<Self, Self::Error> {
        let dts = (&value.ts).try_into()?;
        Ok(Self::new(dts, value.acc))
    }
}

impl<A: Alphabet> Pointed for OmegaAutomaton<A> {
    fn initial(&self) -> Self::StateIndex {
        self.ts.initial()
    }
}

impl<A: Alphabet> Pointed for DeterministicOmegaAutomaton<A> {
    fn initial(&self) -> Self::StateIndex {
        self.ts.initial()
    }
}

impl<A: Alphabet> TransitionSystem for OmegaAutomaton<A> {
    type StateIndex = usize;

    type StateColor = usize;

    type EdgeColor = AcceptanceMask;

    type EdgeRef<'this> = <DTS<A, usize, AcceptanceMask> as TransitionSystem>::EdgeRef<'this>
    where
        Self: 'this;

    type EdgesFromIter<'this> = <DTS<A, usize, AcceptanceMask> as TransitionSystem>::EdgesFromIter<'this>
    where
        Self: 'this;

    type StateIndices<'this> = <DTS<A, usize, AcceptanceMask> as TransitionSystem>::StateIndices<'this>
    where
        Self: 'this;

    type Alphabet = A;

    fn alphabet(&self) -> &Self::Alphabet {
        self.ts.alphabet()
    }

    fn state_indices(&self) -> Self::StateIndices<'_> {
        self.ts.state_indices()
    }

    fn edges_from<Idx: Indexes<Self>>(&self, state: Idx) -> Option<Self::EdgesFromIter<'_>> {
        self.ts.edges_from(state.to_index(self)?)
    }

    fn state_color<Idx: Indexes<Self>>(&self, state: Idx) -> Option<Self::StateColor> {
        self.ts.state_color(state.to_index(self)?)
    }
}

impl<A: Alphabet> TransitionSystem for DeterministicOmegaAutomaton<A> {
    type StateIndex = usize;

    type StateColor = usize;

    type EdgeColor = AcceptanceMask;

    type EdgeRef<'this> = <DTS<A, usize, AcceptanceMask> as TransitionSystem>::EdgeRef<'this>
    where
        Self: 'this;

    type EdgesFromIter<'this> = <DTS<A, usize, AcceptanceMask> as TransitionSystem>::EdgesFromIter<'this>
    where
        Self: 'this;

    type StateIndices<'this> = <DTS<A, usize, AcceptanceMask> as TransitionSystem>::StateIndices<'this>
    where
        Self: 'this;

    type Alphabet = A;

    fn alphabet(&self) -> &Self::Alphabet {
        self.ts.alphabet()
    }

    fn state_indices(&self) -> Self::StateIndices<'_> {
        self.ts.state_indices()
    }

    fn edges_from<Idx: Indexes<Self>>(&self, state: Idx) -> Option<Self::EdgesFromIter<'_>> {
        self.ts.edges_from(state.to_index(self)?)
    }

    fn state_color<Idx: Indexes<Self>>(&self, state: Idx) -> Option<Self::StateColor> {
        self.ts.state_color(state.to_index(self)?)
    }
}

impl<A: Alphabet> PredecessorIterable for OmegaAutomaton<A> {
    type PreEdgeRef<'this> = <DTS<A, usize, AcceptanceMask> as PredecessorIterable>::PreEdgeRef<'this>
    where
        Self: 'this;

    type EdgesToIter<'this> =  <DTS<A, usize, AcceptanceMask> as PredecessorIterable>::EdgesToIter<'this>
    where
        Self: 'this;

    fn predecessors<Idx: Indexes<Self>>(&self, state: Idx) -> Option<Self::EdgesToIter<'_>> {
        self.ts.predecessors(state.to_index(self)?)
    }
}

impl<A: Alphabet> PredecessorIterable for DeterministicOmegaAutomaton<A> {
    type PreEdgeRef<'this> = <DTS<A, usize, AcceptanceMask> as PredecessorIterable>::PreEdgeRef<'this>
    where
        Self: 'this;

    type EdgesToIter<'this> =  <DTS<A, usize, AcceptanceMask> as PredecessorIterable>::EdgesToIter<'this>
    where
        Self: 'this;

    fn predecessors<Idx: Indexes<Self>>(&self, state: Idx) -> Option<Self::EdgesToIter<'_>> {
        self.ts.predecessors(state.to_index(self)?)
    }
}

impl<A: Alphabet> Deterministic for DeterministicOmegaAutomaton<A> {
    fn transition<Idx: Indexes<Self>>(
        &self,
        state: Idx,
        symbol: SymbolOf<Self>,
    ) -> Option<Self::EdgeRef<'_>> {
        self.ts.transition(state.to_index(self)?, symbol)
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn omega_acceptance_conditions() {}
}
