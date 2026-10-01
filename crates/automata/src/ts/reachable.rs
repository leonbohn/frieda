use crate::TransitionSystem;
use crate::congruence::MinimalRepresentative;
use crate::core::{alphabet::Expression, math::OrderedSet};
use crate::ts::{IsEdge, StateIndex};
use std::collections::{BTreeMap, VecDeque};

/// Struct that can return the minimal representatives of a transition system. A minimal representative
/// for a state `q` of some transition system is the length-lexicographically minimal string with which
/// `q` can be reached from a given state.
#[derive(Debug, Clone)]
pub struct LengthLexicographicMinimalRepresentatives<'a, Ts: TransitionSystem> {
    ts: &'a Ts,
    seen: OrderedSet<Ts::StateIndex>,
    queue: BTreeMap<MinimalRepresentative<Ts>, StateIndex<Ts>>,
}

#[allow(missing_docs)]
impl<'a, Ts> LengthLexicographicMinimalRepresentatives<'a, Ts>
where
    Ts: TransitionSystem,
{
    pub fn new(ts: &'a Ts, origin: Ts::StateIndex) -> Self {
        let seen = OrderedSet::from_iter([origin]);
        let queue = [(MinimalRepresentative::new(vec![], origin), origin)]
            .into_iter()
            .collect();
        Self { ts, seen, queue }
    }
}

impl<Ts> Iterator for LengthLexicographicMinimalRepresentatives<'_, Ts>
where
    Ts: TransitionSystem,
{
    type Item = MinimalRepresentative<Ts>;

    fn next(&mut self) -> Option<Self::Item> {
        if let Some((access, q)) = self.queue.pop_first() {
            if let Some(it) = self.ts.edges_from(q) {
                for edge in it {
                    let p = edge.target();
                    if self.seen.insert(p) {
                        for sym in edge.expression().symbols() {
                            let mut new_access = access.clone();
                            new_access.push(sym);
                            self.queue
                                .insert(MinimalRepresentative::new(new_access, p), p);
                        }
                    }
                }
            }
            return Some(access);
        }
        None
    }
}

/// Allows iterating over the reachable states of a transition system.
pub struct Reachable<'a, Ts: TransitionSystem, const FULL_EDGE: bool = false> {
    ts: &'a Ts,
    seen: OrderedSet<Ts::StateIndex>,
    it: Ts::EdgesFromIter<'a>,
    queue: VecDeque<StateIndex<Ts>>,
}

impl<'a, Ts, const FULL_EDGE: bool> Reachable<'a, Ts, FULL_EDGE>
where
    Ts: TransitionSystem,
{
    /// Creates a new iterator that will yield the reachable states of the transition system starting
    pub fn new(ts: &'a Ts, origin: Ts::StateIndex) -> Self {
        let mut seen = OrderedSet::new();
        seen.insert(origin);
        let mut queue = VecDeque::with_capacity(ts.hint_size().0);
        queue.push_front(origin);
        Self {
            seen,
            ts,
            it: ts.edges_from(origin).expect("origin state does not exist"),
            queue,
        }
    }
}

// Iterator that does not output the color of the states.

impl<'a, Ts> Reachable<'a, Ts, false>
where
    Ts: TransitionSystem,
{
    /// Creates a new variant that only outputs the indices of reachable vertices.
    pub fn state_indices(ts: &'a Ts, origin: Ts::StateIndex) -> Self {
        Self::new(ts, origin)
    }
}

impl<Ts> Iterator for Reachable<'_, Ts, false>
where
    Ts: TransitionSystem,
{
    type Item = Ts::StateIndex;

    fn next(&mut self) -> Option<Self::Item> {
        let q = self.queue.pop_front()?;
        let Some(it) = self.ts.edges_from(q) else {
            panic!("state does not exist");
        };
        for edge in it {
            if self.seen.insert(edge.target()) {
                self.queue.push_back(edge.target());
            }
        }
        Some(q)
    }
}

// One that outputs the color of the states.

impl<'a, Ts> Reachable<'a, Ts, true>
where
    Ts: TransitionSystem,
{
    /// Creates a new instance of the variant of `Self` that outputs full edges.
    pub fn edges(ts: &'a Ts, origin: Ts::StateIndex) -> Self {
        Self::new(ts, origin)
    }
}

impl<'a, Ts> Iterator for Reachable<'a, Ts, true>
where
    Ts: TransitionSystem,
{
    type Item = Ts::EdgeRef<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if let Some(edge) = self.it.next() {
                if self.seen.insert(edge.target()) {
                    self.queue.push_back(edge.target());
                }
                return Some(edge);
            }

            let q = self.queue.pop_front()?;
            self.it = self.ts.edges_from(q).expect("state does not exist");
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::TransitionSystem;
    use crate::automaton::DFA;
    use crate::congruence::MinimalRepresentative;
    use crate::core::word::FiniteWord;
    use itertools::Itertools;

    #[test]
    fn reachable_states() {
        let dfa = DFA::builder()
            .with_state_colors([false, false, true])
            .with_edges([
                (0, 'a', 1),
                (0, 'b', 0),
                (1, 'a', 2),
                (1, 'b', 0),
                (2, 'a', 2),
                (2, 'b', 2),
            ])
            .into_dfa(0);

        assert_eq!(
            dfa.minimal_representatives_iter_from(0).collect::<Vec<_>>(),
            vec![
                MinimalRepresentative::new("".collect_vec(), 0u32),
                MinimalRepresentative::new("a".collect_vec(), 1),
                MinimalRepresentative::new("aa".collect_vec(), 2)
            ]
        );
        assert_eq!(dfa.reachable_state_indices().collect_vec(), vec![0, 1, 2]);
        assert_eq!(dfa.reachable_state_indices().collect_vec(), vec![0, 1, 2]);

        assert_eq!(dfa.reachable_state_indices_from(2).collect_vec(), vec![2]);
    }
}
