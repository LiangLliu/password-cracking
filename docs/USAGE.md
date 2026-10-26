# Usage Guide

**English** | [中文](USAGE.zh-CN.md)

## Quick start

The fixtures in `examples/` use the password `92eo`:

```bash
cargo build --release
./target/release/password-cracking -f examples/test.zip \
  dictionary -w wordlists/common-passwords.txt
```

The program prints the detected format, attack mode, speed and result. A
progress bar appears while the search runs on a terminal. Pipes and CI logs do
not get the bar.

## Formats

The program reads magic bytes first and falls back to the extension when the
header is inconclusive. The extension also separates unencrypted OOXML files
from plain ZIP archives.

| Format | Extensions | Encryption |
|--------|------------|------------|
| ZIP | `.zip` | ZipCrypto, AES-128/192/256 |
| PDF | `.pdf` | RC4-40, RC4-128, AES-128, AES-256 R5/R6 |
| Office | `.docx`, `.xlsx`, `.pptx`, `.docm`, `.xlsm`, `.pptm` | Agile AES-128/192/256 |

PDF owner passwords are opt-in:

```bash
password-cracking -f document.pdf dictionary -w passwords.txt --pdf-owner
```

The owner check triples the work per candidate. Office support covers
Agile encryption from Office 2010 and later. The program rejects legacy
CryptoAPI/ECMA-376 files with an error.

## Attack modes

### Dictionary

```bash
# One file
password-cracking -f document.zip dictionary -w passwords.txt

# Every .txt file in a directory, sorted
password-cracking -f document.zip dictionary -w wordlists/

# Drop duplicates (memory grows with the number of unique candidates)
password-cracking -f document.zip dictionary -w rockyou.txt --dedup
```

The wordlist is memory-mapped and read by byte offset, so file size does not
raise memory use. Lines lose surrounding whitespace. Empty lines and lines that
start with `#` are skipped. Candidates stay as bytes, so non-UTF-8 lists work.

### Brute-force

```bash
# 4 digit PIN
password-cracking -f document.pdf brute-force -c digits --min-length 4 --max-length 4

# 1 to 6 digits
password-cracking -f document.zip brute-force -c digits --min-length 1 --max-length 6

# Custom character set
password-cracking -f document.zip brute-force -c "abc123!@#" --min-length 1 --max-length 8
```

The keyspace for one length is `charset_size ^ length`. Every extra position
multiplies the work by the charset size.

### Mask

A mask sets the character set for each position.

```bash
# Two digits then two lowercase letters
password-cracking -f document.zip mask -m "?d?d?l?l"

# Literal text followed by two digits
password-cracking -f document.zip mask -m "admin?d?d"

# Password contains a literal ? (what?)
password-cracking -f document.zip mask -m "what??"
```

| Token | Characters | Count |
|-------|------------|-------|
| `?d` | Digits | 10 |
| `?l` | Lowercase letters | 26 |
| `?u` | Uppercase letters | 26 |
| `?s` | Printable special characters | 31 |
| `?a` | Printable ASCII | 93 |
| `??` | Literal `?` | 1 |

Other characters are literals. A mask can hold at most 128 positions.

### Hybrid

Hybrid mode applies mutation rules to each dictionary word. It includes the
original word.

```bash
# Capitalize and apply l33t substitutions
password-cracking -f document.zip hybrid -w passwords.txt --capitalize --l33t

# Append 0 to 99 to each word
password-cracking -f document.zip hybrid -w passwords.txt --append-digits 99

# Several rules at once
password-cracking -f document.zip hybrid -w passwords.txt \
  --capitalize --upper --lower --l33t --reverse --duplicate --append-digits 99
```

With `--append-digits N`, a word produces the original, one variant per rule
that changes it, and `N + 1` number variants. Without append, drop the number
variants. Rules apply to the original word, not to each other. With
`--capitalize --l33t`, `password` yields `password`, `Password` and `p@$$w0rd`.

## Options

| Option | Short | Description |
|--------|-------|-------------|
| `--file <PATH>` | `-f` | Target file |
| `--threads <N>` | `-t` | Worker threads (default: all logical cores) |
| `--quiet` | `-q` | Print only the result |
| `--checkpoint <PATH>` | | Save progress to a file |
| `--resume` | | Continue from the checkpoint |
| `--checkpoint-interval <SECS>` | | Seconds between checkpoint writes (default: 5) |
| `--pdf-owner` | | Also try PDF owner passwords |
| `--help` | `-h` | Show help |
| `--version` | `-V` | Show the version |

Global options work before or after the subcommand:

```bash
password-cracking -f document.zip -t 4 dictionary -w passwords.txt
password-cracking dictionary -w passwords.txt -f document.zip -t 4
```

## Checkpoints

Long dictionary, brute-force, mask and hybrid runs can stop and resume.

```bash
# Start and write a checkpoint every 5 seconds
password-cracking -f document.pdf brute-force -c alnum --min-length 1 --max-length 8 \
  --checkpoint progress.ck

# Continue after Ctrl-C or a reboot
password-cracking -f document.pdf brute-force -c alnum --min-length 1 --max-length 8 \
  --checkpoint progress.ck --resume
```

The checkpoint stores the verified frontier: the start of the earliest batch
that has not finished. A resume starts at that batch. A hard kill loses at most
one checkpoint interval plus the batches in flight.

The file holds three lines:

```text
brute-force
5:0,0,0,0:1
/absolute/path/to/document.pdf
```

The lines are the source name, the source position and the target path. Resume
refuses to start when the source or target differs.

The program deletes the checkpoint when the search ends, whether it finds a
password or exhausts the keyspace. Dictionary resume assumes the wordlist did
not change. After a resume, `--dedup` does not rebuild its duplicate set, so the
program may check some candidates twice.

## Character sets

| Name | Characters | Count |
|------|------------|-------|
| `digits` | `0123456789` | 10 |
| `lower` | `abcdefghijklmnopqrstuvwxyz` | 26 |
| `upper` | `ABCDEFGHIJKLMNOPQRSTUVWXYZ` | 26 |
| `special` | Punctuation and symbols | 31 |
| `alnum` | Digits, lower and upper | 62 |
| `all` | Digits, lower, upper and special | 93 |

Any other value is a custom character set. Repeated characters only repeat
work.

## Mutation rules

| Rule | Flag | `password` becomes |
|------|------|--------------------|
| Capitalize | `--capitalize` | `Password` |
| Uppercase | `--upper` | `PASSWORD` |
| Lowercase | `--lower` | `password` |
| L33t | `--l33t` | `p@$$w0rd` |
| Reverse | `--reverse` | `drowssap` |
| Duplicate | `--duplicate` | `passwordpassword` |
| Append digits | `--append-digits N` | `password0` to `passwordN` |

L33t substitutions: `a` to `@`, `e` to `3`, `i` to `1`, `o` to `0`, `s` to `$`.

## Scripting

Quiet mode prints the password to stdout and keeps diagnostics on stderr.

```bash
PASSWORD=$(password-cracking -q -f document.zip dictionary -w passwords.txt)
```

| Exit code | Meaning |
|-----------|---------|
| `0` | Password found |
| `1` | Search finished, no password |
| `2` | Invalid input or runtime error |

With `-q` and no match, stdout stays empty and stderr gets `not found`.

## Shell integration

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

The `.deb`, `.rpm`, `.apk` and Homebrew packages install completions and the
man page.

## Examples

A ZIP password made of four lowercase letters:

```bash
password-cracking -f backup.zip brute-force -c lower --min-length 4 --max-length 4
```

A ZIP password that starts with `pass` and ends with two lowercase letters:

```bash
password-cracking -f backup.zip mask -m "pass?l?l"
```

A PDF that only accepts the owner password:

```bash
password-cracking -f statement.pdf dictionary -w passwords.txt --pdf-owner
```

An Office file, where dictionary and hybrid attacks are the practical options:

```bash
password-cracking -f report.docx dictionary -w wordlists/common-passwords.txt
password-cracking -f report.docx hybrid -w wordlists/common-passwords.txt \
  --capitalize --append-digits 99
```

Batch mode uses the exit code:

```bash
#!/usr/bin/env bash
set -u

for file in ~/documents/*.{zip,pdf,docx,xlsx,pptx}; do
    [ -f "$file" ] || continue
    printf 'Cracking %s ... ' "$file"
    password-cracking -q -f "$file" dictionary -w wordlists/ || echo "not found"
done
```

## Tuning

1. Try a dictionary first.
2. Use the smallest charset that could work.
3. Bound the length. Each position multiplies the keyspace.
4. Use a mask when you know the shape.
5. Leave the thread count at the default unless the machine is shared.
6. Skip `--dedup` on large lists unless the list has many duplicates.

## Troubleshooting

| Symptom | Cause or fix |
|---------|--------------|
| Exit code 1 | The search exhausted the keyspace. Check the wordlist, charset and length range. |
| Wordlist lines are skipped | The program treats lines that start with `#` as comments and skips empty lines. |
| Owner-only PDF is not found | Add `--pdf-owner`. |
| `Unsupported Office encryption version` | The file uses legacy CryptoAPI/ECMA-376 encryption. |
| `Checkpoint belongs to target ...` | The checkpoint was made for another file. Use a new path. |
| Resume misses a password | The wordlist or attack parameters changed between runs. |
| Office is slow | Every candidate costs about 100,000 hash iterations. Use a dictionary or hybrid attack. |
| No progress bar | stdout is not a terminal. |
