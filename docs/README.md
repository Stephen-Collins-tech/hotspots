# Faultline Documentation

This directory contains comprehensive documentation for the Faultline MVP project.

## Documentation Overview

### For Users

- **[README.md](../README.md)** - Quickstart guide and user-facing documentation
- **[USAGE.md](./USAGE.md)** - Comprehensive usage guide with examples
- **[lrs-spec.md](./lrs-spec.md)** - Detailed Local Risk Score specification with formulas and examples
- **[suppression.md](./suppression.md)** - Suppression comments guide (handling false positives)
- **[language-support.md](./language-support.md)** - Supported languages and syntax
- **[limitations.md](./limitations.md)** - Known limitations and future work

### For Developers

- **[architecture.md](./architecture.md)** - System architecture and design overview
- **[implementation-summary.md](./implementation-summary.md)** - Complete summary of implemented features across all phases
- **[design-decisions.md](./design-decisions.md)** - Key design decisions and rationale
- **[invariants.md](./invariants.md)** - Global invariants that must be maintained

### For Maintainers

- **[TASKS.md](TASKS.md)** - Original MVP task specification (all phases complete)
- **[design-decisions.md](./design-decisions.md)** - Design rationale for future changes
- **[limitations.md](./limitations.md)** - Known limitations to address

## Quick Navigation

### Understanding the System

1. Start with [architecture.md](./architecture.md) for system overview
2. Read [design-decisions.md](./design-decisions.md) for key choices
3. Review [invariants.md](./invariants.md) for constraints

### Using Faultline

1. Read [README.md](../README.md) for quickstart
2. See [USAGE.md](./USAGE.md) for complete feature guide
3. Consult [lrs-spec.md](./lrs-spec.md) for score interpretation
4. Use [suppression.md](./suppression.md) for handling false positives
5. Check [language-support.md](./language-support.md) for feature support

### Developing Features

1. Review [implementation-summary.md](./implementation-summary.md) for what's done
2. Understand [design-decisions.md](./design-decisions.md) for rationale
3. Maintain [invariants.md](./invariants.md) compliance

## Document Status

All documentation is complete and up-to-date (as of 2026-02-03):

- ✅ Architecture documented
- ✅ Implementation summary complete
- ✅ Design decisions captured
- ✅ Invariants documented
- ✅ LRS specification complete
- ✅ Language support documented (TypeScript, JavaScript, JSX, TSX)
- ✅ Usage guide with examples (snapshot, delta, policies, config)
- ✅ Suppression comments documented
- ✅ Policy engine documented (7 built-in policies)
- ✅ Configuration files documented
- ✅ HTML reports documented
- ✅ Limitations documented

## Related Files

- `../README.md` - Main project README
- `TASKS.md` - Original MVP task specification
- `../tests/fixtures/` - Test TypeScript files
- `../tests/golden/` - Expected JSON outputs
