# Performance Guide

## Quick Reference

### Speed by File Type

| File Type | Speed (passwords/sec) | Notes |
|-----------|---------------------|-------|
| ZIP (Traditional) | 40,000+ | Fastest |
| ZIP (AES) | 10,000+ | Moderate |
| PDF | 1,000-5,000 | Slow |
| Office | 100-1,500 | Very slow |

### Time Estimates

For alphanumeric passwords (62 characters):
- 4 characters: ~3 minutes
- 5 characters: ~3 hours
- 6 characters: ~2 days
- 7 characters: ~3 months
- 8 characters: ~15 years

## Optimization Tips

1. **Use all CPU cores**: `-p aggressive` (default)
2. **Start with dictionary attack**: Much faster than brute force
3. **Limit character set**: Digits only > lowercase > alphanumeric > all chars
4. **Estimate password length**: Saves exponential time

## Build Optimization

Add to `Cargo.toml` for maximum performance:
```toml
[profile.release]
opt-level = 3
lto = "thin"
codegen-units = 1
```

## Common Issues

**Memory usage too high**: Use balanced mode (`-p balanced`)

**Wrong password accepted**: Fixed - now properly validates CRC32

**Progress bar issues**: Fixed - stops at actual completion