use anyhow::Result;

fn main() -> Result<()> {
    ncy_server::run(&std::env::args().skip(1).collect::<Vec<_>>(), None)
}
