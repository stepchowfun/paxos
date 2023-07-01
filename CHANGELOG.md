# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [1.0.7] - 2023-07-01

### Changed
- Even if there's no value to propose, the proposer is run periodically to learn if a value was chosen and let the other nodes know about it.

## [1.0.6] - 2023-07-01

### Changed
- The output is now less verbose.

## [1.0.5] - 2023-06-29

### Changed
- For efficiency, proposers now use the round from the minimum proposal number returned by the `accept` endpoint to update their own round counter.

## [1.0.4] - 2023-06-29

### Added
- Paxos supports a new platform: Windows on AArch64.

## [1.0.3] - 2023-06-02

### Added
- Paxos supports a new platform: musl Linux on AArch64.

## [1.0.2] - 2023-05-23

### Added
- Paxos supports a new platform: GNU Linux on AArch64.

## [1.0.1] - 2023-05-13

### Added
- Paxos supports a new platform: macOS on Apple silicon.

## [1.0.0] - 2021-06-20

### Added
- Initial release.
