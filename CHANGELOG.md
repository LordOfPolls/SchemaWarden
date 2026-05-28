# Changelog

All notable changes to Schema Warden are documented here.

---
## v1.1.0 — 2026-05-28

### Bug Fixes
- Ensure full file included for diff render

- Correctly parse optional port for server host


### Features
- Add logging

- Improve drift summary output and diff generation


### Refactoring
- Replace sqlparser with custom SQL normalizer

---## v1.0.0 — 2026-05-27

### Bug Fixes
- Dont diff baseline against itself

- Add safer path for parser failure

- Avoid diffing auto-generated pks

- Exclude db passes mutliple times, not csv

- Sqlparser discarding extra statements


### Documentation
- Add WIP notice

- Document cmd args

- Update readme

- Highlight this is for MSSQL


### Features
- Init

- Basic connection

- Fetch defs

- Diff

- Dynamically get tenant DBs

- Use command line args

- Normalise module def with parser

- Exit with non-zero code on schema drift

- Add option to exclude databases

- Add arg to only scan one object

- Handle for comment blocks in sql fallback

- Add concurrent tennant db scanning

- Support scanning databases across multiple SQL Server hosts

- Add support for outputting json

- Generate diff files for modules

- Support dotenv file for cli args

- Force readonly connection

---
