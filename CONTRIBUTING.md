# Contributing to NoString

Thank you for your interest in contributing to NoString.

## Development Setup

### Prerequisites

- Rust 1.75+ (`rustup update stable`)
- Node.js 20+ (for Tauri frontend)
- Git

### Clone and Build

```bash
git clone https://github.com/nostring/nostring
cd nostring
cargo build
cargo test
```

### Project Structure

```
nostring/
├── crates/              # Rust libraries
│   ├── nostring-core    # Seed, crypto, encryption
│   ├── nostring-inherit # Policies, miniscript
│   ├── nostring-shamir  # SLIP-39, Codex32
│   ├── nostring-electrum# Bitcoin network
│   ├── nostring-notify  # Notifications
│   └── nostring-watch   # UTXO monitoring
├── tauri-app/           # Desktop app
└── docs/                # Documentation
```

---

## Code Style

### Rust

- Run `cargo fmt` before committing
- Run `cargo clippy` and fix warnings
- Follow existing patterns in the codebase
- Document public APIs with `///` comments
- Write tests for new functionality

### Tests

Every feature should have tests:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_feature_works() {
        // Arrange
        let input = ...;
        
        // Act
        let result = feature(input);
        
        // Assert
        assert_eq!(result, expected);
    }
}
```

For network-dependent tests, use `#[ignore]`:

```rust
#[test]
#[ignore = "requires network access"]
fn test_network_feature() {
    // ...
}
```

### Commits

- Use descriptive commit messages
- Reference issues when applicable
- Keep commits focused (one feature/fix per commit)

Example:
```
Add Codex32 share generation

- Implement BIP-93 BCH checksum
- Add generate_shares() and combine_shares()
- Pass all BIP-93 test vectors
- 12 new tests

Closes #42
```

---

## Pull Requests

1. Fork the repository
2. Create a feature branch (`git checkout -b feature/my-feature`)
3. Make your changes
4. Run tests (`cargo test && cargo test -- --ignored`)
5. Run lints (`cargo fmt && cargo clippy`)
6. Push to your fork
7. Open a pull request

### PR Checklist

- [ ] Tests pass (`cargo test`)
- [ ] Code is formatted (`cargo fmt`)
- [ ] No clippy warnings (`cargo clippy`)
- [ ] Documentation updated if needed
- [ ] Commit messages are descriptive

---

## Security

If you discover a security vulnerability, please **do not** open a public issue.

Instead, email security@nostring.dev with:
- Description of the vulnerability
- Steps to reproduce
- Potential impact

We will respond within 48 hours.

---

## Architecture Decisions

Major decisions are documented in `docs/`. Before making significant changes:

1. Check existing documentation
2. Open an issue to discuss
3. Get consensus before implementing

---

## Questions?

- Open an issue for bugs or feature requests
- Check existing issues before creating new ones
- Be respectful and constructive

---

*Thank you for helping make Bitcoin inheritance sovereign.*
