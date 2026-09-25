//! Binary entrypoint for the `cryptovol` command-line tool.

use clap::{Parser, Subcommand};
use cryptovol_cli::KdfArg;
use std::process::ExitCode;

#[derive(Debug, Parser)]
#[command(name = "cryptovol")]
#[command(about = "Read-only explorer for encrypted volume containers")]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Debug, Subcommand)]
enum Commands {
    Info {
        container: String,
    },
    TestOpen {
        container: String,
        #[arg(
            long,
            help = "Personal Iterations Multiplier (PIM); omit for VeraCrypt default"
        )]
        pim: Option<u32>,
        #[arg(long, value_enum, help = "KDF/hash hint; omit to autoprobe")]
        kdf: Option<KdfArg>,
    },
    ProbeFs {
        container: String,
        #[arg(
            long,
            help = "Personal Iterations Multiplier (PIM); omit for VeraCrypt default"
        )]
        pim: Option<u32>,
        #[arg(long, value_enum, help = "KDF/hash hint; omit to autoprobe")]
        kdf: Option<KdfArg>,
    },
    Ls {
        container: String,
        path: String,
        #[arg(long, help = "Show long listing with metadata")]
        long: bool,
        #[arg(
            long,
            help = "Personal Iterations Multiplier (PIM); omit for VeraCrypt default"
        )]
        pim: Option<u32>,
        #[arg(long, value_enum, help = "KDF/hash hint; omit to autoprobe")]
        kdf: Option<KdfArg>,
    },
    Extract {
        container: String,
        source_path: String,
        destination_path: String,
        #[arg(long, help = "Overwrite the destination file if it exists")]
        overwrite: bool,
        #[arg(long, help = "Create missing parent directories of the destination")]
        parents: bool,
        #[arg(
            long,
            help = "Personal Iterations Multiplier (PIM); omit for VeraCrypt default"
        )]
        pim: Option<u32>,
        #[arg(long, value_enum, help = "KDF/hash hint; omit to autoprobe")]
        kdf: Option<KdfArg>,
    },
}

fn main() -> ExitCode {
    let cli = Cli::parse();

    match cli.command {
        Some(Commands::Info { container }) => cryptovol_cli::commands::info(&container),
        Some(Commands::TestOpen {
            container,
            pim,
            kdf,
        }) => cryptovol_cli::commands::test_open(&container, pim, kdf.map(Into::into)),
        Some(Commands::ProbeFs {
            container,
            pim,
            kdf,
        }) => cryptovol_cli::commands::probe_fs(&container, pim, kdf.map(Into::into)),
        Some(Commands::Ls {
            container,
            path,
            long,
            pim,
            kdf,
        }) => cryptovol_cli::commands::ls(&container, &path, long, pim, kdf.map(Into::into)),
        Some(Commands::Extract {
            container,
            source_path,
            destination_path,
            overwrite,
            parents,
            pim,
            kdf,
        }) => cryptovol_cli::commands::extract(
            &container,
            &source_path,
            &destination_path,
            overwrite,
            parents,
            pim,
            kdf.map(Into::into),
        ),
        None => {
            println!("{}", cryptovol_core::product_name());
            ExitCode::SUCCESS
        }
    }
}
