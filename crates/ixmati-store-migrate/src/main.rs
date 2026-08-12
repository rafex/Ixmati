use ixmati_store_migrate::{Mode, load_manifest, run};
use std::path::PathBuf;

fn usage() -> ! {
    eprintln!("uso: ixmati-store-migrate <plan|verify|execute> --manifest migration.toml");
    std::process::exit(2);
}

fn main() {
    let mut args = std::env::args().skip(1);
    let command = args.next().unwrap_or_else(|| usage());
    let mut manifest_path = None;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--manifest" => manifest_path = args.next().map(PathBuf::from),
            _ => usage(),
        }
    }
    let manifest_path = manifest_path.unwrap_or_else(|| usage());
    let mode = match command.as_str() {
        "plan" => Mode::Plan,
        "verify" => Mode::Verify,
        "execute" => Mode::Execute,
        _ => usage(),
    };
    match load_manifest(&manifest_path).and_then(|manifest| run(&manifest, mode)) {
        Ok(report) => println!(
            "{}",
            serde_json::to_string_pretty(&report).expect("report serializes")
        ),
        Err(error) => {
            eprintln!("ixmati-store-migrate: {error}");
            std::process::exit(1);
        }
    }
}
