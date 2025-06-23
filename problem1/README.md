# Problem 1: Zero Padding Numbers in Strings

## Overview

This solution implements a function that takes a string and an integer X, returning a string with all whole numbers zero-padded to X characters.

## Running the Solution

```bash
# Build the project
cargo build

# Run tests
cargo test

# Run example program
cargo run

# Run with release optimizations
cargo build --release
cargo run --release
```

## Examples

- `pad_numbers("James Bond 7", 3)` → `"James Bond 007"`
- `pad_numbers("PI=3.14", 2)` → `"PI=03.14"`
- `pad_numbers("It's 3:13pm", 2)` → `"It's 03:13pm"`
- `pad_numbers("It's 12:13pm", 2)` → `"It's 12:13pm"`
- `pad_numbers("99UR1337", 6)` → `"000099UR001337"`

## Performance Analysis

### Time Complexity
- **O(n)** where n is the length of the input string
- The regex engine performs a single pass through the string to find all digit sequences
- Each replacement operation is O(1) for padding calculation

### Space Complexity
- **O(n)** where n is the length of the output string
- In worst case, output string can be larger than input (when padding increases digit lengths)
- Additional O(m) space for regex matches, where m is the number of digit sequences found

### Implementation Details

The solution uses the `regex` crate (v1.10) for pattern matching:
- Pattern `\d+` matches one or more consecutive digits
- The `replace_all` method efficiently handles all replacements in a single pass
- Format string `{:0>width$}` provides zero-padding functionality

### Design Decisions

1. **Regex over manual parsing**: Chosen for clarity and correctness. While manual character iteration might be slightly faster, regex provides a robust, well-tested solution that handles Unicode correctly.

2. **Time optimization**: The single-pass regex approach is preferred over multiple string iterations, minimizing time complexity.

3. **Memory trade-off**: We accept the memory overhead of creating a new string rather than modifying in-place (which Rust strings don't allow anyway) for simplicity and safety.

## AI Usage Disclosure

- **Model**: Claude (Anthropic)
- **Usage**: Asked about edge cases to ensure comprehensive test coverage
- **Validation**: All test cases were manually reviewed and verified against the problem requirements