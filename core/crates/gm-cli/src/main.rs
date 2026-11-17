fn main() {
    std::process::exit(gm_cli::run(std::env::args().skip(1).collect()));
}
