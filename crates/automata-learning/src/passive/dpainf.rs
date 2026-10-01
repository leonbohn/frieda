use std::{
    collections::{BTreeSet, VecDeque},
    fmt::Display,
};

use automata::{Class, Pointed, RightCongruence, TransitionSystem, ts::operations::ProductIndex};
use itertools::Itertools;
use tracing::{error, trace, warn};

use crate::{
    passive::{ClassOmegaSample, FiniteSample, OmegaSample, SetSample, SplitOmegaSample},
    prefixtree::prefix_tree,
};

use automata::core::alphabet::Alphabet;
use automata::core::word::FiniteWord;
use automata::core::{Show, Void, math};
use automata::representation::{CollectTs, IntoTs};
use automata::ts::operations::Product;
use automata::ts::predecessors::PredecessorIterable;
use automata::ts::{Deterministic, IsEdge, ScalarIndexType, Shrinkable, Sproutable, StateIndex};
use owo_colors::OwoColorize;

/// Represents a consistency check that can be performed on a congruence. This is used in the
/// omega-sprout algorithm to ensure in each iteration, that the produced congruence relation
/// is consistent with the given constraints. The constraints can either be given by a conflict
/// relation, by a list or they could be verified against a sample (or other form of data).
pub trait ConsistencyCheck<A: Alphabet> {
    /// Verifies that `cong` is consistent with the constraint.
    fn consistent(&self, cong: &RightCongruence<A>) -> bool;
    /// Returns an approximate threshold for the number of classes in the congruence. This is
    /// useful for algorithms to detect infinite loops.
    fn threshold(&self) -> usize;
    /// Returns a reference to the alphabet used by the constraint.
    fn alphabet(&self) -> &A;
}

impl<A: Alphabet, CC: ConsistencyCheck<A>> ConsistencyCheck<A> for &CC {
    fn alphabet(&self) -> &A {
        CC::alphabet(self)
    }
    fn consistent(&self, cong: &RightCongruence<A>) -> bool {
        CC::consistent(self, cong)
    }
    fn threshold(&self) -> usize {
        CC::threshold(self)
    }
}

impl<A: Alphabet> ConsistencyCheck<A> for FiniteSample<A> {
    fn consistent(&self, cong: &RightCongruence<A>) -> bool {
        let positive_indices: math::OrderedSet<_> = self
            .positive_words()
            .filter_map(|w| cong.reached_state_index(w))
            .collect();
        let negative_indices: math::OrderedSet<_> = self
            .negative_words()
            .filter_map(|w| cong.reached_state_index(w))
            .collect();
        positive_indices.is_disjoint(&negative_indices)
    }

    fn threshold(&self) -> usize {
        self.max_word_len() * 2
    }

    fn alphabet(&self) -> &A {
        &self.alphabet
    }
}

/// Stores two DFAs and a math::Set of conflicts between them.
#[derive(Clone)]
pub struct ConflictRelation<A: Alphabet> {
    dfas: [RightCongruence<A>; 2],
    conflicts: math::OrderedSet<(StateIndex, StateIndex)>,
}

impl<A: Alphabet> ConsistencyCheck<A> for ConflictRelation<A> {
    fn alphabet(&self) -> &A {
        self.dfas[0].alphabet()
    }
    /// Verifies that a given congruence is consistent with the conflicts.
    fn consistent(&self, cong: &RightCongruence<A>) -> bool {
        let left = cong.ts_product(&self.dfas[0]);
        let right = cong.ts_product(&self.dfas[1]);
        let right_reachable = right.reachable_state_indices().collect_vec();

        for ProductIndex(lcong, ldfa) in left.reachable_state_indices() {
            for ProductIndex(rcong, rdfa) in right_reachable
                .iter()
                .filter(|ProductIndex(rcong, _)| rcong == &lcong)
            {
                if lcong == *rcong && self.conflicts.contains(&(ldfa, *rdfa)) {
                    let lname = ldfa.show();
                    let rname = rdfa.show();
                    let congname = lcong.show();
                    trace!(
                        "\t\tConflict found, ({congname}, {lname}) and ({congname}, {rname}) reachable with ({lname}, {rname}) in conflicts"
                    );
                    return false;
                }
            }
        }
        true
    }

    /// Returns a preliminary threshold for the number of states in the product of the two DFAs.
    fn threshold(&self) -> usize {
        2 * self.dfas[0].size() * self.dfas[1].size()
    }
}

impl<A: Alphabet> std::fmt::Debug for ConflictRelation<A> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        todo!()
    }
}

impl<A: Alphabet> ConflictRelation<A> {
    /// Returns a reference to the underlying alphabet of one of the DFAs. We assure that both DFAs use the same
    /// alphabet.
    pub fn alphabet(&self) -> &A {
        self.dfas[0].alphabet()
    }
}

/// Computes a conflict relation encoding iteration consistency. For more details on the construction,
/// see Lemma 29 in [this paper](https://arxiv.org/pdf/2302.11043.pdf).
pub fn iteration_consistency_conflicts<A: Alphabet>(
    samples: &SplitOmegaSample<'_, A>,
    class: Class<A::Symbol>,
) -> ConflictRelation<A> {
    let idx = samples.cong().reached_state_index(&class).unwrap();
    let Some(sample) = samples.get(idx) else {
        panic!("Sample for class {:?} does not exist!", class)
    };
    let periodic_sample = sample.to_periodic_sample();

    trace!(
        "Positive periodic words: {}\nNegative periodic words: {}",
        periodic_sample
            .positive()
            .map(|w| format!("{:?}", w))
            .join(","),
        periodic_sample
            .negative()
            .map(|w| format!("{:?}", w))
            .join(",")
    );

    let looping_words = samples.cong().looping_words(idx);

    let left_pta = prefix_tree(sample.alphabet.clone(), periodic_sample.positive())
        .map_state_colors(|mr| {
            !mr.is_empty() && periodic_sample.classify(mr.omega_power()) == Some(true)
        })
        .into_dfa()
        .intersection(&looping_words)
        .collect_dfa();

    let right_pta = prefix_tree(sample.alphabet.clone(), periodic_sample.negative())
        .map_state_colors(|mr| {
            !mr.is_empty() && periodic_sample.classify(mr.omega_power()) == Some(false)
        })
        .into_dfa()
        .intersection(&looping_words)
        .collect_dfa();

    let mut conflicts = math::OrderedSet::default();
    let mut queue = VecDeque::from_iter(
        left_pta
            .accepting_states()
            .cartesian_product(right_pta.accepting_states()),
    );

    let mut left_cache: std::collections::HashMap<_, _> = std::collections::HashMap::default();
    let mut right_cache: std::collections::HashMap<_, _> = std::collections::HashMap::default();

    while let Some((left, right)) = queue.pop_front() {
        if !conflicts.insert((left, right)) {
            continue;
        }
        let left_pred = left_cache
            .entry(left)
            .or_insert_with(|| left_pta.predecessors(left).unwrap().collect_vec());
        let right_pred = right_cache
            .entry(right)
            .or_insert_with(|| right_pta.predecessors(right).unwrap().collect_vec());

        for l in left_pred {
            for r in right_pred
                .iter()
                .filter(|o| o.expression() == l.expression())
            {
                queue.push_back((l.source(), r.source()));
            }
        }
    }

    let (left, left_initial) = left_pta.into_dts_preserving_and_initial();
    let (right, right_initial) = right_pta.into_dts_preserving_and_initial();

    let conflicts: BTreeSet<_> = conflicts
        .into_iter()
        .map(|(l, r)| (left.old_to_new(l).unwrap(), right.old_to_new(r).unwrap()))
        .collect();

    ConflictRelation {
        dfas: [
            left.erase_colors()
                .with_initial(left_initial)
                .into_right_congruence(),
            right
                .erase_colors()
                .with_initial(right_initial)
                .into_right_congruence(),
        ],
        conflicts,
    }
}

/// Computes a conflict relation encoding prefix consistency. For more details on how this works, see
/// Lemma 28 in [this paper](https://arxiv.org/pdf/2302.11043.pdf).
pub fn prefix_consistency_conflicts<A: Alphabet>(sample: &OmegaSample<A>) -> ConflictRelation<A> {
    let left_pta = prefix_tree(sample.alphabet.clone(), sample.positive_words());
    let right_pta = prefix_tree(sample.alphabet.clone(), sample.negative_words());

    let dfa = (&left_pta).ts_product(&right_pta);

    trace!("built prefix tree product");
    let sccs = dfa.sccs();
    trace!("computed scc decomposition");

    let states_with_infinite_run: Vec<_> = sccs
        .iter()
        .filter_map(|(_, scc)| {
            if !scc.is_transient() {
                Some(scc.clone().into_iter().map(Into::into))
            } else {
                None
            }
        })
        .flatten()
        .collect();

    let mut conflicts = math::OrderedSet::default();
    for ProductIndex(l, r) in dfa.state_indices() {
        let reachable = dfa
            .reachable_state_indices_from(ProductIndex(l, r))
            .collect_vec();
        if reachable
            .iter()
            .any(|ProductIndex(p, q)| states_with_infinite_run.contains(&(*p, *q)))
        {
            conflicts.insert((l, r));
        }
    }

    ConflictRelation {
        dfas: [
            left_pta.erase_state_colors().into_right_congruence(),
            right_pta.erase_state_colors().into_right_congruence(),
        ],
        conflicts,
    }
}

impl<A: Alphabet> ConsistencyCheck<A> for () {
    fn consistent(&self, cong: &RightCongruence<A>) -> bool {
        true
    }

    fn threshold(&self) -> usize {
        0
    }

    fn alphabet(&self) -> &A {
        unimplemented!("This does not make sense, you should not call this function directly")
    }
}

/// This constraint ensures that the learned automaton separates idempotents.
#[derive(Clone)]
pub struct SeparatesIdempotents<'a, A: Alphabet> {
    sample: &'a ClassOmegaSample<'a, A>,
}

impl<'a, A: Alphabet> SeparatesIdempotents<'a, A> {
    /// Creates a new instance of the constraint.
    pub fn new(sample: &'a ClassOmegaSample<'a, A>) -> Self {
        Self { sample }
    }
}

impl<A: Alphabet> ConsistencyCheck<A> for SeparatesIdempotents<'_, A> {
    fn consistent(&self, cong: &RightCongruence<A>) -> bool {
        true
    }

    fn threshold(&self) -> usize {
        todo!()
    }

    fn alphabet(&self) -> &A {
        todo!()
    }
}

#[derive(Debug)]
pub enum DpaInfError<A: Alphabet> {
    /// The threshold has been exceeded, returns constructed right congruence and threshold value
    Threshold(RightCongruence<A>, usize),
    /// The given timeout has been exceeded, returns right congruence that has been constructed thus far
    Timeout(RightCongruence<A>),
}

/// Runs the omega-sprout algorithm on a given conflict relation.
#[allow(clippy::result_large_err)]
pub fn dpainf<A, C>(
    conflicts: C,
    additional_constraints: Vec<Box<dyn ConsistencyCheck<A>>>,
    allow_transitions_into_epsilon: bool,
    timeout_seconds: Option<u64>,
) -> Result<RightCongruence<A>, DpaInfError<A>>
where
    A: Alphabet,
    C: ConsistencyCheck<A>,
{
    let mut cong = RightCongruence::new_with_initial_color(conflicts.alphabet().clone(), Void);
    let initial = cong.initial();
    let threshold = conflicts.threshold();

    // We maintain a math::Set of missing transitions and go through them in order of creation for the states and in order
    // give by alphabet for the symbols for one state (this amouts to BFS).
    let mut queue: VecDeque<_> = conflicts
        .alphabet()
        .universe()
        .map(|sym| (initial, sym))
        .collect();

    let time_start = std::time::Instant::now();
    let timeout_seconds = timeout_seconds.unwrap_or(u64::MAX);
    'outer: while let Some((source, sym)) = queue.pop_front() {
        if time_start.elapsed().as_secs() >= timeout_seconds {
            error!("exceeded timeout, returning right congruence built so far");
            return Err(DpaInfError::Timeout(cong));
        }

        for target in (0..cong.size()) {
            let target = ScalarIndexType::from_usize(target);
            if !allow_transitions_into_epsilon && target == initial {
                continue;
            }
            assert!(
                cong.add_edge((source, cong.make_expression(sym), target))
                    .is_none()
            );

            if conflicts.consistent(&cong)
                && additional_constraints.iter().all(|c| c.consistent(&cong))
            {
                trace!(
                    "\tTransition {source}--{}-->{target} is consistent",
                    sym.show(),
                );
                continue 'outer;
            } else {
                trace!(
                    "\tTransition {source}--{}-->{target} is not consistent",
                    sym.show(),
                );
                cong.remove_edges_between_matching(source, target, sym);
            }
        }

        if cong.size() > threshold {
            error!("exceeded threshold on number of states {threshold}");
            return Err(DpaInfError::Threshold(cong, threshold));
        }

        trace!(
            "No consistent transition found, adding new state {}",
            cong.size()
        );

        let new_state = cong.add_state(Void);
        cong.add_edge((source, cong.make_expression(sym), new_state));
        queue.extend(std::iter::repeat(new_state).zip(conflicts.alphabet().universe()))
    }

    Ok(cong)
}

#[cfg(test)]
pub(crate) mod tests {
    use crate::passive::{SetSample, dpainf::ConflictRelation, sample::OmegaSample};
    use automata::core::alphabet::CharAlphabet;
    use automata::core::upw;
    use automata::{Class, TransitionSystem};
    use itertools::Itertools;

    pub fn inf_aba_sample() -> (CharAlphabet, OmegaSample<CharAlphabet>) {
        let Ok(sample) = OmegaSample::try_from_str(
            r#"omega
            alphabet: a,b
            positive:
            abab
            aba
            ababb
            abaa
            abaab
            negative:
            bab
            abba
            bbab
            aaabb
            a
            abb
            aabb
            babb
            b
            abbba
            abbb"#,
        ) else {
            panic!("Cannot parse sample")
        };
        (sample.alphabet.clone(), sample)
    }

    pub fn testing_larger_forc_sample() -> (CharAlphabet, OmegaSample<CharAlphabet>) {
        let Ok(sample) = OmegaSample::try_from_str(
            r#"omega
        alphabet: a,b
        positive:
        bbabab
        ab
        baa
        abbab
        babab
        babba
        bbaba
        babab
        babba
        aba
        aab
        abaabb
        ababb
        a
        abab
        baba
        ba
        bbaba
        abbab
        babbba
        abbab
        abbaab
        babbbba
        negative:
        bba
        abba
        baab
        bbba
        abb
        abbba
        bab
        bba
        babb
        bbab
        b
        bb
        abba
        bbaab
        abbb
        bbaa
        abbaa
        babbab
        bbabba
        babbb
        bbabb
        "#,
        ) else {
            panic!("Cannot parse sample");
        };
        (sample.alphabet.clone(), sample)
    }

    fn testing_smaller_forc_smaple() -> (CharAlphabet, OmegaSample<CharAlphabet>) {
        let alphabet = CharAlphabet::of_size(3);
        (
            alphabet.clone(),
            SetSample::new_omega_from_pos_neg(
                alphabet,
                [
                    upw!("a"),
                    upw!("baa"),
                    upw!("aca"),
                    upw!("caab"),
                    upw!("abca"),
                ],
                [upw!("b"), upw!("c"), upw!("ab"), upw!("ac"), upw!("abc")],
            ),
        )
    }

    #[test]
    fn learn_small_forc() {
        let (alphabet, sample) = testing_smaller_forc_smaple();
        let cong = sample.infer_prefix_congruence().unwrap();
        assert_eq!(cong.size(), 1);

        let split_sample = sample.split(&cong);
        let eps = Class::epsilon();
        let eps_sample = split_sample.get(0).unwrap();

        let conflicts: ConflictRelation<CharAlphabet> =
            super::iteration_consistency_conflicts(&split_sample, eps);

        let prc_eps = super::dpainf(conflicts, vec![], false, None).unwrap();
        assert_eq!(prc_eps.size(), 6);
    }

    #[test_log::test]
    fn learn_larger_forc() {
        let (alphabet, sample) = testing_larger_forc_sample();
        let cong = sample.infer_prefix_congruence().unwrap();
        tracing::debug!("Got prefix congruence");
        let split = sample.split(&cong);
        let forc = split.infer_forc();
        let prc_eps = forc[0].clone();
        assert_eq!(prc_eps.size(), 13);
    }
}
