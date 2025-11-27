#[derive(Debug, Default)]
struct Stats {
    same: usize,
    same_non_zero: usize,
    diff: usize,
    diff_each_score: f32,
    diff_score: f32,
}

fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let mut args = std::env::args();

    let prog = args.next().expect("argv[0] is program name");
    let usage = || format!("{prog} <DATAFILE-A> <DATAFILE-B>");

    let a = args.next().ok_or_else(usage)?;
    let b = args.next().ok_or_else(usage)?;

    println!("Will diff a:{a} and b:{b}");

    println!("Loading {a}");
    let a = chuva::Chuva::load(a)?;
    println!("Loading {b}");
    let b = chuva::Chuva::load(b)?;

    let delta = (a.created_at - b.created_at)
        .total(jiff::Unit::Minute)
        .unwrap();
    assert_eq!(0, (delta as isize) % 5);
    let step = (delta / 5f64) as isize;
    if delta > 0f64 {
        println!("a is newer than b ({step})")
    } else if delta < 0f64 {
        println!("b is newer than a ({step})")
    }

    let mut stats = Stats::default();
    for offset in (0..chuva::MAX_OFFSET).step_by(chuva::STEPS) {
        let a = a.by_offset(offset).unwrap();
        let b = b.by_offset(offset).unwrap();

        let (a, b) = adjust(a, b, step);
        assert_eq!(a.len(), b.len());

        if a == b {
            stats.same += 1;

            if a.iter().sum::<f32>() != 0f32 {
                stats.same_non_zero += 1;
            }
            continue;
        }

        stats.diff += 1;
        stats.diff_each_score += each_score(a, b);
        stats.diff_score += other_score(a, b);

        let a_line = a.iter().map(|&v| spark(v)).collect::<String>();
        let b_line = b.iter().map(|&v| spark(v)).collect::<String>();
        eprintln!("a/{a_line}\nb/{b_line}\n")
    }

    stats.diff_each_score /= stats.diff as f32;
    stats.diff_score /= stats.diff as f32;

    println!("{stats:?}");

    Ok(())
}

const fn spark(mmhr: f32) -> char {
    // TODO figure out good buckets? this is pure yolo
    //      so maybe look at yearly stats and slice
    //      according to the distribution?
    if mmhr < 0f32.next_up() {
        ' '
    } else if mmhr < 0.13 {
        '▁'
    } else if mmhr < 0.25 {
        '▂'
    } else if mmhr < 0.5 {
        '▃'
    } else if mmhr < 2.0 {
        '▄'
    } else if mmhr < 4.0 {
        '▅'
    } else if mmhr < 6.0 {
        '▆'
    } else if mmhr < 8.0 {
        '▇'
    } else {
        '█'
    }
}

fn adjust<'a>(mut a: &'a [f32], mut b: &'a [f32], step: isize) -> (&'a [f32], &'a [f32]) {
    if step < 0 {
        // b is newer
        let s = usize::try_from(-step).unwrap();
        a = &a[s..];
        b = &b[0..(b.len() - s)];
    } else if step > 0 {
        // a is newer
        let s = usize::try_from(step).unwrap();
        a = &a[0..(a.len() - s)];
        b = &b[s..];
    }

    (a, b)
}

fn other_score(a: &[f32], b: &[f32]) -> f32 {
    a.iter()
        .zip(b.iter())
        .map(|(a, b)| a - b)
        .sum::<f32>()
        .abs()
}

fn each_score(a: &[f32], b: &[f32]) -> f32 {
    a.iter().zip(b.iter()).map(|(a, b)| (a - b).abs()).sum()
}
