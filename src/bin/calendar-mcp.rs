use std::io;

fn main() {
    let result = calendar_gpui::calendar::default_data_path()
        .map_err(io::Error::other)
        .and_then(|path| {
            calendar_gpui::mcp::Server::new(path).run(io::stdin().lock(), io::stdout().lock())
        });
    if let Err(error) = result {
        eprintln!("Calendar MCP: {error}");
        std::process::exit(1);
    }
}
