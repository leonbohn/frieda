use automata::alphabet::{FreeMonoid, Symbol};
use automata::word::{PeriodicOmegaWord, Rotate};

fn compute<S: Symbol>(symbols: Vec<S>, max_length: usize) -> (Vec<PeriodicOmegaWord<S>>, usize) {
    let mut unique = Vec::new();
    let mut neg = 0;

    'outer: for w in FreeMonoid::non_empty(symbols) {
        if w.len() >= max_length {
            break;
        }
        let periodic = PeriodicOmegaWord::new(w);

        if unique.contains(&periodic) {
            neg += 1;
            continue 'outer;
        }

        for rot in periodic.rotations() {
            let periodic = PeriodicOmegaWord::new(rot);
            if unique.contains(&periodic) {
                neg += 1;
                continue 'outer;
            }
        }
        unique.push(periodic);
    }
    (unique, neg)
}

fn main() {
    let symbols = vec!['a', 'b', 'c', 'd', 'e', 'f', 'g'];

    for i in 2..10 {
        let (unique, neg) = compute(symbols.clone(), i);
        println!(
            "length {i}\n{} | {} that is {}%",
            unique.len(),
            neg,
            100.0 * unique.len() as f64 / (neg as f64 + unique.len() as f64)
        )
    }
}
