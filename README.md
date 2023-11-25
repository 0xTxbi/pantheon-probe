# PantheonProbe 🛰️

[![License](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)

PantheonProbe is a network diagnostic tool that provides comprehensive insights
into your network performance. It's designed to help you measure latency, packet
loss, jitter, and bandwidth with ease.

---

## Table of Contents

- [PantheonProbe 🛰️](#pantheonprobe-️)
     - [Table of Contents](#table-of-contents)
     - [1. Installation ](#1-installation-)
     - [2. Usage ](#2-usage-)
          - [Options ](#options-)
          - [Examples ](#examples-)
     - [3. Network Metrics ](#3-network-metrics-)
          - [Latency ⏱️](#latency-️)
          - [Packet Loss 📦 🚫](#packet-loss--)
          - [Jitter 🌊](#jitter-)
          - [Bandwidth 🌐](#bandwidth-)
          - [DNS Resolution Time 🕒](#dns-resolution-time-)
     - [5. Reporting Issues ](#5-reporting-issues-)
     - [6. License ](#6-license-)

---

## 1. Installation <a name="installation"></a>

To install PantheonProbe, use the Rust package manager, Cargo:

```sh
cargo install pantheon-probe
```

This command will download and compile the tool, making it ready for use.

---

## 2. Usage <a name="usage"></a>

PantheonProbe provides a range of options to customize and fine-tune network
measurements.

### Options <a name="options"></a>

- `-t, --target <HOST>`: Specifies the target host or IP address for testing.

### Examples <a name="examples"></a>

1. Measure network metrics for a specific host:

      ```sh
      pantheon-probe -t example.com
      ```

---

## 3. Network Metrics <a name="network-metrics"></a>

PantheonProbe provides the following network metrics:

### Latency ⏱️<a name="latency"></a>

Latency measures the time taken for a packet to travel from the source to the
destination and back. It is an essential metric for assessing the responsiveness
of a network connection.

### Packet Loss 📦 🚫<a name="packet-loss"></a>

Packet loss quantifies the percentage of packets that fail to reach their
destination. High packet loss can indicate network congestion or instability.

### Jitter 🌊<a name="jitter"></a>

Jitter measures the variability in packet arrival times. A low jitter value
indicates a stable network connection, while high jitter can lead to
inconsistent performance.

### Bandwidth 🌐<a name="bandwidth"></a>

Bandwidth measures the maximum data transfer rate between two points in a
network. It is crucial for determining the capacity of a network connection.

### DNS Resolution Time 🕒<a name="dns-resolution-time"></a>

DNS Resolution Time measures the time taken to resolve the target host's domain
name to an IP address. This metric is important for understanding the efficiency
of DNS resolution in your network.

---

## 5. Reporting Issues <a name="reporting-issues"></a>

If you encounter any issues or have suggestions for improvement, please
[open an issue on GitHub](https://github.com/0xTxbi/pantheon-probe/issues).

---

## 6. License <a name="license"></a>

This tool is licensed under the MIT License. See the
[LICENSE](https://github.com/0xTxbi/pantheon-probe/blob/main/LICENSE) file for
details.
