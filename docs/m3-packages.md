# M3 preview packages and local discovery

This first package slice loads a bounded section in the standalone scenery preview. It does not yet stream chunks, install into the exported rider, or establish performance acceptance. The format is explicitly `openride.preview-package` rather than the final world/route distribution format.

## Try the RPV package

```powershell
./tools/preview-scenery.ps1 -Package local/content/rpv-preview.orpack -ContentDirectory local/content
```

**Library** shows immediate child package folders. Choose a folder, select a package, and click **Open selected**. Copy another package directory into the chosen content folder and use **Refresh**; no app rebuild is required. **Choose folder** changes the library for the current session. The default is `user://content` under the scenery preview's Godot user-data directory. The library dialog displays the actual path. No GIS tools or network calls are needed to load a package.

The generated RPV package is a roughly 7.1 MB self-contained preview directory. It uses the earlier manual repair proposal (53 retained points), not the later detector proposal, because no user-selected detector repair file has been supplied. Its metadata remains `authoring-preview-unvalidated`, `trainer_ready=false`; packaging does not approve a repair or enable hardware control.

## Build

```powershell
python tools/package-scenery.py export/rpv-m3-repaired/preparation.json local/content/rpv-preview.orpack --id rpv-preview --version 0.1.0
```

The parent directory must exist and output directory must be new. Original preparation and correction files remain untouched. The directory contains:

- `manifest.json`: package identity/content version, required reader capability, explicit empty dependencies, bounds/origin, source preparation/route hashes, selected profile fingerprint, review status, and payload sizes/hashes.
- `preparation.json`: embedded preview terrain, road geometry, source and corrected/repaired profiles, source snapshot metadata. Authoring-side file references are removed; no OBJ or acquisition tools are needed at runtime.
- `attribution.json`: source attribution and separation of application and geographic-content licenses.

The builder writes the manifest last. Discovery requires both listed payloads and verifies lengths/checksums; incomplete copies cannot be opened. Only the two fixed payload names are accepted. The reader rejects unknown schema/capabilities, external dependencies, duplicate/unsupported paths, missing attribution, and payload tampering. It revalidates on selection. Limits: 64 KiB manifest/attribution, 16 MiB preparation, followed by the existing viewer geometry validation. Packages contain JSON data only; no code, PCK, scripts or dynamic plugins are loaded. Checksums establish consistency, not publisher authenticity.

Private route locations and activity hashes remain in this local authoring package. This is not a prepared public distribution release or a completed content-license audit.

## Verification and remaining work

Python package tests cover deterministic output, copied payload checksums, source preservation and refusal to overwrite. Godot checks cover a valid package plus unsupported versions/capabilities/dependencies, unsafe paths, trainer-ready rejection, corrupt payload and incomplete installation. A copy into a fresh library was discovered and opened through the library selection handler, and the library UI/rendered RPV package were inspected.

Next: split runtime geometry into spatial chunks, implement bounded loading during simulated traversal, then integrate library discovery in the exported client and measure frame times/memory/load stalls. Buildings and optional OSM2World placement also remain open. The current preview retains the entire two-kilometre preparation and all its diagnostics in memory.
