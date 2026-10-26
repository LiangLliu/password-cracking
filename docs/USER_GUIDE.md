# User Guide

**English** | [中文](USER_GUIDE.zh-CN.md)

Complete usage guide for `password-cracking`. The command-line reference is
also available through `password-cracking --help` and `password-cracking man`.

## Contents

1. [Installation](#installation)
2. [Quick start](#quick-start)
3. [How it works](#how-it-works)
4. [File format support](#file-format-support)
5. [Attack modes](#attack-modes)
6. [Global options](#global-options)
7. [Checkpoint and resume](#checkpoint-and-resume)
8. [Character sets](#character-sets)
9. [Mutation rules](#mutation-rules)
10. [Wordlists](#wordlists)
11. [Quiet mode and exit codes](#quiet-mode-and-exit-codes)
12. [Shell completions and man page](#shell-completions-and-man-page)
13. [Performance tips](#performance-tips)
14. [Examples](#examples)
15. [Troubleshooting](#troubleshooting)
16. [Limitations](#limitations)

---

## Installation

### Package managers

```bash
# Homebrew (macOS / Linux)
brew install LiangLliu/tap/password-cracking

# Debian / Ubuntu / Raspberry Pi OS (signed APT repository)
sudo install -m 0755 -d /etc/apt/keyrings
curl -fsSL https://lianglliu.github.io/password-cracking/key.gpg \
  | sudo tee /etc/apt/keyrings/password-cracking.gpg > /dev/null
echo "deb [signed-by=/etc/apt/keyrings/password-cracking.gpg] https://lianglliu.github.io/password-cracking stable main" \
  | sudo tee /etc/apt/sources.list.d/password-cracking.list
sudo apt update && sudo apt install password-cracking

# Docker
docker run --rm -v "$PWD:/data" ghcr.io/lianglliu/password-cracking --version
```

Arch Linux (`yay -S password-cracking-bin`) and `cargo install
password-cracking` become available after the AUR package and crates.io token
are configured. See [Distribution Channels](DISTRIBUTION.md) for the current
status.

### Static binary

```bash
curl -fsSL https://raw.githubusercontent.com/LiangLliu/password-cracking/master/scripts/install.sh | sh
```

The installer verifies the release SHA-256, prefers the static musl build on
Linux, and installs to `/usr/local` or `~/.local`.

### From source

```bash
git clone https://github.com/LiangLliu/password-cracking
cd password-cracking
cargo build --release
# binary: target/release/password-cracking
```

---

## Quick start

The fixtures in `examples/` use the password `92eo`.

```bash
password-cracking -f examples/test.zip dictionary -w wordlists/common-passwords.txt
```

```text
Password Cracker
================
Target: examples/test.zip

Format:  ZIP (ZipCrypto)
Attack:  dictionary
Threads: 10
Keyspace: 33

Result
======
Duration: 0s
Attempts: 33
Speed:    3518 passwords/sec

Password found: 92eo
```

On a terminal, a progress bar is shown while the search runs. When stdout is
not a terminal (pipes, CI logs), the bar is suppressed.

---

## How it works

`password-cracking` has two independent halves:

- **`PasswordSource`** produces candidates: dictionary lines, brute-force
  combinations, mask combinations, or dictionary words with mutations.
- **`PasswordVerifier`** checks a candidate against the target file.

A generator thread fills bounded batches and sends them through a FIFO channel
to a fixed pool of worker threads. The engine uses a two-phase check where a
cheap filter exists:

1. `quick_check()` rejects most wrong passwords cheaply. Only ZIP ZipCrypto has
   a meaningful filter (the 12-byte encryption header).
2. `verify()` performs the full password check.

For PDF and Office, verification itself is the expensive step, so
`quick_check()` deliberately returns `true` and the password is verified
exactly once.

Batch sizes adapt between 32 and 4096 candidates, targeting roughly 250 ms per
batch. This keeps checkpoints timely for slow formats (Office) while amortizing
synchronization for fast formats (ZIP).

---

## File format support

The format is detected from magic bytes; the extension is only used to
distinguish OOXML (`.docx`/`.xlsx`/`.pptx`) from plain ZIP archives.

| Format | Extensions | Encryption | Typical speed (10-core M2 Pro) |
|--------|------------|------------|-------------------------------|
| ZIP | `.zip` | ZipCrypto, AES-128/192/256 | 11M+ passwords/sec (ZipCrypto) |
| PDF | `.pdf` | RC4-40, RC4-128, AES-128, AES-256 R5/R6 | ~530K passwords/sec (RC4-128) |
| Office | `.docx`, `.xlsx`, `.pptx` | Agile AES-128/192/256 | 500–800 passwords/sec |

PDF owner passwords are opt-in (`--pdf-owner`) because the extra check roughly
triples the per-candidate cost.

---

## Attack modes

### Dictionary attack

Tries every line from a wordlist. Single files and directories are supported.

```bash
# Single file
password-cracking -f document.zip dictionary -w passwords.txt

# Directory: all .txt files, sorted, recursively
password-cracking -f document.zip dictionary -w ./wordlists/

# Remove duplicates (memory grows with the number of unique candidates)
password-cracking -f document.zip dictionary -w /data/rockyou.txt --dedup
```

Wordlists are memory-mapped and read by byte offset. Memory stays flat no
matter how large the file is. Lines are trimmed of ASCII whitespace; empty
lines and lines starting with `#` are skipped. Non-UTF-8 wordlists (for example
GBK) work because candidates are handled as bytes.

### Brute-force attack

Generates every combination of a character set for lengths
`min-length..=max-length`.

```bash
# 4-digit PIN
password-cracking -f document.pdf brute-force -c digits --min-length 4 --max-length 4

# 1-6 digits
password-cracking -f document.zip brute-force -c digits --min-length 1 --max-length 6

# Lowercase, 1-4 characters
password-cracking -f document.zip brute-force -c lower --min-length 1 --max-length 4

# Custom character set
password-cracking -f document.zip brute-force -c "abc123!@#" --min-length 1 --max-length 8
```

Exact-length keyspace (`C^L`):

| Charset | Length 4 | Length 6 | Length 8 |
|---------|----------|----------|----------|
| `digits` (10) | 10,000 | 1,000,000 | 100,000,000 |
| `lower` (26) | 456,976 | 308,915,776 | 208,827,064,576 |
| `alnum` (62) | 14,776,336 | 56,800,235,584 | 218,340,105,584,896 |
| `all` (93) | 74,805,201 | 646,990,183,449 | 5,595,818,096,650,401 |

### Mask attack

A mask assigns a character set to each position. It is the best choice when
you know the password shape.

```bash
# 92eo = digit digit lower lower
password-cracking -f document.zip mask -m "?d?d?l?l"

# Capital + 8 lower + 2 digits
password-cracking -f document.docx mask -m "?u?l?l?l?l?l?l?l?d?d"

# Literal text: "admin" followed by two digits
password-cracking -f document.zip mask -m "admin?d?d"

# A literal question mark
password-cracking -f document.zip mask -m "what??"
```

| Placeholder | Character set | Size |
|-------------|---------------|------|
| `?d` | digits | 10 |
| `?l` | lowercase | 26 |
| `?u` | uppercase | 26 |
| `?s` | printable special characters | 31 |
| `?a` | all printable ASCII | 93 |
| `??` | literal `?` | 1 |

Any other character is a literal. Multi-byte UTF-8 literals are preserved
byte-for-byte. A mask may have at most 128 positions.

### Hybrid attack

Applies mutation rules to every dictionary word. The original word is always
included.

```bash
# Capitalize and apply l33t substitutions
password-cracking -f document.zip hybrid -w wordlists/common-passwords.txt --capitalize --l33t

# Append 0-99 to each word
password-cracking -f document.zip hybrid -w wordlists/common-passwords.txt --append-digits 99

# All rules at once
password-cracking -f document.zip hybrid -w wordlists/common-passwords.txt \
  --capitalize --upper --lower --l33t --reverse --duplicate --append-digits 99
```

Keyspace per word: `1 (original) + number of active rules + (N + 1)` where `N`
is the `--append-digits` value. With 33 words and
`--capitalize --append-digits 99`, the total is `33 × (1 + 1 + 100) = 3,366`.

See [Mutation rules](#mutation-rules).

---

## Global options

| Option | Short | Description |
|--------|-------|-------------|
| `--file <PATH>` | `-f` | Target file (required for attack modes) |
| `--threads <N>` | `-t` | Worker threads (default: all logical cores; minimum 1) |
| `--quiet` | `-q` | Print only the result |
| `--checkpoint <PATH>` | | Save progress periodically to PATH |
| `--resume` | | Resume from the checkpoint file |
| `--checkpoint-interval <SECS>` | | Seconds between checkpoint writes (default 5) |
| `--pdf-owner` | | Also try PDF owner passwords (~3× slower) |
| `--help` | `-h` | Show help |
| `--version` | `-V` | Show version |

Global options may appear before or after the subcommand:

```bash
password-cracking -f document.zip -t 4 dictionary -w passwords.txt
password-cracking dictionary -w passwords.txt -f document.zip -t 4
```

---

## Checkpoint and resume

Long dictionary, brute-force, mask and hybrid runs can be interrupted and
resumed without re-checking verified candidates.

```bash
# Start; a checkpoint is written every 5 seconds
password-cracking -f document.pdf brute-force -c alnum --min-length 1 --max-length 8 \
  --checkpoint progress.ck

# Continue after Ctrl-C, a crash, or a reboot
password-cracking -f document.pdf brute-force -c alnum --min-length 1 --max-length 8 \
  --checkpoint progress.ck --resume

# Custom interval
password-cracking ... --checkpoint progress.ck --checkpoint-interval 10
```

The checkpoint stores the **verified frontier**: the start position of the
earliest batch that has not been fully verified. A resume therefore never skips
candidates that were only generated or partially checked. A hard kill loses at
most one checkpoint interval plus the in-flight batches.

The file contains three lines:

```text
brute-force
5:0,0,0,0:1
/absolute/path/to/document.pdf
```

- the source name (`dictionary`, `brute-force`, `mask`, `hybrid`)
- the source-specific position
- the canonical target path

Resume refuses to run if the source or target differs from the checkpoint.

Notes:

- The checkpoint is removed when the search finishes (found or exhausted).
- For dictionary attacks, the wordlist must be unchanged between runs. Files in
  a directory are processed in sorted order for stability.
- With `--dedup`, the duplicate set is not reconstructed after a resume.
  Duplicates may be retried; this is harmless, only extra work.

---

## Character sets

| Name | Characters | Count |
|------|------------|-------|
| `digits` | `0123456789` | 10 |
| `lower` | `abcdefghijklmnopqrstuvwxyz` | 26 |
| `upper` | `ABCDEFGHIJKLMNOPQRSTUVWXYZ` | 26 |
| `special` | `` !@#$%^&*()-_=+[]{}\|;:'",.<>/?`~ `` | 31 |
| `alnum` | digits + lower + upper | 62 |
| `all` | digits + lower + upper + special | 93 |

Any other string is treated as a custom character set. Duplicate characters
should be avoided; they only repeat work.

---

## Mutation rules

| Rule | Flag | Input `password` → output |
|------|------|---------------------------|
| Capitalize | `--capitalize` | `Password` |
| Uppercase | `--upper` | `PASSWORD` |
| Lowercase | `--lower` | `password` |
| L33t | `--l33t` | `p@$$w0rd` |
| Reverse | `--reverse` | `drowssap` |
| Duplicate | `--duplicate` | `passwordpassword` |
| Append digits | `--append-digits N` | `password0` … `passwordN` |

L33t substitutions: `a→@`, `e→3`, `i→1`, `o→0`, `s→$`.

Rule variants are generated independently from the original word, not
cumulatively. For example, `--capitalize --l33t` produces `password`,
`Password`, and `p@$$w0rd` — not `P@$$w0rd`.

---

## Wordlists

Bundled wordlists:

| File | Contents | Lines |
|------|----------|-------|
| `common-passwords.txt` | Common passwords | 33 |
| `chinese-common.txt` | Common Chinese passwords | 40 |
| `pins-4digit.txt` | 0000–9999 | 10,000 |
| `pins-6digit.txt` | 000000–999999 | 1,000,000 |

Directory mode loads every `.txt` file recursively, in sorted order:

```bash
password-cracking -f document.zip dictionary -w wordlists/
```

Create your own wordlist as a UTF-8 or raw-byte text file with one candidate
per line. Lines starting with `#` are comments.

---

## Quiet mode and exit codes

`-q` prints only the password on stdout. Diagnostics go to stderr.

```bash
PASSWORD=$(password-cracking -q -f document.zip dictionary -w passwords.txt)
```

| Exit code | Meaning |
|-----------|---------|
| `0` | Password found |
| `1` | Search exhausted, password not found |
| `2` | Invalid input or runtime error (details on stderr) |

In quiet mode a missing password prints `not found` to stderr; stdout stays
empty.

---

## Shell completions and man page

```bash
# Bash
password-cracking completions bash > ~/.local/share/bash-completion/completions/password-cracking

# Zsh
password-cracking completions zsh > "${fpath[1]}/_password-cracking"

# Fish
password-cracking completions fish > ~/.config/fish/completions/password-cracking.fish

# Man page
password-cracking man > /tmp/password-cracking.1
man /tmp/password-cracking.1
```

Package builds (`deb`/`rpm`/`apk`, Homebrew) install these automatically.

---

## Performance tips

1. **Start with a dictionary.** It is orders of magnitude cheaper than
   brute-force.
2. **Use the smallest charset that could work.** Digits are 6.2× smaller than
   alnum per position.
3. **Bound the length.** Each extra position multiplies the keyspace by the
   charset size.
4. **Use a mask when you know the shape.** `?d?d?l?l` is 67,600 candidates,
   while `?a?a?a?a` is 74.8 million.
5. **Let it use all cores.** The default is all logical CPUs; limit with `-t`
   only if the machine is shared.
6. **For Office, use dictionary/hybrid first.** Agile encryption costs ~100K
   SHA-512 iterations per candidate.

Approximate time for an exact-length ZIP ZipCrypto search at 11M/sec:

| Charset | Length 4 | Length 6 | Length 8 |
|---------|----------|----------|----------|
| digits | instant | instant | 9 sec |
| lower | instant | 28 sec | 5.3 hours |
| alnum | 1.3 sec | 1.4 hours | 230 days |
| all | 7 sec | 16 hours | 16 years |

See the [Performance Guide](PERFORMANCE.md) for PDF and Office tables and for
the benchmark methodology.

---

## Examples

### A forgotten ZIP password

You remember it was four lowercase letters:

```bash
password-cracking -f backup.zip brute-force -c lower --min-length 4 --max-length 4
```

You remember it started with `pass` and ended with two lowercase letters:

```bash
password-cracking -f backup.zip mask -m "pass?l?l"
```

### A PDF protected by a 4–6 digit PIN

```bash
password-cracking -f statement.pdf brute-force -c digits --min-length 4 --max-length 6
```

### A PDF that only accepts the owner password

```bash
password-cracking -f document.pdf dictionary -w passwords.txt --pdf-owner
```

`--pdf-owner` enables the owner check for RC4-40/128, AES-128 and AES-256
(R5/R6). It is off by default because it roughly triples per-candidate cost.

### An Office document with common passwords

```bash
# Dictionary first — Office is slow
password-cracking -f report.docx dictionary -w wordlists/common-passwords.txt

# Large list: streaming keeps memory flat; --dedup removes duplicates
password-cracking -f report.docx dictionary -w /data/rockyou.txt --dedup

# Mutations: capitalize + append 0-99
password-cracking -f report.docx hybrid -w wordlists/common-passwords.txt \
  --capitalize --append-digits 99
```

### Batch processing

```bash
#!/usr/bin/env bash
set -u

for file in ~/documents/*.{zip,pdf,docx,xlsx,pptx}; do
    [ -f "$file" ] || continue
    printf 'Cracking %s ... ' "$file"
    if password-cracking -q -f "$file" dictionary -w wordlists/ 2>/dev/null; then
        :
    else
        echo "not found"
    fi
done
```

The `if` works because the CLI exits 1 when no password is found.

### Bundled fixtures

```bash
cd examples
uv run python create_test_files.py   # regenerate fixtures
cd ..

for fmt in zip pdf docx xlsx pptx; do
    printf '%-6s ' "$fmt"
    ./target/release/password-cracking -q -f "examples/test.$fmt" \
        dictionary -w wordlists/common-passwords.txt
done
```

---

## Troubleshooting

| Symptom | Cause / fix |
|---------|-------------|
| `password not found` and exit 1 | The search space was exhausted. Check the wordlist, charset and length range. |
| Everything is skipped in a wordlist | Lines starting with `#` are comments. Remove the prefix or edit the file. |
| PDF owner-only file not found | Add `--pdf-owner`. |
| Office: `Unsupported Office encryption version` | Legacy CryptoAPI/ECMA-376 encryption is not supported; only Agile (v4). |
| `Checkpoint belongs to target …` | The checkpoint was created for a different file. Use a fresh checkpoint path. |
| Resume does not find a password that should be there | The wordlist or attack parameters changed between runs. Resume does not re-scan verified prefixes. |
| Very slow Office search | Expected: ~100K SHA-512 iterations per candidate. Use dictionary/hybrid. |
| `Permission denied` reading a wordlist | Check file permissions; containers may need `--user`. |
| Progress bar missing | stdout is not a terminal. This is intentional. |

---

## Limitations

- **Office**: Agile encryption (v4) only; legacy CryptoAPI/ECMA-376 is rejected.
- **RAR5 / 7z**: not implemented.
- **PDF**: RC4/AES user and owner passwords; empty passwords and certificate
  encryption are out of scope.
- **ZIP**: ZipCrypto and AES; split/multi-volume archives are not supported.
  If entries use different passwords, no single password can satisfy the
  verifier.
- **Windows**: not supported.
- **Dictionary resume** assumes the wordlist is unchanged.
