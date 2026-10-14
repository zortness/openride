# Native authoring tooling and agent workflows

Recorded 2026-10-11 during initial repository publication preparation.
The authoring migration is now approved for M7 (2026-10-13), delivered incrementally
through [S08–S23](m7-authoring-repair-tasks.md). The historical recommendations
below remain useful migration guidance. S08 introduces `openride-author` for
project lifecycle; later slices port each production authoring stage behind it.

## Native authoring direction

The Python scripts are functioning product authoring code, not disposable tests.
The Rust workspace already handles activity import/export, terrain preparation,
trainer control and ride simulation. Keep Python tests and the reference pipeline
while moving production authoring capabilities into native Rust incrementally.

Recommended order:

1. Inventory the request/job/result contracts between the Godot route editor,
   Python workflow and native content builder. Preserve project schemas, source
   provenance, hashes, errors and resumability through the transition.
2. Move workflow orchestration and source acquisition into a native authoring
   executable, retaining retries, timeouts, cache validation and atomic writes.
3. Port road matching/corrections, repair/profile generation, geometry/chunk
   generation and imagery assembly in bounded slices with synthetic parity cases.
4. Switch one editor stage at a time after old/new outputs satisfy format and
   semantic checks. Account for floating-point differences instead of demanding
   identical bytes where those do not affect the contract.
5. Remove the end-user Python requirement only after the whole authoring flow
   passes on a clean Windows machine. Keep Python as optional developer tooling
   where useful. Validate each later target platform independently.

Do not postpone saving the current working baseline for this migration. A bundled
Python runtime could simplify installation temporarily but is not a native port.
No compiler/packager or new dependency was selected in this audit.

## Agent guidance

AGENTS.md captures short, always-applicable project rules now. Recommended future
repository-local skills should describe repeatable procedures with explicit inputs
and outputs, rather than duplicate long progress documents:

- **Activity intake:** read-only format inventory and complementary-source grouping;
  timestamps/units/gaps/elevation checks; concise findings and route-section proposal;
  private outputs under local/. No automatic road correction or trainer commands.
- **Build and verify:** discover toolchains, sync generated Godot sources, locked
  native build, isolated userdata, headless plus exported-app checks and packaging;
  report exact verification evidence and remaining physical checks.
- **Hardware capture triage:** distinguish capabilities, measurements and control
  acknowledgments; identify dropouts/source identities; sanitize replay fixtures;
  keep hardware testing explicitly separate from fixture-only results.

Build/verify is the first skill to extract; activity intake is next. The hardware
skill should follow M6's stable diagnostic-bundle format. Skill creation is proposed,
not performed here. Keep executable checks in the repository so skills invoke the
same checks contributors use, and avoid personal absolute paths in skill text.
