# PantheonProbe

[![License](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)

PantheonProbe is a Rust network diagnostics CLI for measuring latency, packet
loss, jitter, DNS resolution time, and HTTP transfer throughput. It supports
single runs, repeated watch mode, an interactive terminal dashboard, and local
history for comparing results over time.

## Installation

```sh
cargo install pantheon-probe
```

## Commands

Run a measurement:

```sh
pantheon-probe run -t 1.1.1.1
```

Run a fuller bandwidth test profile:

```sh
pantheon-probe run -t 1.1.1.1 --profile full
```

Watch repeated measurements:

```sh
pantheon-probe watch -t 1.1.1.1 --interval 30
```

Launch the terminal dashboard:

```sh
pantheon-probe tui -t 1.1.1.1 --interval 30
```

Show recent saved runs:

```sh
pantheon-probe history -t 1.1.1.1 --limit 5
```

Export saved runs:

```sh
pantheon-probe export -t 1.1.1.1 --format csv
```

Compare the latest two runs for a target:

```sh
pantheon-probe compare -t 1.1.1.1
```

Compare two specific saved runs:

```sh
pantheon-probe compare --previous-id 1777210123095-1-1-1-1 --current-id 1777210223714-1-1-1-1
```

Use JSON output for automation:

```sh
pantheon-probe run -t 1.1.1.1 --json
```

Use custom throughput endpoints:

```sh
pantheon-probe run -t example.com \
  --provider custom \
  --download-url https://downloads.example.com/file.bin \
  --upload-url https://uploads.example.com/sink
```

## Storage

PantheonProbe saves probe runs under `~/.pantheon-probe/runs` by default.

To override the storage location:

```sh
PANTHEON_PROBE_HOME=/path/to/data pantheon-probe history
```

## Measurements

- `ping`: sent, received, packet loss, min, avg, median, p95, max, stddev, and
  jitter
- `dns`: resolution time and resolved addresses
- `bandwidth`: profile-driven HTTP download and upload runs with provider-aware
  sizing and aggregate stats

## Notes

- Ping measurements currently shell out to the system `ping` command.
- Throughput checks support `quick`, `standard`, and `full` profiles.
- The built-in bandwidth provider defaults to Cloudflare speed test URLs, and
  custom endpoints can override it.

## Issues

If you hit a bug or want a feature, [open an issue](https://github.com/0xTxbi/pantheon-probe/issues).
