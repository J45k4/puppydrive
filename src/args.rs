use clap::Parser;
use clap::Subcommand;


#[derive(Debug, Parser)]
#[clap(name = "puppydrive")]
pub struct Args {
    #[clap(long)]
    pub peer: Vec<String>,
    #[clap(long)]
    pub bind: Vec<String>,
}
