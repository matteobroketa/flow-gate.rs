# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.0] - 2026-04-10

### Added
- Full Gating-ML 2.0 XML parsing and serialization (`flow-gate-xml`)
- Gate evaluation engine with Rectangle, Polygon, Ellipsoid, and Boolean gates (`flow-gate-core`)
- All standard transforms: Logicle, FASinh, Logarithmic, Linear, Hyperlog
- Parallel gate evaluation via Rayon
- Zero-copy `MatrixView` for FFI integration
- C-ABI layer for Python/R bindings (`flow-gate-ffi`)
- Compliance runner binary against ISAC official test corpus
- Topological gate ordering with cycle detection
- Compensation (spectrum matrix) and ratio transformation support
