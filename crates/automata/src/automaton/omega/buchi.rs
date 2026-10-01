use crate::automaton::Semantics;
use crate::representation::{CollectTs, IntoTs};
use crate::ts::{Deterministic, StateColor, SymbolOf};
use crate::{DTS, TransitionSystem, automaton::InfiniteWordAutomaton, ts::run};
use automata_core::Void;
use automata_core::alphabet::CharAlphabet;
use automata_core::word::ReducedOmegaWord;

/// Defines the [`Semantics`] of a deterministic Büchi automaton (DBA),
/// which is an acceptor of infinite words. It considers the set of
/// transitions which is taken infinitely often. If one of those is
/// colored with `true`, then the automaton accepts, while if all of the
/// transitions that are taken infinitely often are colored with `false`,
/// the automaton rejects.
///
/// This type will rarely be used on its own, for the automaton that makes
/// use of it, see [`DBA`].
#[derive(Debug, Default, Clone, Eq, PartialEq, Hash, Copy)]
pub struct BuchiCondition;

impl<T: Deterministic<EdgeColor = bool>> Semantics<T, true> for BuchiCondition {
    type Observer = run::GreatestEdgeColor<T>;
    type Output = bool;
    fn evaluate(&self, observed: <Self::Observer as run::Observer<T>>::Current) -> Self::Output {
        observed
    }
}

/// A deterministic Büchi automaton (DBA) is a deterministic automaton with Büchi acceptance condition.
/// It accepts a word if it has a successful infinite run that takes an accepting
/// transition (i.e. one that is labeled with `true`) infinitely often.
///
/// In a certain sense, it is a special case of a deterministic parity automaton [`super::DPA`] with
/// min even and priorities 0 and 1.
pub type DBA<A = CharAlphabet, Q = Void, D = DTS<A, Q, bool>> =
    InfiniteWordAutomaton<A, BuchiCondition, Q, bool, true, D>;
/// Helper trait for creating a [`DBA`] from a given transition system.
pub type IntoDBA<T> = DBA<<T as TransitionSystem>::Alphabet, StateColor<T>, T>;

impl<C> IntoDBA<C>
where
    C: Deterministic<EdgeColor = bool>,
{
    /// Performs a streamlining operation akin to [`super::DPA::streamlined`].
    pub fn streamlined(&self) -> DBA<C::Alphabet>
    where
        C: IntoTs + Clone,
    {
        self.clone()
            .map_edge_colors(|c| if c { 0 } else { 1 })
            .with_initial(self.initial)
            .into_dpa()
            .streamlined()
            .map_edge_colors(|c| {
                assert!(c == 0 || c == 1, "too many colors");
                c % 2 == 0
            })
            .collect_dba()
    }

    /// Tries to identify a word which is accepted by `self`. If such a word exists, it returns it and otherwise
    /// the function gives back `None`.
    pub fn give_word(&self) -> Option<ReducedOmegaWord<SymbolOf<Self>>> {
        for (_, good_scc) in self
            .sccs()
            .iter()
            .filter(|(_, scc)| !scc.is_empty() && self.is_reachable_from(self.initial, scc.first()))
        {
            if let Some(full_word) = good_scc.maximal_word() {
                // let InfinityColors(colors) = self
                //     .induced(&full_word, self.initial())
                //     .expect("word is valid");
                if let Some(infset) = self.visited_edge_colors_from(good_scc.first(), &full_word)
                    && infset.iter().any(|b| *b)
                {
                    let spoke = self
                        .word_from_to(self.initial, good_scc.first())
                        .expect("We know this is reachable!");
                    return Some(ReducedOmegaWord::ultimately_periodic(spoke, full_word));
                }
                // if colors.contains(&true) {
                //     let base = self
                //         .word_from_to(self.initial(), *good_scc.first().unwrap())
                //         .expect("we know this is reachable");
                //     return Some(OmegaWord::from_parts(base, full_word));
            }
        }
        None
    }

    /// Returns `true` if and only if `self` accepts a non-empty language.
    pub fn is_empty(&self) -> bool {
        self.give_word().is_none()
    }
}
