use chuva::{Chuva, MAX_OFFSET, ModelKind, STEPS};

fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let mut args = std::env::args();

    let prog = args.next().expect("argv[0] is program name");
    let usage = || format!("{prog} <ensemble|simple> <DATAFILE>");

    let mode = args.next().ok_or_else(usage)?;
    let file = args.next().ok_or_else(usage)?;

    let (plain, nd) = match mode.as_str() {
        "ensemble" => {
            let plain = Chuva::load_kind(&file, ModelKind::Ensemble)?;
            let nd = Chuva::load_kind(&file, ModelKind::EnsembleNdarray)?;
            (plain, nd)
        }
        "simple" => {
            let plain = Chuva::load_kind(&file, ModelKind::Simple)?;
            let nd = Chuva::load_kind(&file, ModelKind::SimpleNdarray)?;
            (plain, nd)
        }
        _ => return Err(usage().into()),
    };

    for offset in (0..MAX_OFFSET).step_by(STEPS) {
        let a = plain.by_offset(offset).unwrap();
        let b = nd.by_offset(offset).unwrap();
        if a != b {
            println!("offset={offset}\nen={a:?}\nnd={b:?}");
        }
    }

    Ok(())
}
