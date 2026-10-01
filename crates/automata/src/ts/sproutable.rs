use crate::{Pointed, TransitionSystem};
use automata_core::alphabet::{Alphabet, CharAlphabet};
use automata_core::math::Bijection;
use bit_set::BitSet;
use itertools::Itertools;

use super::{EdgeColor, EdgeTuple, IntoEdgeTuple, IsEdge, StateColor, StateIndex};

/// Implemented by [`TransitionSystem`]s (TS) that can be created for a given [`Alphabet`],
/// which is may be used in conjunction with [`Sproutable`] to successively grow a
/// TS.
///
/// Usually, this will *not* be implemented by TS that have a [designated initial state](`Pointed`).
/// For those, a dedicated method should be used.
pub trait ForAlphabet<A: Alphabet>: Sized {
    /// Creates an instance of `Self` for the given [`Alphabet`]. The resulting
    /// TS should be empty.
    fn for_alphabet(from: A) -> Self;

    /// Creates an instance of `Self` for the given [`Alphabet`] and a hint for the size of the transition
    /// system allowing for preallocation of memory. The resulting TS should be empty.
    fn for_alphabet_size_hint(from: A, _size_hint: usize) -> Self {
        Self::for_alphabet(from)
    }
}

/// Marker trait for [`Alphabet`]s that can be indexed, i.e. where we can associate each
/// [`Alphabet::Symbol`] and [`Alphabet::Expression`] with a unique index (a `usize`).
pub trait IndexedAlphabet: Alphabet {
    /// Turns the given symbol into an index.
    fn symbol_to_index(&self, sym: Self::Symbol) -> usize;
    /// Turns the given expression into an index.
    fn expression_to_index(&self, sym: &Self::Expression) -> usize;
    /// Returns the symbol that corresponds to the given index.
    fn symbol_from_index(&self, index: usize) -> Self::Symbol;
    /// Returns the expression that corresponds to the given index.
    fn expression_from_index(&self, index: usize) -> Self::Expression;
    /// Turns the given expression into a symbol.
    fn expression_to_symbol(&self, expression: &Self::Expression) -> Self::Symbol {
        self.symbol_from_index(self.expression_to_index(expression))
    }
    /// Turns the given symbol into an expression.
    fn symbol_to_expression(&self, symbol: Self::Symbol) -> Self::Expression {
        self.expression_from_index(self.symbol_to_index(symbol))
    }
}

impl IndexedAlphabet for CharAlphabet {
    fn symbol_to_index(&self, sym: Self::Symbol) -> usize {
        self.expression_to_index(&sym)
    }

    fn expression_to_index(&self, sym: &Self::Expression) -> usize {
        self.universe()
            .position(|x| x == *sym)
            .expect("Must be present in alphabet")
    }

    fn symbol_from_index(&self, index: usize) -> Self::Symbol {
        self.expression_from_index(index)
    }

    fn expression_from_index(&self, index: usize) -> Self::Expression {
        assert!(index < self.size());
        self[index]
    }
}

/// Trait for transition systems that allow insertion of states and transitions.
pub trait Sproutable: TransitionSystem {
    /// Adds a new state with the given color. The method returns the index of the newly created
    /// state.
    ///
    /// # Example
    /// ```
    /// use automata::{
    ///     core::{alphabet::CharAlphabet, Void},
    ///     ts::{Deterministic, ForAlphabet, TSBuilder, Shrinkable, Sproutable, TransitionSystem},
    ///     DTS,
    /// };
    ///     
    /// let mut ts = DTS::<_, _, Void>::for_alphabet(CharAlphabet::of_size(3));
    /// let q0 = ts.add_state(false);
    /// let before = ts.size();
    /// let q1 = ts.add_state(true);
    /// assert_eq!(ts.size(), before + 1);
    /// ```
    /// We need to explicitly give the state color for the created transition system as we do not
    /// add any edges and the type can therefore not be inferred.
    fn add_state(&mut self, color: StateColor<Self>) -> Self::StateIndex;

    /// Adds a new edge to the transition system. Returns an [`EdgeTuple`] consisting of the overwritten
    /// edge if it existed, or `None` otherwise.
    ///
    /// The details of the edge are given as a type that implements [`IntoEdgeTuple`]. This allows
    /// for a more flexible way of adding edges, as the method can be called with a tuple, a struct
    /// or any other type that can be converted into a tuple of the form
    /// `(StateIndex, Expression, EdgeColor, StateIndex)`. Moreover, if `EdgeColor` is `Void`, then
    /// we can simply call the method with a tuple of the form `(StateIndex, Expression, StateIndex)`
    /// and the method will automatically convert the tuple into the correct form.
    ///
    /// # Example
    /// ```
    /// use automata::{
    ///     core::alphabet::CharAlphabet,
    ///     ts::{Deterministic, ForAlphabet, TSBuilder, Shrinkable, Sproutable, TransitionSystem},
    ///     DTS,
    /// };
    ///
    /// let mut ts = DTS::for_alphabet(CharAlphabet::of_size(3));
    /// let q0 = ts.add_state(false);
    /// let q1 = ts.add_state(true);
    /// assert!(ts.edge(q0, 'a').is_none());
    /// assert!(ts.add_edge((q0, 'a', q1)).is_none());
    /// assert!(ts.edge(q0, 'a').is_some());
    ///
    /// assert!(ts.add_edge((q0, 'a', q0)).is_some(), "Cannot add overlapping edges as ts is deterministic");
    /// ```
    fn add_edge<E>(&mut self, t: E) -> Option<EdgeTuple<Self>>
    where
        E: IntoEdgeTuple<Self>;

    /// Builds a new transition system by collecting all states and transitions present in another.
    /// The method returns the new transition system and a [bijective mapping](`Bijection`) between the old and new
    /// state indices.
    ///
    /// Creates a new transition system, by collecting all states and transitions present in `ts`.
    /// This is done by using a naive approach, which simply iterates through all states and adds
    /// them one by one. At the same time, a bijective mapping between old and new state indices is
    /// created. Subsequently, the transitions are inserted one by one. Finally, the newly created
    /// transition system is returned together with the bijective state index mapping.
    ///
    /// # Example
    /// ```
    /// use automata::{
    ///     core::alphabet::CharAlphabet,
    ///     ts::{Deterministic, ForAlphabet, TSBuilder, TransitionSystem, Shrinkable, Sproutable},
    ///     DTS,
    /// };
    ///
    /// let source: DTS<CharAlphabet, usize, usize> = DTS::builder()
    ///     .with_transitions([(0, 'a', 0, 0), (0, 'b', 0, 0)])
    ///     .with_state_colors([0])
    ///     .into_dts();
    ///
    /// let (dts, map) = DTS::sprout_from_ts_with_bijection(&source);
    /// assert_eq!(source.size(), dts.size());
    /// assert_eq!(map.get_by_left(&0), Some(&0));
    /// ```
    fn sprout_from_ts_with_bijection<Ts>(
        ts: Ts,
    ) -> (Self, Bijection<Ts::StateIndex, StateIndex<Self>>)
    where
        Self: ForAlphabet<Ts::Alphabet>,
        Ts: TransitionSystem<
                Alphabet = Self::Alphabet,
                StateColor = StateColor<Self>,
                EdgeColor = EdgeColor<Self>,
            >,
    {
        let mut out: Self = Self::for_alphabet(ts.alphabet().clone());
        let mut map = Bijection::new();
        for index in ts.state_indices() {
            map.insert(
                index,
                out.add_state(
                    ts.state_color(index)
                        .expect("We assume each state to be colored!"),
                ),
            );
        }
        for index in ts.state_indices() {
            let source = *map.get_by_left(&index).unwrap();
            for edge in ts.edges_from(index).expect("State exists") {
                out.add_edge((
                    source,
                    edge.expression().clone(),
                    edge.color(),
                    *map.get_by_left(&edge.target()).unwrap(),
                ));
            }
        }
        (out, map)
    }

    /// Builds a new transition system by collecting all states and transitions present in another.
    ///
    /// Creates a new transition system, by collecting all states and transitions present in `ts`.
    /// This is done by using a naive approach, which simply iterates through all states and adds
    /// them one by one. At the same time, a bijective mapping between old and new state indices is
    /// created. Subsequently, the transitions are inserted one by one. Finally, the newly created
    /// transition system is returned.
    ///
    /// # Example
    /// ```
    /// use automata::{
    ///     core::alphabet::CharAlphabet,
    ///     ts::{Deterministic, TransitionSystem, ForAlphabet, TSBuilder, Shrinkable, Sproutable},
    ///     DTS,
    /// };
    ///
    /// let source: DTS<CharAlphabet, usize, usize> = DTS::builder()
    ///     .with_transitions([(0, 'a', 0, 0), (0, 'b', 0, 0)])
    ///     .with_state_colors([0])
    ///     .into_dts();
    ///
    /// let dts = DTS::sprout_from_ts(&source);
    /// assert_eq!(source.size(), dts.size());
    /// ```
    fn sprout_from_ts<Ts>(ts: Ts) -> Self
    where
        Self: ForAlphabet<Ts::Alphabet>,
        Ts: TransitionSystem<
                Alphabet = Self::Alphabet,
                StateColor = StateColor<Self>,
                EdgeColor = EdgeColor<Self>,
            >,
    {
        Self::sprout_from_ts_with_bijection(ts).0
    }

    /// Turns the automaton into a complete one, by adding a sink state and adding transitions
    /// to it from all states that do not have a transition for a given symbol.
    ///
    /// The sink state will be colored with `sink_color` and each newly introduced edge will
    /// be colored with `edge_color`.
    ///
    /// # Example
    /// We should be able to take a TS that is not yet complete and use this method to build
    /// one which is complete. For an incomplete transition system, this should result in
    /// a new state being created.
    /// ```
    /// use automata::{
    ///     core::{alphabet::CharAlphabet, Void},
    ///     ts::{Deterministic, ForAlphabet, TSBuilder, Shrinkable, TransitionSystem, Sproutable},
    ///     DTS,
    /// };
    ///
    /// let mut ts = TSBuilder::without_colors()
    ///     .with_edges([(0, 'a', 0)])
    ///     .with_alphabet_symbols(['a', 'b'])
    ///     .into_dts();
    /// assert!(!ts.is_complete());
    /// ts.complete_with_colors(Void, Void);
    /// assert_eq!(ts.size(), 2);
    /// assert!(ts.is_complete());
    /// ```
    ///
    /// However if we call the method on a TS that is already complete, i.e. it has a transition
    /// for every alphabet symbol going out from every state, then the method should return the
    /// original TS unchanged.
    /// ```
    /// use automata::{
    ///     core::{alphabet::CharAlphabet, Void},
    ///     ts::{Deterministic, ForAlphabet, TSBuilder, TransitionSystem, Shrinkable, Sproutable},
    ///     DTS,
    /// };
    /// let mut ts = TSBuilder::without_colors()
    ///     .with_edges([(0, 'a', 0), (0, 'b', 0)])
    ///     .with_alphabet_symbols(['a', 'b'])
    ///     .into_dts();
    /// assert!(ts.is_complete());
    /// ts.complete_with_colors(Void, Void);
    /// assert_eq!(ts.size(), 1);
    /// assert!(ts.is_complete());
    /// ```
    fn complete_with_colors(&mut self, sink_color: Self::StateColor, edge_color: Self::EdgeColor)
    where
        Self::Alphabet: IndexedAlphabet,
    {
        // in case we are already a complete TS, we can return without changes
        if self.is_complete() {
            return;
        }

        let sink = self.add_state(sink_color);
        for sym in self.alphabet().universe().collect_vec() {
            self.add_edge((
                sink,
                self.alphabet().symbol_to_expression(sym),
                edge_color.clone(),
                sink,
            ));
        }
        let mut seen = BitSet::with_capacity(self.alphabet().size());
        for state in self.state_indices().collect_vec() {
            seen.make_empty();
            for edge in self.edges_from(state).unwrap() {
                seen.insert(self.alphabet().expression_to_index(edge.expression()));
            }
            for missing in (0..self.alphabet().size()).filter(|i| !seen.contains(*i)) {
                self.add_edge((
                    state,
                    self.alphabet().expression_from_index(missing),
                    edge_color.clone(),
                    sink,
                ));
            }
        }
    }

    /// Add a new states with colors given by an iterator. For each provided color, a new state is
    /// created. Returns an iterator over the indices of the newly created states.
    ///
    /// # Example
    /// ```
    /// use automata::{
    ///     core::{alphabet::CharAlphabet, Void},
    ///     ts::{Deterministic, TransitionSystem, ForAlphabet, TSBuilder, Shrinkable, Sproutable},
    ///     DTS,
    /// };
    ///
    /// let mut ts = DTS::<_, _, Void>::for_alphabet(CharAlphabet::of_size(3));
    /// let states = ts.extend_states([true, false, true]);
    /// assert_eq!(states.collect::<Vec<_>>(), vec![0, 1, 2]);
    /// ```
    /// We need to explicitly give the edge color for the created transition system as we do not
    /// add any edges and the type can therefore not be inferred.
    fn extend_states<I: IntoIterator<Item = StateColor<Self>>>(
        &mut self,
        iter: I,
    ) -> impl Iterator<Item = Self::StateIndex> {
        iter.into_iter().map(move |color| self.add_state(color))
    }

    /// Sets the color of the state with the given index. If the state does not exist, the method
    /// should panic. Usually, we would like to avoid recoloring states individually, and instead
    /// use the [`TransitionSystem::map_state_colors`] method.
    fn set_state_color(&mut self, index: StateIndex<Self>, color: StateColor<Self>);

    /// Sets the state color of the initial state. This method is only available for a ts if it
    /// is [`Pointed`] and it simply obtains the initial state and subsequetly [sets its color](`Self::set_state_color`).
    fn set_initial_color(&mut self, color: StateColor<Self>)
    where
        Self: Pointed,
    {
        self.set_state_color(self.initial(), color);
    }
}

#[cfg(test)]
mod tests {
    use crate::ts::{Deterministic, ForAlphabet, Sproutable};
    use crate::{DTS, TransitionSystem};
    use automata_core::alphabet::{Alphabet, CharAlphabet};

    #[test]
    fn for_alphabet_inference() {
        let mut ts = DTS::for_alphabet(CharAlphabet::of_size(3));
        assert_eq!(ts.alphabet().size(), 3);

        let q0 = ts.add_state(false);
        let q1 = ts.add_state(true);
        assert_eq!(ts.edge(q0, 'a'), None);
        ts.add_edge((q0, 'a', q1));
        assert_eq!(ts.reached_state_index_from(q0, "a"), Some(q1));
    }

    #[test]
    fn sprout_after_creating() {
        let mut ts = DTS::for_alphabet(CharAlphabet::of_size(3));
        let q0 = ts.add_state(false);
        let q1 = ts.add_state(true);
        assert_eq!(ts.edge(q0, 'a'), None);
        ts.add_edge((q0, 'a', q1));
        assert_eq!(ts.reached_state_index_from(q0, "a"), Some(q1));
    }

    #[test]
    fn complete_ts() {
        let mut partial = DTS::builder()
            .default_color(())
            .with_transitions([
                (0, 'a', 0, 0),
                (0, 'b', 0, 0),
                (0, 'c', 0, 1),
                (1, 'a', 0, 0),
            ])
            .into_dts();
        assert_eq!(partial.reached_state_index_from(0, "aaacb"), None);
        assert!(!partial.is_complete());

        partial.complete_with_colors((), 2);
        for w in ["abbaccababcab", "bbcca", "cc", "aababbabbabbccbabba"] {
            if partial.reached_state_index_from(0, w).unwrap() < 1 {
                panic!("Word {} misclassified", w);
            }
        }
    }
}
