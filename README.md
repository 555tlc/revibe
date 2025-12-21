# revibe

[![Rust](https://img.shields.io/badge/rust-1.92%2B-orange)](https://www.rust-lang.org/)
[![License](https://img.shields.io/badge/license-Apache--2.0-blue)](LICENSE)

```
██████╗ ███████╗██╗   ██╗██╗██████╗ ███████╗
██╔══██╗██╔════╝██║   ██║██║██╔══██╗██╔════╝
██████╔╝█████╗  ██║   ██║██║██████╔╝█████╗  
██╔══██╗██╔══╝  ╚██╗ ██╔╝██║██╔══██╗██╔══╝  
██║  ██║███████╗ ╚████╔╝ ██║██████╔╝███████╗
╚═╝  ╚═╝╚══════╝  ╚═══╝  ╚═╝╚═════╝ ╚══════╝
```

**Mistral's open-source CLI coding assistant - Rust Edition**

Revibe is a Rust port of [Mistral Vibe](https://github.com/mistralai/mistral-vibe), providing a command-line coding assistant powered by Mistral's models. It offers a conversational interface to your codebase, allowing you to use natural language to explore, modify, and interact with your projects.

## Features

- **Interactive Chat**: A conversational AI agent that understands your requests and breaks down complex tasks.
- **Powerful Toolset**: Built-in tools for file manipulation, code searching, version control, and command execution.
  - `bash` - Execute shell commands
  - `read_file` - Read file contents
  - `write_file` - Create or overwrite files
  - `search_replace` - Make precise edits to files
  - `grep` - Search for patterns in code
  - `todo` - Track tasks and progress
- **MCP Support**: Connect to Model Context Protocol servers for extended functionality.
- **Highly Configurable**: Customize models, providers, tool permissions, and UI preferences.
- **Safety First**: Tool execution approval and pattern-based allowlists/denylists.

## Installation

### From Source

```bash
# Clone the repository
git clone https://github.com/nicksenger/revibe
cd revibe

# Build and install
cargo install --path crates/revibe-cli
```

### Prerequisites

- Rust 1.92 or later
- A Mistral API key (get one at [console.mistral.ai](https://console.mistral.ai))

## Quick Start

1. Run revibe for the first time to set up your API key:

   ```bash
   revibe --setup
   ```

2. Navigate to your project directory and start chatting:

   ```bash
   cd /path/to/your/project
   revibe
   ```

3. Ask questions and give instructions:

   ```
   > Find all TODO comments in the project
   > Refactor the main function to be more modular
   > Write tests for the user authentication module
   ```

## Usage

### Interactive Mode

```bash
# Start interactive session
revibe

# Start with an initial prompt
revibe "Explain this codebase"

# Start in auto-approve mode (skip tool confirmations)
revibe --auto-approve

# Start in plan mode (read-only tools only)
revibe --plan
```

### Programmatic Mode

```bash
# Run a single prompt and exit
revibe -p "List all Python files in src/"

# Pipe input
echo "What does this function do?" | revibe -p

# JSON output
revibe -p "Summarize the README" --output json

# Limit cost or turns
revibe -p "Refactor this file" --max-price 1.0 --max-turns 10
```

### Slash Commands

In interactive mode, use these commands:

| Command | Description |
|---------|-------------|
| `/help` | Show available commands |
| `/quit`, `/exit`, `/q` | Exit the session |
| `/clear`, `/reset` | Clear conversation history |
| `/compact` | Compress conversation to save tokens |
| `/stats` | Show session statistics |

### Shell Commands

Prefix with `!` to run shell commands directly:

```
> !git status
> !npm test
```

### File References

Use `@` to reference files in your prompts:

```
> Read @src/main.rs and explain what it does
> Update @config.toml with the new settings
```

## Configuration

Configuration is stored in `~/.revibe/config.toml`:

```toml
# Active model
active_model = "devstral-2"

# Auto-compact threshold (tokens)
auto_compact_threshold = 200000

# UI theme
textual_theme = "textual-dark"

# System prompt
system_prompt_id = "cli"

# Tool configurations
[tools.bash]
permission = "ask"
allowlist = ["ls", "cat", "git status"]

# Custom providers
[[providers]]
name = "custom"
api_base = "https://api.example.com/v1"
api_key_env_var = "CUSTOM_API_KEY"

# Custom models
[[models]]
name = "custom-model"
provider = "custom"
alias = "custom"
```

### Environment Variables

- `REVIBE_HOME` - Custom configuration directory (default: `~/.revibe`)
- `MISTRAL_API_KEY` - Mistral API key

### MCP Servers

Configure MCP servers for extended functionality:

```toml
[[mcp_servers]]
name = "fetch"
transport = "stdio"
command = "mcp-server-fetch"
args = []

[[mcp_servers]]
name = "database"
transport = "http"
url = "http://localhost:8000"
```

## Project Structure

```
revibe/
├── crates/
│   ├── revibe-cli/      # Main CLI binary
│   ├── revibe-core/     # Core agent, config, types
│   ├── revibe-llm/      # LLM backends (Mistral, OpenAI-compatible)
│   ├── revibe-tools/    # Tool abstractions and builtins
│   └── revibe-mcp/      # MCP protocol support
├── Cargo.toml           # Workspace configuration
└── README.md
```

## Development

```bash
# Run tests
cargo test

# Run with logging
RUST_LOG=debug cargo run -p revibe-cli

# Check formatting
cargo fmt --check

# Run clippy
cargo clippy --all-targets
```

## Differences from Python Version

This Rust implementation aims to be a 1:1 functional port with the following differences:

- **Performance**: Native compilation for faster startup and execution
- **Memory**: Lower memory footprint
- **Dependencies**: Uses Rust ecosystem (tokio, reqwest, clap, ratatui)
- **Distribution**: Single static binary, no runtime dependencies

## License

Copyright 2025 Devstral 2

Licensed under the Apache License, Version 2.0. See [LICENSE](LICENSE) for details.

## Related Projects

- [mistral-vibe](https://github.com/mistralai/mistral-vibe) - Original Python implementation
