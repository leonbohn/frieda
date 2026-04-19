use std::{cell::RefCell, fmt::Debug};

use automata::DTS;
use automata::core::alphabet::Alphabet;
use automata::core::word::Concat;
use automata::core::word::FiniteWord;
use automata::core::{Show, math};
use automata::ts::{ForAlphabet, Sproutable, SymbolOf};
use fixedbitset::FixedBitSet;
use itertools::Itertools;
use tracing::{debug, info, trace, warn};

use super::{Experiment, Hypothesis, ObservationTable, oracle::Oracle};

const ITERATION_THRESHOLD: usize = if cfg!(debug_assertions) { 50 } else { 200000 };

type Word<D> = Vec<SymbolOf<D>>;
pub type Experiments<D> = Vec<Experiment<SymbolOf<D>>>;

/// An implementation of the L* algorithm.
pub struct LStar<D: Hypothesis, T: Oracle<Alphabet = D::Alphabet>> {
    // the alphabet of what we are learning
    alphabet: D::Alphabet,
    // a mapping containing all queries that have been posed so far, together with their output
    queries: RefCell<math::OrderedMap<Word<D>, D::Output>>,
    // the minimal access words forming the base states
    base: Vec<Word<D>>,
    // all known experiments
    experiments: Vec<Experiment<SymbolOf<D>>>,
    // mapping from input word to a bitset, where the i-th entry gives the value for
    // the output of concatenating input word and i-th experiment
    table: math::OrderedMap<Word<D>, Vec<D::Output>>,
    // the oracle
    oracle: T,
    observations: ObservationTable<SymbolOf<D>, T::Output>,
}

impl<D: Hypothesis, T: Oracle<Alphabet = D::Alphabet, Output = D::Output>> LStar<D, T> {
    pub fn new(alphabet: D::Alphabet, oracle: T) -> Self {
        Self {
            experiments: D::mandatory_experiments(&alphabet).into_iter().collect(),
            observations: ObservationTable::with_rows_and_experiments(
                [],
                D::mandatory_experiments(&alphabet),
            ),
            alphabet,
            queries: RefCell::new(math::OrderedMap::default()),
            base: vec![vec![]],
            table: math::OrderedMap::default(),
            oracle,
        }
    }

    fn output(&self, w: &Word<D>) -> D::Output {
        if !self.queries.borrow().contains_key(w) {
            let c = self.oracle.output(w);
            assert!(self.queries.borrow_mut().insert(w.to_owned(), c).is_none());
        }
        self.queries.borrow().get(w).unwrap().clone()
    }

    fn update_table(&mut self) {
        let mut updates = vec![];
        let experiment_count = self.experiments.len();

        for (n, mr) in self.one_letter_extensions().enumerate() {
            let stored_experiment_count = self.table.get(&mr).map(|r| r.len()).unwrap_or(0);
            if stored_experiment_count < experiment_count {
                for i in stored_experiment_count..experiment_count {
                    let concat = Concat(&mr, &self.experiments[i]).collect_vec();
                    let output = self.output(&concat).clone();
                    updates.push((mr.clone(), i, output));
                }
            } else {
                assert_eq!(
                    stored_experiment_count,
                    experiment_count,
                    "Too many experiments present for {}",
                    mr.as_string()
                );
            }
        }

        for (mr, i, output) in updates {
            assert!(i < self.experiments.len());
            let mut row = self.table.entry(mr).or_default();
            row.push(output);
        }

        if cfg!(debug_assertions) {
            for mr in self.one_letter_extensions() {
                let Some(val) = self.table.get(&mr) else {
                    panic!("No table entry for {}", mr.as_string());
                };
                if val.len() != experiment_count {
                    panic!(
                        "Row for {} does not have enough entries, has {} of {experiment_count}",
                        mr.show(),
                        val.len()
                    )
                }
            }
        }

        trace!("After update the table is\n{:?}", self);
    }

    pub fn infer(&mut self) -> D {
        let start = std::time::Instant::now();
        let mut iteration = 0;
        let threshold = std::env::var("MAX_ITERATIONS")
            .unwrap_or(format!("{ITERATION_THRESHOLD}"))
            .parse()
            .unwrap();

        'outer: while iteration < threshold {
            self.update_table();
            iteration += 1;
            trace!("LStar iteration {iteration} with table\n{:?}", self);
            let todo = self.rows_to_promote();
            trace!(
                "Have to promote rows: {}",
                todo.iter().map(FiniteWord::as_string).join(", ")
            );

            if !todo.is_empty() {
                for r in todo {
                    self.base.push(r);
                }
                // TODO optimize this call!
                self.update_table();
                continue 'outer;
            }

            let hypothesis = self.hypothesis();

            let Err((witness, expected_color)) = self.oracle.equivalence(&hypothesis) else {
                let duration = start.elapsed().as_millis();
                info!("Execution of LStar took {duration}ms");
                return hypothesis;
            };
            assert!(hypothesis.output(&witness) != expected_color);
            self.process_counterexample(witness, expected_color);
        }

        panic!("Iteration threshold exceeded!")
    }

    fn process_counterexample(&mut self, word: Word<D>, color: D::Output) {
        trace!("Processing counterexample {}", word.as_string());
        for i in 0..(word.len()) {
            let suffix = Experiment(word[i..].to_vec());
            assert!(!suffix.is_empty());
            if !self.experiments.contains(&suffix) {
                trace!("Adding counterexample {}", suffix.as_string());
                self.experiments.push(suffix)
            }
        }
    }

    fn state_color(&self, mr: &Word<D>) -> D::StateColor {
        D::give_state_color(
            mr,
            &self.experiments,
            self.table
                .get(mr)
                .expect("Cannot give color for non-existent word"),
        )
    }

    fn hypothesis(&self) -> D {
        let start = std::time::Instant::now();

        let mut ts: DTS<_, _, _> = DTS::for_alphabet_size_hint(self.alphabet.clone(), 1);
        let mut state_map = math::Map::default();
        let mut observations = math::Map::default();

        for mr in &self.base {
            let color = self.state_color(mr);
            let id = ts.add_state(color);
            state_map.insert(mr, id);

            let observed = self.table.get(mr).unwrap();
            observations.insert(observed, mr);
        }

        if cfg!(debug_assertions) {
            for (mr, obs) in &self.table {
                if !observations.contains_key(&obs) {
                    let to_promote = self.rows_to_promote();
                    panic!(
                        "rows {to_promote:?} are not closed even though they should be!\n{self:?}"
                    );
                }
            }
        }

        for (mr, i) in &state_map {
            for a in self.alphabet.universe() {
                let obs = self
                    .table
                    .get(&Concat(mr, [a]).into_vec())
                    .expect("Can only work if table is closed!");

                let Some(target) = observations.get(obs) else {
                    panic!(
                        "could not find target for {:?} ({mr:?} on symbol {a:?}) in table\n{:?}\n{:?}",
                        Concat(mr, [a]).into_vec(),
                        self,
                        self.table
                    );
                };

                let color = D::give_transition_color(
                    mr,
                    a,
                    target,
                    &self.experiments,
                    self.table.get(*mr).unwrap(),
                    self.table.get(*target).unwrap(),
                );

                let added = ts.add_edge((
                    *i,
                    self.alphabet.make_expression(a),
                    color,
                    *state_map.get(target).unwrap(),
                ));
                assert!(added.is_none());
            }
        }

        let duration = start.elapsed().as_micros();
        debug!("Building hypothesis took {duration} microseconds");

        D::from_transition_system(ts, *state_map.get(&vec![]).unwrap())
    }

    fn inconsistent_words(&self) -> Option<Word<D>> {
        for (i, left) in self.base.iter().enumerate() {
            'middle: for right in &self.base[i..] {
                if self.table.get(left) != self.table.get(right) {
                    continue 'middle;
                }

                for sym in self.alphabet.universe() {
                    let left_ext = left
                        .iter()
                        .chain(std::iter::once(&sym))
                        .cloned()
                        .collect_vec();
                    let right_ext = right
                        .iter()
                        .chain(std::iter::once(&sym))
                        .cloned()
                        .collect_vec();

                    let l = self.table.get(&left_ext).expect("Should be present!");
                    let r = self.table.get(&right_ext).expect("Should be present!");

                    if l == r {
                        continue 'middle;
                    }
                    'inner: for (j, e) in self.experiments.iter().enumerate() {
                        assert!(j < std::cmp::min(l.len(), r.len()));
                        if l[j] == r[j] {
                            continue 'inner;
                        }
                        return Some(Concat(vec![sym], e).into_vec());
                    }
                }
            }
        }
        None
    }

    fn one_letter_extensions(&self) -> impl Iterator<Item = Word<D>> + '_ {
        self.base
            .iter()
            .flat_map(|w| {
                std::iter::once(w.clone()).chain(self.alphabet.universe().filter_map(|a| {
                    let mut x = w.clone();
                    x.push(a);
                    if !self.base.contains(&x) {
                        Some(x)
                    } else {
                        None
                    }
                }))
            })
            .unique()
    }

    fn rows_to_promote(&self) -> math::Set<Word<D>> {
        trace!("deciding which rows to promote: {:?}", self);
        let start = std::time::Instant::now();

        let mut known = math::Set::from_iter(self.base.iter().map(|b| {
            self.table.get(b).unwrap_or_else(|| {
                panic!(
                    "Experiment {} must be present",
                    owo_colors::OwoColorize::blue(&b.as_string())
                )
            })
        }));
        trace!("start promotion search with known rows: {known:?}");
        let mut out = math::Set::default();

        for (word, seq) in self.table.iter() {
            if known.insert(seq) {
                out.insert(word.clone());
            }
        }

        debug!(
            "Finding rows to promote took {} microseconds",
            start.elapsed().as_micros()
        );
        out
    }
}

impl<D: Hypothesis, T: Oracle<Alphabet = D::Alphabet>> std::fmt::Debug for LStar<D, T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut builder = tabled::builder::Builder::default();
        let mut header = vec!["MR".to_string()];

        for e in &self.experiments {
            header.push(e.as_string());
        }
        builder.push_record(header);
        for (mr, out) in &self.table {
            let mut row = vec![if self.base.contains(mr) {
                owo_colors::OwoColorize::blue(&mr.as_string()).to_string()
            } else {
                mr.as_string()
            }];
            for e in out {
                row.push(format!("{:?}", e));
            }
            builder.push_record(row);
        }

        write!(f, "{}", builder.build())
    }
}

#[cfg(test)]
mod tests {
    use crate::active::{MealyOracle, MooreOracle, Oracle};
    use automata::TransitionSystem;
    use automata::automaton::{MealyMachine, MooreMachine};
    use automata::core::alphabet::CharAlphabet;
    use rand::{Rng, RngExt};
    use tracing::trace;

    #[test]
    fn lstar_random_mealy() {
        let mut rng = rand::rng();
        for i in 0..200 {
            let symbols = rng.random_range(1..5);
            let max_color = rng.random_range(1..10);
            let size = rng.random_range(1..25);

            let pre_gen = std::time::Instant::now();
            let ts = automata::random::generate_random_mealy(symbols, max_color, size);
            trace!(
                "generated mealy for size {size}, symbols {symbols} and max_color {max_color} in {} microseconds",
                pre_gen.elapsed().as_micros()
            );

            let oracle = MealyOracle::new(&ts);

            let start = std::time::Instant::now();
            let mm: MealyMachine = super::LStar::new(oracle.alphabet().clone(), oracle).infer();
            let duration = start.elapsed().as_millis();
            trace!("learning took {} ms", duration);

            if mm.size() != ts.size() {
                panic!(
                    "Sizes don't match, expected {} got {}",
                    ts.size(),
                    mm.size()
                );
            }
        }
    }
    #[test]
    fn lstar_random_moore() {
        let mut rng = rand::rng();
        for i in 0..200 {
            let symbols = rng.random_range(1..5);
            let max_color = rng.random_range(1..10);
            let size = rng.random_range(1..25);

            let pre_gen = std::time::Instant::now();
            let ts = automata::random::generate_random_moore(symbols, max_color, size);
            trace!(
                "generated Moore for size {size}, symbols {symbols} and max_color {max_color} in {} microseconds",
                pre_gen.elapsed().as_micros()
            );

            let oracle = MooreOracle::new(ts.clone());

            let start = std::time::Instant::now();
            let mm: MooreMachine =
                super::LStar::new(CharAlphabet::of_size(symbols), oracle).infer();
            let duration = start.elapsed().as_millis();
            trace!("learning took {} ms", duration);

            if mm.size() != ts.size() {
                panic!(
                    "Sizes don't match, expected {} got {}",
                    ts.size(),
                    mm.size()
                );
            }
        }
    }
}
