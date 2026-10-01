use std::{collections::VecDeque, io::BufRead};

use automata::core::alphabet::{Alphabet, CharAlphabet};
use automata::core::math;
use automata::core::word::{
    OmegaWord, PeriodicOmegaWord, ReducedOmegaWord, ReducedParseError, Word,
};
use automata::representation::CollectTs;
use automata::ts::Deterministic;
use automata::{Class, Pointed, RightCongruence, TransitionSystem};
use either::Either;
use itertools::Itertools;
use thiserror::Error;
use tracing::{debug, trace};

use crate::{
    passive::{
        ClassOmegaSample, SetSample,
        dpainf::{
            DpaInfError, SeparatesIdempotents, dpainf, iteration_consistency_conflicts,
            prefix_consistency_conflicts,
        },
    },
    prefixtree::prefix_tree,
};

use super::{OmegaSample, SplitOmegaSample};

/// Abstracts the types of errors that can occur when parsing an `OmegaSample` from a string.
#[derive(Debug, Clone, Eq, PartialEq, Error)]
#[allow(missing_docs)]
pub enum OmegaSampleParseError {
    #[error("missing sample header")]
    MissingHeader,
    #[error("missing sample alphabet")]
    MissingAlphabet,
    #[error("alphabet directive not followed by `:`")]
    MissingDelimiter,
    #[error("encountered malformed alphabet symbol `{0}`")]
    MalformedAlphabetSymbol(String),
    #[error("sample is inconsistent: `{0}` is classified as positive and negative")]
    Inconsistent(String),
    #[error("alphabet is malformed, missing `positive:` or `negative:` block")]
    MalformedSample,
    #[error("could not parse given omega word, reason: {0}")]
    OmegaWordParseError(ReducedParseError),
}

impl<A: Alphabet> OmegaSample<A> {
    pub fn prefix_tree(&self) -> RightCongruence<A> {
        prefix_tree(self.alphabet().clone(), self.words())
            .erase_state_colors()
            .collect_right_congruence()
    }
}
impl OmegaSample<CharAlphabet> {
    pub fn try_from_lines<I: Iterator<Item = String>>(
        mut lines: I,
    ) -> Result<Self, OmegaSampleParseError> {
        if lines.next().unwrap_or_default().trim() != "omega" {
            return Err(OmegaSampleParseError::MissingHeader);
        }

        let alphabet = lines
            .next()
            .and_then(|line| {
                line.split_once(':')
                    .map(|(header, symbols)| (header.to_string(), symbols.to_string()))
            })
            .map(|(header, symbols)| {
                if header.trim() == "alphabet" {
                    symbols
                        .split(',')
                        .try_fold(Vec::new(), |mut acc, x| {
                            let sym = x.trim();
                            if sym.len() != 1 {
                                Err(OmegaSampleParseError::MalformedAlphabetSymbol(
                                    sym.to_string(),
                                ))
                            } else {
                                acc.push(sym.chars().next().unwrap());
                                Ok(acc)
                            }
                        })
                        .map(CharAlphabet::new)
                } else {
                    Err(OmegaSampleParseError::MissingAlphabet)
                }
            })
            .ok_or(OmegaSampleParseError::MissingDelimiter)??;

        if lines.next().unwrap_or_default().trim() != "positive:" {
            return Err(OmegaSampleParseError::MalformedSample);
        }

        let mut words = math::Map::default();
        'positive: loop {
            match lines.next() {
                Some(word) => {
                    trace!("Parsing positive word \"{word}\"");
                    let word = word.trim();
                    if word.is_empty() || word.starts_with('#') || word == "negative:" {
                        break 'positive;
                    }
                    let parsed = ReducedOmegaWord::try_from_str(word)
                        .map_err(OmegaSampleParseError::OmegaWordParseError)?;
                    if let Some(old_classification) = words.insert(parsed, true) {
                        debug!("Duplicate positive word found");
                    }
                }
                None => return Err(OmegaSampleParseError::MalformedSample),
            }
        }
        for word in lines {
            let word = word.trim();
            if word.starts_with('#') || word.is_empty() {
                continue;
            }
            trace!("Parsing negative word \"{word}\"");
            let parsed = ReducedOmegaWord::try_from_str(word)
                .map_err(OmegaSampleParseError::OmegaWordParseError)?;
            if let Some(old_classification) = words.insert(parsed, false) {
                if old_classification {
                    return Err(OmegaSampleParseError::Inconsistent(word.to_string()));
                }
                debug!("Duplicate negative word found");
            };
        }

        Ok(SetSample::new_omega(alphabet, words))
    }

    pub fn try_from_str(input: &str) -> Result<Self, OmegaSampleParseError> {
        Self::try_from_lines(input.lines().map(|l| l.to_string()))
    }

    pub fn try_from_read<R: BufRead>(mut read: R) -> Result<Self, OmegaSampleParseError> {
        let mut lines = read.lines().filter_map(|line| {
            let line = line.expect("unable to parse line!").trim().to_string();
            if !line.is_empty() && !line.starts_with('#') {
                None
            } else {
                Some(line.to_string())
            }
        });
        Self::try_from_lines(lines)
    }
}

impl<A: Alphabet> OmegaSample<A> {
    /// Creates a new `OmegaSample` from an alphabet as well as two iterators, one
    /// over positive words and one over negative words.
    pub fn new_omega_from_pos_neg<
        W: Into<ReducedOmegaWord<A::Symbol>>,
        I: IntoIterator<Item = W>,
        J: IntoIterator<Item = W>,
    >(
        alphabet: A,
        positive: I,
        negative: J,
    ) -> Self {
        Self {
            alphabet,
            positive: positive.into_iter().map(|w| w.into()).collect(),
            negative: negative.into_iter().map(|w| w.into()).collect(),
        }
    }

    /// Returns an iterator over the positive periodic words in the sample.
    pub fn positive_periodic(&self) -> impl Iterator<Item = PeriodicOmegaWord<A::Symbol>> + '_ {
        self.positive_words()
            .filter_map(|w| PeriodicOmegaWord::try_from(w.clone()).ok())
    }

    /// Returns an iterator over the negative periodic words in the sample.
    pub fn negative_periodic(&self) -> impl Iterator<Item = PeriodicOmegaWord<A::Symbol>> + '_ {
        self.negative_words()
            .filter_map(|w| PeriodicOmegaWord::try_from(w.clone()).ok())
    }

    /// Computes a `PeriodicOmegaSample` containing only the periodic words in the sample.
    pub fn to_periodic_sample(&self) -> PeriodicOmegaSample<A> {
        PeriodicOmegaSample {
            alphabet: self.alphabet.clone(),
            positive: self.positive_periodic().collect(),
            negative: self.negative_periodic().collect(),
        }
    }

    /// Computes the [`RightCongruence`] underlying the sample.
    #[allow(clippy::result_large_err)]
    pub fn infer_prefix_congruence(&self) -> Result<RightCongruence<A>, DpaInfError<A>> {
        dpainf(prefix_consistency_conflicts(self), vec![], true, None)
    }
}

/// A [`PeriodicOmegaSample`] is an omega sample containing only periodic words.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct PeriodicOmegaSample<A: Alphabet> {
    alphabet: A,
    positive: math::Set<PeriodicOmegaWord<A::Symbol>>,
    negative: math::Set<PeriodicOmegaWord<A::Symbol>>,
}

impl<A: Alphabet> PeriodicOmegaSample<A> {
    /// Gives an iterator over all positive periodic words in the sample.
    pub fn positive(&self) -> impl Iterator<Item = &PeriodicOmegaWord<A::Symbol>> + '_ {
        self.positive.iter()
    }

    /// Gives an iterator over all negative periodic words in the sample.
    pub fn negative(&self) -> impl Iterator<Item = &PeriodicOmegaWord<A::Symbol>> + '_ {
        self.negative.iter()
    }

    /// Gives the size i.e. number of positive words.
    pub fn positive_size(&self) -> usize {
        self.positive.len()
    }

    /// Gives the size i.e. number of negative words.
    pub fn negative_size(&self) -> usize {
        self.negative.len()
    }

    /// Returns the size i.e. number of words.
    pub fn size(&self) -> usize {
        self.positive_size() + self.negative_size()
    }

    /// Returns a reference to the alphabet underlying the sample.
    pub fn alphabet(&self) -> &A {
        &self.alphabet
    }

    /// Classify the given word, i.e. return `true` if it is a positive word, `false` if it is a negative word and `None` if it is neither.
    pub fn classify<W: Into<PeriodicOmegaWord<A::Symbol>>>(&self, word: W) -> Option<bool> {
        let word = word.into();
        if self.positive.contains(&word) {
            Some(true)
        } else if self.negative.contains(&word) {
            Some(false)
        } else {
            None
        }
    }

    /// Check whether the given word is contained in the sample.
    pub fn contains<W: Into<PeriodicOmegaWord<A::Symbol>>>(&self, word: W) -> bool {
        self.classify(word).is_some()
    }
}

impl<A: Alphabet> OmegaSample<A> {
    /// Create a new sample of infinite words. The words in the sample are given as an iterator yielding (word, color) pairs.
    pub fn new_omega<W: Into<ReducedOmegaWord<A::Symbol>>, J: IntoIterator<Item = (W, bool)>>(
        alphabet: A,
        words: J,
    ) -> Self {
        let (positive, negative) = words.into_iter().partition_map(|(w, b)| {
            if b {
                Either::Left(w.into())
            } else {
                Either::Right(w.into())
            }
        });

        Self {
            alphabet,
            positive,
            negative,
        }
    }

    /// Splits the sample into a map of [`ClassOmegaSample`]s, one for each class of the underlying [`RightCongruence`].
    pub fn split<'a>(&self, cong: &'a RightCongruence<A>) -> SplitOmegaSample<'a, A> {
        debug_assert!(
            cong.size() > 0,
            "Makes only sense for non-empty congruences"
        );
        let initial = cong.initial();
        // take self as is for epsilon
        let mut split = math::OrderedMap::default();
        split.insert(
            initial,
            ClassOmegaSample::new(cong, Class::epsilon(), self.clone()),
        );
        let mut queue: VecDeque<_> = self
            .entries()
            .map(|(w, c)| (initial, w.reduced(), c))
            .collect();

        while let Some((state, word, color)) = queue.pop_front() {
            trace!("Processing word {:?} in state {}", word, state);
            let (sym, suffix) = word.pop_first();
            // unwrap okay because words are infinite
            if let Some(reached) = cong.successor_index(state, sym) {
                trace!("\tReached successor {reached}");
                if split
                    .entry(reached)
                    .or_insert_with(|| {
                        ClassOmegaSample::empty(
                            cong,
                            Class::from(cong.state_to_mr(reached).unwrap().clone().into_inner()),
                            self.alphabet.clone(),
                        )
                    })
                    .sample_mut()
                    .insert(suffix.reduced(), color)
                {
                    trace!("Added word {:?} to state {}", suffix, reached);
                    queue.push_back((reached, suffix.reduced(), color));
                }
            }
        }

        SplitOmegaSample::new(cong, split)
    }
}
