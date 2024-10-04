# Rodio Development Guide

## Quick Start

1. Install [Rust](https://www.rust-lang.org/tools/install)
2. Clone the repository: `git clone https://github.com/RustAudio/rodio.git`
3. Navigate to the project: `cd rodio`
4. Build the project: `cargo build`

## Project Structure

src/:
- `source/`: Audio source implementations
- `decoder/`: Audio format decoders
- `sink/`: Audio playback and mixing
- `dynamic_mixer/`: Real-time audio mixing
- `spatial/`: Spatial audio capabilities

## Coding Guidelines

- Follow [Rust API Guidelines](https://rust-lang.github.io/api-guidelines/)
- Use `rustfmt` for formatting
- Implement `Source` trait for new audio sources
- Use `Sink` for playback management

## Common Tasks

### Adding a New Audio Source or Effect

1. Create a new file in `src/source/`
2. Implement the `Source` trait to define how audio samples are generated or modified
3. Consider implementing sources like oscillators, noise generators, or effects like amplification, filtering, or distortion
4. Optimize for performance, especially for real-time processing in effects
5. Add unit tests/benchmarks where applicable to ensure proper functionality and performance

### Implementing a New Decoder

1. Add new module in `src/decoder/`
2. Implement necessary traits (e.g., `Decoder`) to handle specific audio formats
3. Focus on efficiently parsing audio file headers and decoding compressed audio data
4. Update `src/decoder/mod.rs` to integrate the new decoder

## Testing

- Write unit tests for each new function (if applicable)
- Add integration tests for end-to-end scenarios
- Run tests: `cargo test`

## Documentation

- Add inline documentation to all public items
- Generate docs: `cargo doc --open`
- Contribute examples to `examples/`

## Contribution Workflow

1. Fork the repository on GitHub
2. Clone your fork locally: `git clone https://github.com/YOUR_USERNAME/rodio.git`
3. Create a feature branch: `git checkout -b feature/your-feature-name`
4. Make changes and add tests where applicable (Dont be afraid to ask for help)
5. Ensure code quality:
   - Run `cargo fmt` to format your code
   - Run `cargo clippy` to check for common mistakes
   - Run `cargo test` to ensure all tests pass (Not strictly necessary)
   - Run `cargo bench` to run benchmarks (Not strictly necessary)
6. Commit your changes following these guidelines: (`git commit`)
  - Write clear, concise commit messages
  - Limit the first line to 50 characters
  - Provide a detailed description after a blank line, if necessary
  - Reference relevant issue numbers (e.g., "Fixes #123")
  - Separate logical changes into multiple commits
  - Avoid commits with unrelated changes
  Example:
  ```
  Add spatial audio support for stereo sources

  - Implement SpatialSource struct
  - Add panning and distance attenuation
  - Update documentation for spatial audio usage

  Fixes #456
  ```
7. Push your changes to your fork: `git push origin feature/your-feature-name`
8. Create a pull request on GitHub

## Getting Help / Got a question?

- Open an issue on GitHub
- Join the Rust Audio Discord
- Ask questions in your pull request
- Open an issue for guidance/questions

## Useful Commands

- Format: `cargo fmt` - Automatically formats the code according to Rust style guidelines
- Lint: `cargo clippy` - Runs the Clippy linter to catch common mistakes and improve code quality
- Benchmark: `cargo bench` - Executes performance benchmarks for the project

For more detailed information, refer to the full documentation and source code.

## Disclaimer

Please note that the guidelines and practices outlined in this document
are not strict rules. They are general recommendations to promote
consistency and quality in contributions.

We understand that every situation is unique and encourage contributors
to use their best judgment. If you have any doubts or questions about
how to approach a particular task or contribution, don't hesitate to
reach out to the maintainers for guidance.
