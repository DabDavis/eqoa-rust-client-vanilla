//! The client binary. For now, the disc and container inspection commands the
//! first slice is built on:
//!
//! ```text
//! eqoa ls   [iso]               every file on the disc
//! eqoa tree [iso] <file>        a container's record tree
//! ```
//!
//! `iso` defaults to `$EQOA_ISO`, then `run/eqoa.iso`.

use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match run(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("eqoa: {e}");
            ExitCode::FAILURE
        }
    }
}

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

fn disc(arg: Option<&String>) -> Result<iso::Iso> {
    let path = arg
        .cloned()
        .or_else(|| std::env::var("EQOA_ISO").ok())
        .unwrap_or_else(|| "run/eqoa.iso".into());
    Ok(iso::Iso::open(&path).map_err(|e| format!("{path}: {e}"))?)
}

fn run(args: &[String]) -> Result<()> {
    match args.first().map(String::as_str) {
        Some("ls") => {
            for e in disc(args.get(1))?.entries() {
                println!("{:>10}  {:>7}  {}", e.size, e.lba, e.path);
            }
        }
        Some("tree") => {
            let (iso_arg, file) = match args.len() {
                2 => (None, &args[1]),
                3 => (Some(&args[1]), &args[2]),
                _ => return Err("usage: eqoa tree [iso] <file>".into()),
            };
            let c = esf::Container::open(disc(iso_arg)?.read(file)?)?;
            for &r in &c.roots {
                print_tree(&c, r, 0);
            }
            eprintln!("{} records, {} DictIDs, {} surfaces", c.nodes.len(), c.dict.len(), c.surfaces.len());
        }
        _ => return Err("usage: eqoa ls [iso] | eqoa tree [iso] <file>".into()),
    }
    Ok(())
}

fn print_tree(c: &esf::Container, id: esf::NodeId, depth: usize) {
    let n = c.node(id);
    println!("{:indent$}{:04X} v{} {} bytes", "", n.ty, n.version, n.size, indent = depth * 2);
    for &k in &n.children {
        print_tree(c, k, depth + 1);
    }
}
