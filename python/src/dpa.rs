use automata::{
    TransitionSystem,
    automaton::DPA,
    core::{Void, alphabet::CharAlphabet},
    ts::{Shrinkable, Sproutable},
};
use pyo3::prelude::*;

#[pyclass(name = "DPA")]
pub struct PyDPA {
    inner: DPA,
}

#[pymethods]
impl PyDPA {
    #[new]
    #[pyo3(signature = (alphabet_size))]
    pub fn new(alphabet_size: u8) -> Self {
        Self {
            inner: DPA::new_with_initial_color(CharAlphabet::of_size(alphabet_size as usize), Void),
        }
    }

    pub fn size(&self) -> usize {
        self.inner.size()
    }

    pub fn add_state(&mut self) -> u32 {
        self.inner.add_state(Void)
    }

    /// Removes state with given id, returns `true` if some state is removed and `false` if no state was removed.
    pub fn remove_state(&mut self, id: u32) -> bool {
        self.inner.remove_state(id).is_some()
    }

    pub fn add_edge(
        &mut self,
        source: u32,
        symbol: char,
        priority: u8,
        target: u32,
    ) -> Option<u32> {
        self.inner
            .add_edge((source, symbol, priority, target))
            .map(|tup| tup.3)
    }

    pub fn remove_edge(&mut self, source: u32, symbol: char, target: u32) -> bool {
        let Some(removed) = self
            .inner
            .remove_edges_between_matching(source, target, symbol)
        else {
            return false;
        };
        assert!(removed.len() <= 1);
        removed.is_empty()
    }
}
