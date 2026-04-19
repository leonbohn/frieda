use super::{ConsumingInfixIterator, FiniteWord, OmegaWord, ReducedOmegaWord, Word};

/// A suffix of a [`Word`] which skips a fixed number of symbols. If the underlying
/// word is infinite, the suffix is also infinite. If the underlying word is finite, the suffix
/// is also finite.
#[derive(Clone, PartialEq, Debug, Hash, Eq)]
pub struct Skip<'a, W: Word> {
    sequence: &'a W,
    offset: usize,
}

impl<'a, W: Word> Skip<'a, W> {
    /// Creates a new suffix, which skips the first `offset` symbols of the given sequence.
    pub fn new(sequence: &'a W, offset: usize) -> Self {
        Self { sequence, offset }
    }
}

impl<W: Word> Word for Skip<'_, W> {
    type Symbol = W::Symbol;
    const FINITE: bool = W::FINITE;
    fn nth(&self, position: usize) -> Option<W::Symbol> {
        self.sequence.nth(self.offset + position)
    }
}

impl<W: FiniteWord> FiniteWord for Skip<'_, W> {
    type Symbols<'this>
        = std::iter::Skip<W::Symbols<'this>>
    where
        Self: 'this;

    fn collect_vec(&self) -> Vec<W::Symbol> {
        (self.offset..self.sequence.len())
            .map(|position| self.sequence.nth(position).unwrap())
            .collect()
    }

    fn len(&self) -> usize {
        self.sequence.len().saturating_sub(self.offset)
    }

    fn symbols(&self) -> Self::Symbols<'_> {
        self.sequence.symbols().skip(self.offset)
    }
}

#[derive(Clone, PartialEq, Debug, Hash, Eq)]
pub struct Rotated<W>(pub W, pub usize);

impl<W: FiniteWord> Word for Rotated<W> {
    type Symbol = W::Symbol;
    const FINITE: bool = true;
    fn nth(&self, position: usize) -> Option<W::Symbol> {
        self.0.nth((position + self.1) % self.0.len())
    }
}

#[derive(Clone, PartialEq, Debug, Hash, Eq)]
pub struct RotatedIter<'a, W> {
    rotated: &'a Rotated<W>,
    start: usize,
    position: usize,
}

impl<'a, W> RotatedIter<'a, W> {
    pub fn new(rotated: &'a Rotated<W>, start: usize) -> Self {
        Self {
            rotated,
            start,
            position: 0,
        }
    }
}

impl<W: FiniteWord> Iterator for RotatedIter<'_, W> {
    type Item = W::Symbol;
    fn next(&mut self) -> Option<Self::Item> {
        if self.position < self.rotated.len() {
            let out = self
                .rotated
                .nth((self.start + self.position) % self.rotated.len());
            assert!(out.is_some());
            self.position += 1;
            out
        } else {
            None
        }
    }
}

impl<W: FiniteWord> FiniteWord for Rotated<W> {
    type Symbols<'this>
        = RotatedIter<'this, W>
    where
        Self: 'this;

    fn symbols(&self) -> Self::Symbols<'_> {
        RotatedIter::new(self, self.1)
    }

    fn collect_vec(&self) -> Vec<W::Symbol> {
        self.symbols().collect()
    }

    fn len(&self) -> usize {
        self.0.len()
    }
}

impl<W: OmegaWord> OmegaWord for Skip<'_, W> {
    type Spoke<'this>
        = Infix<'this, W>
    where
        Self: 'this;

    type Cycle<'this>
        = Infix<'this, W>
    where
        Self: 'this;

    fn reduced(&self) -> crate::word::ReducedOmegaWord<W::Symbol> {
        if self.offset >= self.sequence.spoke_len() {
            let mut period = self.sequence.cycle_vec();
            period
                .rotate_left((self.offset - self.sequence.spoke_len()) % self.sequence.cycle_len());
            ReducedOmegaWord::from_raw_parts(period, 0)
        } else {
            let representation: Vec<_> = self
                .sequence
                .spoke()
                .symbols()
                .skip(self.offset)
                .chain(self.cycle().symbols())
                .collect();
            ReducedOmegaWord::from_raw_parts(
                representation,
                self.sequence.spoke_len() - self.offset,
            )
        }
    }

    fn spoke(&self) -> Self::Spoke<'_> {
        if self.offset < self.sequence.loop_index() {
            self.sequence
                .infix(self.offset, self.sequence.loop_index() - self.offset)
        } else {
            self.sequence.infix(self.sequence.loop_index(), 0)
        }
    }

    fn cycle(&self) -> Self::Cycle<'_> {
        if self.offset < self.sequence.loop_index() {
            self.sequence
                .infix(self.sequence.loop_index(), self.sequence.cycle_len())
        } else {
            self.sequence.infix(
                self.sequence.loop_index()
                    + (self.offset.saturating_sub(self.sequence.loop_index())
                        % self.sequence.cycle_len()),
                self.sequence.cycle_len(),
            )
        }
    }
}

/// Represents an infix of a [`Word`]. This is a finite word, which is a subsequence of the
/// original word. It is specified by a starting position and a length, and stores a reference
/// to the underlying word.
#[derive(Clone, PartialEq, Debug, Hash, Eq)]
pub struct Infix<'a, W: Word + ?Sized> {
    sequence: &'a W,
    offset: usize,
    length: usize,
}

impl<'a, W: Word + ?Sized> Infix<'a, W> {
    /// Creates a new suffix, which skips the first `offset` symbols of the given sequence.
    pub fn new(sequence: &'a W, offset: usize, length: usize) -> Self {
        Self {
            sequence,
            offset,
            length,
        }
    }
}

impl<W: Word> Word for Infix<'_, W> {
    type Symbol = W::Symbol;
    const FINITE: bool = true;
    fn nth(&self, position: usize) -> Option<W::Symbol> {
        if position < self.length {
            self.sequence.nth(self.offset + position)
        } else {
            None
        }
    }
}

impl<W: Word> FiniteWord for Infix<'_, W> {
    type Symbols<'this>
        = ConsumingInfixIterator<'this, W>
    where
        Self: 'this;

    fn symbols(&self) -> Self::Symbols<'_> {
        ConsumingInfixIterator::new(self.sequence, self.offset, self.offset + self.length)
    }

    fn collect_vec(&self) -> Vec<W::Symbol> {
        (self.offset..(self.offset + self.length))
            .map(|position| self.sequence.nth(position).unwrap())
            .collect()
    }

    fn len(&self) -> usize {
        self.length
    }
}

#[cfg(test)]
mod tests {

    use itertools::Itertools;

    use crate::{
        upw,
        word::{FiniteWord, OmegaWord, ReducedOmegaWord, Word},
    };

    #[test]
    fn finite_word_infix() {
        let fw = "abcde".to_string();
        assert_eq!(fw.infix(1, 3).collect_vec(), vec!['b', 'c', 'd']);
        assert_eq!(fw.infix(1, 3).as_string(), "bcd".to_string());
    }

    #[test]
    fn skip_to_reduced_omega_word() {
        assert_eq!(upw!("bbb", "a").skip(2).reduced(), upw!("b", "a"));
    }

    #[test]
    fn subwords() {
        let word = ReducedOmegaWord::periodic("abab");
        let pref = word.prefix(2);
        assert_eq!(pref.symbols().collect_vec(), vec!['a', 'b']);

        let word = upw!("ab", "ac");
        assert_eq!(
            word.skip(3).prefix(4).collect_vec(),
            vec!['c', 'a', 'c', 'a']
        );
        assert_eq!(
            word.skip(1)
                .skip(1)
                .skip(1)
                .skip(1)
                .skip(4)
                .prefix(2)
                .collect_vec(),
            vec!['a', 'c']
        );
        let w = word.skip(3);
        assert!(w.spoke().is_empty());
        assert_eq!(w.cycle().collect_vec(), vec!['c', 'a']);

        let offset_normalized = upw!("abba").skip(1).skip(20).reduced();
        assert!(offset_normalized.spoke().is_empty());
        assert_eq!(offset_normalized.cycle().to_vec(), vec!['b', 'b', 'a', 'a']);
    }
}
