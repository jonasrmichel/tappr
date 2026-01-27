# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

**tappr** - "Ride the beat of the world's airwaves"

A Rust project (early stage development).

## Build Commands

Once Cargo.toml is created:

```bash
cargo build              # Build the project
cargo build --release    # Build with optimizations
cargo run                # Run the project
cargo test               # Run all tests
cargo test <test_name>   # Run a specific test
cargo clippy             # Run linter
cargo fmt                # Format code
```

## Project Status

This repository is in its initial setup phase with no source code yet. The next steps would typically be:

1. Create `Cargo.toml` with project dependencies
2. Create `src/main.rs` or `src/lib.rs` entry point
3. Implement core functionality
