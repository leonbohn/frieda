use csv::Writer;
use itertools::{Either, Itertools};
use rayon::prelude::*;
use std::{collections::HashMap, env, fs, path::PathBuf, time::Duration};
use tracing_subscriber::{Layer, layer::SubscriberExt, util::SubscriberInitExt};

use automata::alphabet::{Alphabet, CharAlphabet};
use automata::automaton::{BuchiCondition, DBA, DPA, MinEvenParityCondition, Semantics};
use automata::representation::CollectTs;
use automata::ts::Deterministic;
use automata::ts::run::InfiniteObserver;
use automata::word::{OmegaWord, ReducedOmegaWord};
use automata::{Color, Void, math, upw};
use automata::{
    DTS, TransitionSystem,
    automaton::InfiniteWordAutomaton,
    hoa::WriteHoa,
    random::{generate_random_dba, generate_random_dpa, generate_random_omega_words},
};
use automata_learning::passive::{
    OmegaSample,
    sprout::{SproutError, sprout},
};
use tracing::{info, warn};

fn main() {
    // initialize logger
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::fmt::layer()
                .pretty()
                .with_writer(std::io::stdout)
                .with_writer(std::io::stderr)
                .with_filter(tracing_subscriber::filter::LevelFilter::INFO),
        )
        .init();

    let args: Vec<String> = env::args().collect();
    if args.contains(&"gen".to_string()) {
        // (re)generate tasks
        info!("Start generating learning tasks");
        let automata_sizes = vec![4, 8, 16];
        let automata_per_size = 10;
        let train_sizes = vec![100, 1000, 10000];
        let test_size = 10000;
        let num_sets = 5;
        let lambda = 0.95;
        generate_tasks(
            automata_sizes,
            automata_per_size,
            train_sizes,
            test_size,
            num_sets,
            lambda,
        );
    }
    if args.contains(&"sprout".to_string()) {
        info!("Running sprout learner on all tasks");
        run_sprout();
    }
    info!("Done");
}

pub fn run_sprout() {
    // load task directories
    let mut task_dirs = vec![];
    let entries = fs::read_dir("data/tasks").expect("No learning tasks available");

    for entry in entries.flatten() {
        if let Ok(file_type) = entry.file_type() {
            if file_type.is_dir() {
                task_dirs.push(entry.path());
            }
        } else {
            warn!("Couldn't get file type for {:?}", entry.path());
        }
    }
    task_dirs
        .clone()
        // .into_iter()
        .into_par_iter()
        .map(load_sample)
        .enumerate()
        .for_each(|(i, sample)| {
            let dir = &task_dirs[i];
            // check if task was already computed
            if dir.join("result.csv").exists() {
                info!("already done for task {i} {:?}", dir.to_string_lossy());
                return;
            }

            if dir.to_string_lossy().contains("dba") {
                let time = std::time::Instant::now();
                info!(
                    "starting DBA learner for task {i} {:?}",
                    dir.to_string_lossy()
                );
                match sprout(sample, BuchiCondition) {
                    Ok(learned) => {
                        let elapsed = time.elapsed();
                        info!(
                            "task {i} \"{:?}\" learning took {} ms",
                            dir.to_string_lossy(),
                            elapsed.as_millis()
                        );
                        export_automaton(
                            format!("{}/learned.hoa", dir.to_str().unwrap()),
                            &learned,
                        );
                        export_sprout_result(dir, &learned, elapsed);
                    }
                    Err(SproutError::Threshold {
                        thres: _thres,
                        aut: learned,
                    }) => {
                        let elapsed = time.elapsed();
                        info!(
                            "task {i} \"{:?}\" exceeded threshold after {} ms",
                            dir.to_string_lossy(),
                            elapsed.as_millis()
                        );
                        export_automaton(
                            format!("{}/learned.hoa", dir.to_str().unwrap()),
                            &learned,
                        );
                        export_sprout_result(dir, &learned, elapsed);
                    }
                    Err(SproutError::Timeout { aut: partial }) => {
                        let elapsed = time.elapsed();
                        info!(
                            "exceeded timeout on task {i} with partial ts of size {}: {:?}",
                            partial.size(),
                            dir.to_string_lossy()
                        );
                        export_sprout_timeout(dir, partial, elapsed);
                    }
                }
            } else {
                let time = std::time::Instant::now();
                info!(
                    "starting DPA learner for task {i} {:?}",
                    dir.to_string_lossy()
                );
                match sprout(sample, MinEvenParityCondition) {
                    Ok(learned) => {
                        let elapsed = time.elapsed();
                        info!(
                            "task {i} \"{:?}\" learning took {} ms",
                            dir.to_string_lossy(),
                            elapsed.as_millis()
                        );
                        export_automaton(
                            format!("{}/learned.hoa", dir.to_str().unwrap()),
                            &learned,
                        );
                        export_sprout_result(dir, &learned, elapsed);
                    }
                    Err(SproutError::Threshold {
                        thres: _thres,
                        aut: learned,
                    }) => {
                        let elapsed = time.elapsed();
                        info!(
                            "task {i} \"{:?}\" exceeded threshold after {} ms",
                            dir.to_string_lossy(),
                            elapsed.as_millis()
                        );
                        export_automaton(
                            format!("{}/learned.hoa", dir.to_str().unwrap()),
                            &learned,
                        );
                        export_sprout_result(dir, &learned, elapsed);
                    }
                    Err(SproutError::Timeout { aut: partial }) => {
                        let elapsed = time.elapsed();
                        info!(
                            "exceeded timeout on task {i} with partial ts of size {}: {:?}",
                            partial.size(),
                            dir.to_string_lossy()
                        );
                        export_sprout_timeout(dir, partial, elapsed);
                    }
                }
            }
        });
}

/// Generate a sample of ultimately periodic words by loading the training set from
/// the learning task located in the given dircetory.
pub fn load_sample(dir: PathBuf) -> OmegaSample {
    let mut rdr = csv::Reader::from_path(dir.join("train.csv")).expect("No training set found");
    let (pos_words, neg_words): (Vec<_>, Vec<_>) = rdr.deserialize().partition_map(|result| {
        let (spoke, cycle, class): (String, String, bool) =
            result.expect("Failed to read training set");
        let word = upw!(spoke, cycle);
        if class {
            Either::Left(word)
        } else {
            Either::Right(word)
        }
    });

    // todo: check for size of alphabet
    let alphabet = CharAlphabet::of_size(2);
    OmegaSample::new_omega_from_pos_neg(alphabet, pos_words, neg_words)
}

/// generate set of learning tasks for DBA and DPA.
pub fn generate_tasks(
    automata_sizes: Vec<usize>,
    automata_per_size: usize,
    train_sizes: Vec<usize>,
    test_size: usize,
    num_sets: usize,
    lambda: f64,
) {
    // set parameters
    let num_symbols = 2;
    let num_prios = 5;
    fs::create_dir_all("data/automata").unwrap();
    fs::create_dir_all("data/sets").unwrap();

    // generate DBAs
    info!("generating DBAs");
    let mut dbas = HashMap::new();
    for &size in automata_sizes.iter() {
        let mut auts = vec![];
        for i in 0..automata_per_size {
            let dba = generate_dba(num_symbols, size, lambda);
            export_automaton(aut_name(size, i, "dba".to_string()), &dba);
            auts.push(dba);
        }
        dbas.insert(size, auts);
    }

    // generate DPAs
    info!("generating DPAs");
    let mut dpas = HashMap::new();
    for &size in automata_sizes.iter() {
        let mut auts = vec![];
        for i in 0..automata_per_size {
            let dpa = generate_dpa(num_symbols, size, num_prios, lambda);
            export_automaton(aut_name(size, i, "dpa".to_string()), &dpa);
            auts.push(dpa);
        }
        dpas.insert(size, auts);
    }

    // generate train and test sets
    info!("generating word sets");
    let mut sets_dba = HashMap::new();
    let mut sets_dpa = HashMap::new();
    for &aut_size in automata_sizes.iter() {
        for &train_size in train_sizes.iter() {
            let mut sets_of_size_dba = vec![];
            let mut sets_of_size_dpa = vec![];
            for i in 0..num_sets {
                // DBA sets
                let len_spoke = std::cmp::max(8, aut_size);
                let len_cycle = std::cmp::max(8, aut_size);
                let (train, test) =
                    generate_set(num_symbols, len_spoke, len_cycle, train_size, test_size);
                export_set(set_name(aut_size, train_size, i, true, "dba"), &train);
                export_set(set_name(aut_size, train_size, i, false, "dba"), &test);
                sets_of_size_dba.push((train, test));
                // DPA sets
                let len_spoke = 2 * ((aut_size as f64).log2().ceil() as usize) - 1;
                let len_cycle = (2 * aut_size - len_spoke) * len_spoke;
                let (train, test) =
                    generate_set(num_symbols, len_spoke, len_cycle, train_size, test_size);
                export_set(set_name(aut_size, train_size, i, true, "dpa"), &train);
                export_set(set_name(aut_size, train_size, i, false, "dpa"), &test);
                sets_of_size_dpa.push((train, test));
            }
            sets_dba.insert((aut_size, train_size), sets_of_size_dba);
            sets_dpa.insert((aut_size, train_size), sets_of_size_dpa);
        }
    }

    // label dba sets
    info!("labelling dba sets");
    for &aut_size in automata_sizes.iter() {
        for (aut_index, dba) in dbas[&aut_size].iter().enumerate() {
            for &train_size in train_sizes.iter() {
                for (set_index, (tr, te)) in sets_dba[&(aut_size, train_size)].iter().enumerate() {
                    let train = label_set(dba, tr);
                    let test = label_set(dba, te);
                    // export as learning task
                    export_task(
                        task_name(
                            aut_size,
                            train_size,
                            aut_index,
                            set_index,
                            "dba".to_string(),
                        ),
                        dba,
                        &train,
                        &test,
                    );
                }
            }
        }
    }

    // label dpa sets
    info!("labelling dpa sets");
    for &aut_size in automata_sizes.iter() {
        for (aut_index, dpa) in dpas[&aut_size].iter().enumerate() {
            for &train_size in train_sizes.iter() {
                for (set_index, (tr, te)) in sets_dpa[&(aut_size, train_size)].iter().enumerate() {
                    let train = label_set(dpa, tr);
                    let test = label_set(dpa, te);
                    // export as learning task
                    export_task(
                        task_name(
                            aut_size,
                            train_size,
                            aut_index,
                            set_index,
                            "dpa".to_string(),
                        ),
                        dpa,
                        &train,
                        &test,
                    );
                }
            }
        }
    }
}

/// Generate a random DBA with `aut_size` states on an alphabet of size `num_symbols`.
/// The parameter `lambda` is used to draw the acceptance condition from a continuous Bernoulli distribution.
/// The produced automaton has an informative right congruence.
pub fn generate_dba(num_symbols: usize, aut_size: usize, lambda: f64) -> DBA {
    let gen_size = aut_size + (aut_size as f32).log2().round() as usize - 1;
    let mut dba: DBA;
    loop {
        dba = generate_random_dba(num_symbols, gen_size, lambda).streamlined();
        // check if dba has the correct size
        if dba.size() == aut_size {
            let equiv_dpa = dba
                .clone()
                .map_edge_colors(|c| if c { 0 } else { 1 })
                .collect_dpa();
            // check if automaton has informative right congruence
            if equiv_dpa.is_informative_right_congruent() {
                break;
            }
        }
    }
    dba
}

/// Generate a random DPA with `aut_size` states on an alphabet of size `num_symbols`.
/// The parameter `lambda` is used to draw the acceptance condition from a continuous Bernoulli distribution.
/// The produced automaton has an informative right congruence.
pub fn generate_dpa(num_symbols: usize, aut_size: usize, num_prios: u8, lambda: f64) -> DPA {
    let gen_size = aut_size + (aut_size as f32).log2().round() as usize - 1;
    let mut dpa: DPA;
    loop {
        dpa = generate_random_dpa(num_symbols, gen_size, num_prios, lambda)
            .streamlined()
            .collect_dpa();
        // check if dpa has the correct size and has informative right congruence
        if dpa.size() == aut_size && dpa.is_informative_right_congruent() {
            break;
        }
    }
    dpa
}

/// Generate a training set, test set pair of random ultimately periodic words.
/// The length of spoke and cycle are drawn uniformly and the used alphabet is of size `num_symbols`.
pub fn generate_set(
    num_symbols: usize,
    len_spoke: usize,
    len_cycle: usize,
    train_size: usize,
    test_size: usize,
) -> (
    math::Set<ReducedOmegaWord<char>>,
    math::Set<ReducedOmegaWord<char>>,
) {
    let alphabet = CharAlphabet::of_size(num_symbols);
    let mut training_set = generate_random_omega_words(
        &alphabet,
        0,
        len_spoke,
        1,
        len_cycle,
        train_size + test_size,
    );
    let test_set = training_set.split_off(train_size);
    (training_set, test_set)
}

/// Label a `set` of [`ReducedOmegaWord`]s with the result of the given automaton.
pub fn label_set<Z, C>(
    aut: &InfiniteWordAutomaton<CharAlphabet, Z, Void, C, true>,
    set: &math::Set<ReducedOmegaWord<char>>,
) -> Vec<(ReducedOmegaWord<char>, bool)>
where
    Z: Semantics<DTS<CharAlphabet, Void, C>, true, Output = bool>,
    Z::Observer: InfiniteObserver<DTS<CharAlphabet, Void, C>>,
    C: Color,
{
    set.into_iter()
        .map(|w| (w.clone(), aut.accepts(w)))
        .collect()
}

/// Write the given automaton to the given `path` in HOA format
pub fn export_automaton<AUT: WriteHoa>(file: String, aut: &AUT) {
    fs::write(file, aut.to_hoa()).expect("Unable to write file");
}

/// Give filename for an omega automaton
pub fn aut_name(aut_size: usize, aut_index: usize, acc_type: String) -> String {
    format!("data/automata/{acc_type}__aut_size={aut_size}__{aut_index:0>2}.hoa")
}

/// Write the given set to the given `path` as csv
pub fn export_set(file: String, set: &math::Set<ReducedOmegaWord<char>>) {
    let mut wtr = Writer::from_path(file).expect("creating file failed");
    for w in set.iter() {
        wtr.write_record(&[
            w.spoke().iter().collect::<String>(),
            w.cycle().iter().collect(),
        ])
        .unwrap();
    }
    wtr.flush().unwrap();
}

/// Load labelled set from csv. Split into positively and negatively labelled words
pub fn load_set(
    path: &std::path::Path,
    file: String,
) -> (Vec<ReducedOmegaWord<char>>, Vec<ReducedOmegaWord<char>>) {
    let mut rdr = csv::Reader::from_path(path.join(file)).expect("No training set found");
    rdr.deserialize().partition_map(|result| {
        let (spoke, cycle, class): (String, String, bool) =
            result.expect("Failed to read training set");
        let word = upw!(spoke, cycle);
        if class {
            Either::Left(word)
        } else {
            Either::Right(word)
        }
    })
}

/// Give filename for a set of omega words
pub fn set_name(
    aut_size: usize,
    set_size: usize,
    set_index: usize,
    train: bool,
    acc_type: &str,
) -> String {
    let class = if train { "train" } else { "test" };
    format!(
        "data/sets/word_set__{acc_type}__aut_size={aut_size}__sample_size={set_size}__{set_index:0>2}_{class}.csv"
    )
}

pub fn export_labelled_set(file: String, set: &[(ReducedOmegaWord<char>, bool)]) {
    let mut wtr = Writer::from_path(file).expect("creating file failed");
    wtr.write_record(["spoke", "cycle", "acceptance"]).unwrap();
    for (w, r) in set.iter() {
        wtr.write_record([
            w.spoke().iter().collect(),
            w.cycle().iter().collect(),
            format!("{r:?}"),
        ])
        .unwrap();
    }
    wtr.flush().unwrap();
}

/// Write the given omega automata learning task to the given `path` in HOA format
pub fn export_task<AUT: WriteHoa>(
    name: String,
    aut: &AUT,
    train: &[(ReducedOmegaWord<char>, bool)],
    test: &[(ReducedOmegaWord<char>, bool)],
) {
    // remove old results if they exist
    let _ = fs::remove_dir_all(format!("data/tasks/{name}"));
    // export new data
    fs::create_dir_all(format!("data/tasks/{name}")).unwrap();
    export_automaton(format!("data/tasks/{name}/aut.hoa"), aut);
    export_labelled_set(format!("data/tasks/{name}/train.csv"), train);
    export_labelled_set(format!("data/tasks/{name}/test.csv"), test);
    export_settings(
        format!("data/tasks/{name}/settings.txt"),
        name,
        aut.alphabet().size(),
        aut.size(),
        train.len(),
        test.len(),
    );
}

pub fn task_name(
    aut_size: usize,
    set_size: usize,
    aut_index: usize,
    set_index: usize,
    acc_type: String,
) -> String {
    format!(
        "{acc_type}_task__aut_size={aut_size:0>2}__sample_size={set_size:0>5}__{acc_type}{aut_index:0>2}__sample{set_index:0>2}"
    )
}

pub fn export_settings(
    file: String,
    name: String,
    num_symbols: usize,
    aut_size: usize,
    train_size: usize,
    test_size: usize,
) {
    let acc_type = if name.contains("dba") { "dba" } else { "dpa" };
    let mut wtr = Writer::from_path(file).expect("creating file failed");
    wtr.write_record(["name", &name]).unwrap();
    wtr.write_record(["aut_type", acc_type]).unwrap();
    wtr.write_record(["num_symbols", &format!("{num_symbols}")])
        .unwrap();
    wtr.write_record(["aut_size", &format!("{aut_size}")])
        .unwrap();
    wtr.write_record(["train_size", &format!("{train_size}")])
        .unwrap();
    wtr.write_record(["test_size", &format!("{test_size}")])
        .unwrap();
    wtr.flush().unwrap();
}

pub fn export_sprout_timeout(
    task_dir: &std::path::Path,
    partial: impl Deterministic,
    elapsed: Duration,
) {
    let mut wtr = Writer::from_path(task_dir.join("result.csv")).expect("creating file failed");
    wtr.write_record(["abort_automaton_size", &partial.size().to_string()])
        .unwrap();
    wtr.write_record(["time_ms", &format!("{}", elapsed.as_millis())])
        .unwrap();
    wtr.flush().unwrap();
}

/// Score test set and export results
pub fn export_sprout_result<Z, C>(
    task_dir: &std::path::Path,
    learned: &InfiniteWordAutomaton<CharAlphabet, Z, Void, C, true>,
    elapsed: Duration,
) where
    Z: Semantics<DTS<CharAlphabet, Void, C>, true, Output = bool>,
    Z::Observer: InfiniteObserver<DTS<CharAlphabet, Void, C>>,
    C: Color,
{
    let start = std::time::Instant::now();
    // load test set
    let (test_pos, test_neg) = load_set(task_dir, "test.csv".to_string());
    let pos_count = test_pos.len();
    let neg_count = test_neg.len();
    let test_size = pos_count + neg_count;
    // score test set
    let mut scored_pos = label_set(learned, &test_pos.into_iter().collect());
    let scored_neg = label_set(learned, &test_neg.into_iter().collect());
    let pos_correct = scored_pos.iter().filter(|(_, b)| *b).count();
    let neg_correct = scored_neg.iter().filter(|(_, b)| !b).count();
    let total_correct = pos_correct + neg_correct;

    let path_str = task_dir.to_str().unwrap();
    scored_pos.extend(scored_neg);
    export_labelled_set(format!("{}/test_learned.csv", path_str), &scored_pos);

    // export %correct, %pos/neg correct, aut size in result file
    let mut wtr = Writer::from_path(task_dir.join("result.csv")).expect("creating file failed");
    wtr.write_record(["learned_aut_size", &format!("{}", learned.size())])
        .unwrap();
    wtr.write_record(["scored_correct", &format!("{total_correct}")])
        .unwrap();
    wtr.write_record([
        "scored_correct%",
        &format!("{}", total_correct as f64 / test_size as f64),
    ])
    .unwrap();
    wtr.write_record(["pos_correct", &format!("{pos_correct}")])
        .unwrap();
    wtr.write_record([
        "pos_correct%",
        &format!("{}", pos_correct as f64 / pos_count as f64),
    ])
    .unwrap();
    wtr.write_record(["neg_correct", &format!("{neg_correct}")])
        .unwrap();
    wtr.write_record([
        "neg_correct%",
        &format!("{}", neg_correct as f64 / neg_count as f64),
    ])
    .unwrap();
    wtr.write_record(["pos_count", &format!("{pos_count}")])
        .unwrap();
    wtr.write_record([
        "pos_count%",
        &format!("{}", pos_count as f64 / test_size as f64),
    ])
    .unwrap();
    wtr.write_record(["neg_count", &format!("{neg_count}")])
        .unwrap();
    wtr.write_record([
        "neg_count%",
        &format!("{}", neg_count as f64 / test_size as f64),
    ])
    .unwrap();
    wtr.write_record(["time_ms", &format!("{}", elapsed.as_millis())])
        .unwrap();
    wtr.flush().unwrap();
    info!(
        "exported sprout result in {} µs",
        start.elapsed().as_micros()
    );
}
