mod backend;
mod harness;
mod probe;

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

fn main() {
    if let Err(error) = run() {
        eprintln!("sandbox-lab: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) == Some("--probe") {
        return probe::run(&args[1..]);
    }
    if args.is_empty() || args.iter().any(|a| a == "--help") {
        println!("Usage: cargo run --manifest-path tools/sandbox-lab/Cargo.toml -- \\\n  --java-home /absolute/jdk --report /path/report.json [--lwjgl /path/to/jars]\n\n\
Runs a trusted fixture outside and inside the native sandbox.\n\
Supported lab backends: macOS Seatbelt; Linux bubblewrap (no seccomp yet).\n\
Windows continues to use src-tauri's existing sandbox_probe.");
        return Ok(());
    }
    harness::run(&args)
}
