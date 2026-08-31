use std::path::PathBuf;

use clap::Parser;

#[derive(Parser)]
#[command(name = "mcl-server", about = "MCL WebSocket server")]
struct Args {
    #[arg(long, help = "Path to graph TOML file")]
    graph: PathBuf,

    #[arg(long, default_value_t = 3456, help = "Port to listen on")]
    port: u16,

    #[arg(
        long,
        default_value = "crates/mcl-server/static",
        help = "Static files directory"
    )]
    static_dir: PathBuf,
}

#[tokio::main]
async fn main() {
    let args = Args::parse();
    let config = mcl_server::ServerConfig {
        port: args.port,
        graph_path: args.graph,
        static_dir: args.static_dir,
    };
    if let Err(e) = mcl_server::run_server(config).await {
        eprintln!("server error: {e}");
        std::process::exit(1);
    }
}
