use std::collections::{BTreeSet, VecDeque};

use super::SccDecomposition;
use crate::TransitionSystem;
use crate::core::{
    alphabet::Expression,
    math,
    math::{OrderedMap, OrderedSet},
};
use crate::ts::connected_components::Scc;
use crate::ts::predecessors::PredecessorIterable;
use crate::ts::{IndexType, IsEdge, StateIndex};
use itertools::Itertools;

#[derive(Debug, Clone)]
struct TarjanData {
    rootindex: Option<usize>,
}

/// Struct that is used to compute the strongly connected components of a transition system.
/// Many thanks go out to the authors of the petgraph crate <3.
#[derive(Debug, Clone)]
pub struct Tarjan<Idx> {
    index: usize,
    component_count: usize,
    stack: Vec<Idx>,
    data: OrderedMap<Idx, TarjanData>,
}

impl<Idx: IndexType> Default for Tarjan<Idx> {
    fn default() -> Self {
        Self::new()
    }
}

impl<Idx: IndexType> Tarjan<Idx> {
    /// Creates a new Tarjan SCC decomposition instance.
    pub fn new() -> Self {
        Self {
            index: 0,
            component_count: usize::MAX,
            stack: Vec::new(),
            data: OrderedMap::default(),
        }
    }

    pub(crate) fn visit<Ts, F>(&mut self, ts: &Ts, v: Idx, f: &mut F)
    where
        Ts: TransitionSystem<StateIndex = Idx>,
        F: FnMut(&[Idx]),
    {
        let mut node_v_is_root = true;
        let node_v_index = self.index;
        self.data.insert(
            v,
            TarjanData {
                rootindex: Some(node_v_index),
            },
        );
        self.index += 1;

        if let Some(it) = ts.edges_from(v) {
            for edge in it {
                let q = edge.target();
                for _a in edge.expression().symbols() {
                    if self.data.get(&q).and_then(|data| data.rootindex).is_none() {
                        self.visit(ts, q, f);
                    }
                    let w_index = self.data.get(&q).unwrap().rootindex;
                    let v_mut = self.data.get_mut(&v).unwrap();
                    if w_index < v_mut.rootindex {
                        v_mut.rootindex = w_index;
                        node_v_is_root = false;
                    }
                }
            }
        }

        if node_v_is_root {
            let mut adjust = 1;
            let c = self.component_count;
            let node_data = &mut self.data;

            let start = self
                .stack
                .iter()
                .rposition(|w| {
                    if node_data.get(&v).unwrap().rootindex > node_data.get(w).unwrap().rootindex {
                        true
                    } else {
                        node_data.get_mut(w).unwrap().rootindex = Some(c);
                        adjust += 1;
                        false
                    }
                })
                .map(|x| x + 1)
                .unwrap_or_default();

            node_data.get_mut(&v).unwrap().rootindex = Some(c);
            self.stack.push(v);
            f(&self.stack[start..]);
            self.stack.truncate(start);
            self.index -= adjust;
            self.component_count -= 1;
        } else {
            self.stack.push(v);
        }
    }

    pub(crate) fn execute<Ts, F>(&mut self, ts: &Ts, mut f: F)
    where
        Ts: TransitionSystem<StateIndex = Idx>,
        F: FnMut(&[Idx]),
    {
        self.data.clear();

        for q in ts.state_indices() {
            if let Some(TarjanData { rootindex: None }) | None = self.data.get(&q) {
                self.visit(ts, q, &mut f);
            }
        }
        debug_assert!(self.stack.is_empty())
    }
}

/// Recursive application of Tarjan's algorithm form computing the SCC decomposition.
pub fn tarjan_scc_recursive<Ts>(ts: &Ts) -> SccDecomposition<'_, Ts>
where
    Ts: TransitionSystem,
{
    let mut sccs = Vec::new();
    {
        let mut tj = Tarjan::new();
        tj.execute(ts, |scc| sccs.push(Scc::new(ts, scc.to_vec())));
    }
    debug_assert!(
        sccs.iter().collect::<std::collections::HashSet<_>>().len() == sccs.len(),
        "All SCCs must be unique!"
    );
    sccs.sort();
    SccDecomposition::from_sccs(ts, sccs)
}

/// Implementation of Kosaraju's algorithm for computing the SCC decomposition.
pub fn kosaraju<Ts>(ts: &Ts, start: Ts::StateIndex) -> SccDecomposition<'_, Ts>
where
    Ts: TransitionSystem + PredecessorIterable,
{
    let mut visited = OrderedSet::from_iter([start]);
    let mut l = VecDeque::with_capacity(ts.size());
    let mut queue = Vec::with_capacity(ts.size());

    queue.push((start, ts.edges_from(start).unwrap()));

    'outer: while let Some((q, mut edges)) = queue.pop() {
        if let Some(edge) = edges.next() {
            // we proceed dfs-like on the edge
            let target = edge.target();
            if visited.insert(target) {
                queue.extend([(q, edges), (target, ts.edges_from(target).unwrap())]);
            } else {
                queue.push((q, edges));
            }
            continue 'outer;
        }
        // no more edges left, push q to front of l
        l.push_front(q);
    }

    // trace!("Computation for L gave {:?}", l);

    // now we do backwards dfs starting from the first element of l
    let reversed = (&ts).reversed();
    let mut sccs = vec![];

    while let Some(next) = l.pop_front() {
        let mut scc = vec![next];
        for v in reversed.reachable_state_indices_from(next) {
            // trace!("{v:?} is backward reachable from {next:?}");
            if let Some(pos) = l.iter().position(|x| x == &v) {
                l.remove(pos);
                scc.push(v);
            }
        }
        assert!(scc.contains(&next));
        sccs.push(Scc::new(ts, scc));
    }

    SccDecomposition::from_sccs(ts, sccs)
}

/// Calls [`tarjan_scc_iterative_from`] for the set of all states.
pub fn tarjan_scc_iterative<Ts>(ts: &Ts) -> SccDecomposition<'_, Ts>
where
    Ts: TransitionSystem,
{
    tarjan_scc_iterative_from(ts, ts.state_indices())
}

/// Iterative application of Tarjan's algorithm form computing the SCC decomposition.
pub fn tarjan_scc_iterative_from<Ts, I>(ts: &Ts, iter: I) -> SccDecomposition<'_, Ts>
where
    Ts: TransitionSystem,
    I: IntoIterator<Item = StateIndex<Ts>>,
{
    let mut current: usize;
    let mut sccs = vec![];

    let mut indices = OrderedMap::default();
    let mut low = OrderedMap::default();
    let mut stack = vec![];
    let mut on_stack = OrderedSet::default();
    let mut unvisited = iter.into_iter().collect::<BTreeSet<_>>();

    if unvisited.is_empty() {
        return SccDecomposition::from_sccs(ts, sccs);
    };
    let mut queue = Vec::new();

    let start_time = std::time::Instant::now();

    // we do an outermost loop that executes as long as states have not seen all states
    while let Some(&next) = unvisited.first() {
        assert!(queue.is_empty());
        // trace!("adding {next:?} to queue");
        current = 0;
        queue.push((
            next,
            ts.edges_from(next).expect("state is known to exist"),
            current,
        ));

        // loop through the current queue
        'outer: while let Some((q, mut edges, min)) = queue.pop() {
            // trace!(
            //     "considering state {q:?} with stored min {min}\nstack: {{{}}}\ton_stack: {{{}}}\nlows:{}",
            //     stack
            //         .iter()
            //         .map(|idx: &Ts::StateIndex| format!("{idx:?}"))
            //         .join(", "),
            //     on_stack
            //         .iter()
            //         .map(|idx: &Ts::StateIndex| format!("{idx:?}"))
            //         .join(", "),
            //     low.iter()
            //         .map(|(k, v): (&Ts::StateIndex, &usize)| format!("{k:?} -> {v}"))
            //         .join(", ")
            // );
            unvisited.remove(&q);

            if on_stack.insert(q) {
                stack.push(q);
            }

            if let math::ordered_map::Entry::Vacant(e) = indices.entry(q) {
                // trace!("assigning index {current} to state {q:?}");
                e.insert(current);
                low.insert(q, current);
                current += 1;
            }

            'inner: while let Some(edge) = edges.next() {
                // we skip self loops
                if edge.target() == q {
                    continue 'inner;
                }

                // trace!(
                //     "considering edge {:?} --{}--> {:?}",
                //     edge.source(),
                //     edge.expression().show(),
                //     edge.target()
                // );
                let target = edge.target();
                if unvisited.contains(&target) {
                    // trace!(
                    //     "successor {target:?} on {} has not been visited, descending",
                    //     edge.expression().show()
                    // );
                    queue.push((q, edges, min));
                    queue.push((target, ts.edges_from(target).unwrap(), current));
                    continue 'outer;
                }
                if on_stack.contains(&target) {
                    let new_low = std::cmp::min(*low.get(&q).unwrap(), *low.get(&target).unwrap());
                    let (it, _itt) = stack.iter().skip_while(|&x| x != &target).tee();

                    // TODO: Could there be a more performant way of doing this?
                    for x in it {
                        *low.get_mut(x).unwrap() = new_low;
                    }
                    // trace!(
                    //     "successor {target:?} on {} was alread seen, assigning new minimum {new_low} to states {}",
                    //     edge.expression().show(),
                    //     itt.into_iter().map(|x| format!("{x:?}")).join(", ")
                    // );
                }
            }

            // reached when all edges have been explored
            let low_q = *low.get(&q).unwrap();
            if low_q == *indices.get(&q).unwrap() {
                // trace!("{q:?} has matching index and low {low_q}, extracting scc",);
                let mut scc = vec![];
                while on_stack.contains(&q) {
                    let top = stack.pop().unwrap();
                    low.insert(top, low_q);
                    on_stack.remove(&top);
                    scc.push(top);
                }
                let scc = Scc::new(ts, scc);
                // trace!("identified scc {:?}", scc);
                sccs.push(scc);
            }
        }
    }

    tracing::info!(
        "tarjan scc iterative took {}µs",
        start_time.elapsed().as_micros()
    );

    sccs.sort();
    SccDecomposition::from_sccs(ts, sccs)
}

#[cfg(test)]
mod tests {
    use std::{collections::HashSet, time::Instant};

    use super::tarjan_scc_iterative;
    use crate::Pointed;
    use crate::ts::{TSBuilder, TransitionSystem};
    use crate::ts::{
        connected_components::{kosaraju, tarjan_scc_recursive},
        predecessors::PredecessorIterable,
    };
    use automata_core::Void;
    use tracing::debug;

    #[test]
    fn tarjan_iterative() {
        let ts = TSBuilder::without_colors()
            .with_transitions([
                (0, 'a', Void, 0),
                (0, 'b', Void, 1),
                (0, 'c', Void, 2),
                (1, 'a', Void, 1),
                (2, 'a', Void, 3),
                (2, 'b', Void, 2),
                (3, 'a', Void, 2),
            ])
            .into_right_congruence(0);

        let rev = (&ts).reversed();
        let reachable = rev.reachable_state_indices_from(3).collect::<HashSet<_>>();

        assert!(reachable.contains(&3));
        assert!(reachable.contains(&2));

        let start = Instant::now();
        let _sccs = kosaraju(&ts, ts.initial());
        debug!("Kosaraju took {} microseconds", start.elapsed().as_micros());

        let start = Instant::now();
        let _sccs = tarjan_scc_recursive(&ts);
        debug!(
            "Tarjan recursive took {} microseconds",
            start.elapsed().as_micros()
        );

        let start = Instant::now();
        let sccs = tarjan_scc_iterative(&ts);
        debug!(
            "Tarjan iterative took {} microseconds",
            start.elapsed().as_micros()
        );
        assert_eq!(sccs[0], vec![0]);
        assert_eq!(sccs[1], vec![1]);
        assert_eq!(sccs[2], vec![2, 3]);
    }
}
