//! `roves-content-packer` — see ../../../CUSTOMIZATIONS.md for why this
//! exists. Two subcommands:
//!
//! - `pack`: turns a built `dist/`-shaped directory into a handful of
//!   `.pack` archives (tar, zstd-compressed unless made entirely of
//!   already-compressed extensions) plus a `manifest.json`, instead of
//!   shipping it as loose files.
//! - `extract`: reverses that, decompressing back to plain files — used at
//!   game launch time (see the generated `play.sh`/`play.exe`/`Roves.app`
//!   launchers) rather than at bundle/build time, so the release artifact
//!   itself never contains the plain, browsable dist tree.

mod extract;
mod manifest;
mod pack;
mod size;

use std::path::PathBuf;
use std::process::ExitCode;

use bpaf::Parser;

#[derive(Debug, Clone)]
enum Cmd {
    Pack(PackCli),
    Extract(ExtractCli),
}

#[derive(Debug, Clone)]
struct PackCli {
    input: PathBuf,
    output: PathBuf,
    level: i32,
    max_pack_size: String,
    exclude: Vec<String>,
}

#[derive(Debug, Clone)]
struct ExtractCli {
    content_dir: PathBuf,
    dest: Option<PathBuf>,
    force: bool,
}

fn pack_cli() -> impl Parser<PackCli> {
    let input = bpaf::long("input")
        .help("Directory to pack, e.g. a built dist/")
        .argument::<PathBuf>("DIR");
    let output = bpaf::long("output")
        .help("Directory to write .pack files + manifest.json into")
        .argument::<PathBuf>("DIR");
    let level = bpaf::long("level")
        .help("zstd compression level — low values favor speed over ratio")
        .argument::<i32>("N")
        .fallback(1)
        .display_fallback();
    let max_pack_size = bpaf::long("max-pack-size")
        .help("Split into multiple archives past this size, e.g. 500M")
        .argument::<String>("SIZE")
        .fallback("500M".to_string())
        .display_fallback();
    let exclude = bpaf::long("exclude")
        .help("Glob (relative to --input) of files to leave as loose, uncompressed files; repeatable")
        .argument::<String>("GLOB")
        .many();
    bpaf::construct!(PackCli { input, output, level, max_pack_size, exclude })
}

fn extract_cli() -> impl Parser<ExtractCli> {
    let content_dir = bpaf::long("content-dir")
        .help("Directory containing manifest.json + .pack files, produced by `pack`")
        .argument::<PathBuf>("DIR");
    let dest = bpaf::long("dest")
        .help("Directory to extract the original files into. Defaults to a stable, per-install \
            location under the OS temp directory — nothing is extracted next to --content-dir \
            unless this is passed explicitly.")
        .argument::<PathBuf>("DIR")
        .optional();
    let force = bpaf::long("force")
        .help("Re-extract even if the cached content hash in --dest already matches")
        .switch();
    bpaf::construct!(ExtractCli { content_dir, dest, force })
}

fn cmd() -> Cmd {
    let pack = pack_cli()
        .map(Cmd::Pack)
        .to_options()
        .descr("Pack a dist/-shaped directory into a handful of tar+zstd archives");
    let extract = extract_cli()
        .map(Cmd::Extract)
        .to_options()
        .descr("Extract archives produced by `pack` back into plain files");
    let pack_cmd = pack.command("pack");
    let extract_cmd = extract.command("extract");
    bpaf::construct!([pack_cmd, extract_cmd])
        .to_options()
        .descr("Packs/unpacks a game's dist/ so a release doesn't ship it as trivially browsable loose files")
        .run()
}

fn main() -> ExitCode {
    let result = match cmd() {
        Cmd::Pack(args) => run_pack(args),
        Cmd::Extract(args) => run_extract(args),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

fn run_pack(args: PackCli) -> Result<(), String> {
    let exclude = args
        .exclude
        .iter()
        .map(|pattern| glob::Pattern::new(pattern).map_err(|e| format!("--exclude {pattern:?}: {e}")))
        .collect::<Result<Vec<_>, _>>()?;
    let opts = pack::PackOptions {
        input: args.input,
        output: args.output,
        level: args.level,
        max_pack_size: size::parse_size(&args.max_pack_size)?,
        exclude,
    };
    pack::pack(&opts)
}

fn run_extract(args: ExtractCli) -> Result<(), String> {
    let opts = extract::ExtractOptions {
        content_dir: args.content_dir,
        dest: args.dest,
        force: args.force,
    };
    // Printed so callers (the generated launchers) that don't pass --dest
    // explicitly can capture where extraction actually landed, e.g.
    // `CACHE_DIR="$(roves-content-packer extract --content-dir "$DIR")"`.
    let dest = extract::extract(&opts)?;
    println!("{}", dest.display());
    Ok(())
}
