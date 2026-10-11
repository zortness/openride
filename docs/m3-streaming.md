# Bounded road/terrain streaming preview

## Background loading and bounded prefetch

The user visually confirmed the initial streaming example. The loader now reads, hashes and parses chunks on a worker thread. Scene nodes and GPU meshes remain on the main thread, with at most one chunk built per frame. It prefetches the neighborhood 100 m ahead along the route, prioritizing currently required cells.

Budgets: at most nine live mesh chunks, three decoded/cache entries, and one in-flight read. Speculative entries are evicted before required work; results for obsolete scrub targets are discarded. These count limits supplement the existing per-file and geometry limits, not an exact RAM budget. Playback waits when the current neighborhood is incomplete, displays a loading state, and resumes when ready. Scrubbing may briefly show incomplete terrain while loading. Closing joins the worker; automated error exits release meshes before quitting.

Rendered validation after the change: peak nine live chunks, three cached entries, 48 loads/39 unloads (including rapid scrub tests), 27 prefetch hits, one obsolete result discarded. Maximum neighborhood bookkeeping/unload time was 0.644 ms; maximum single-chunk main-thread mesh construction was 4.046 ms. These are component timings on the same development GPU, not comparable full-frame percentiles or a benchmark acceptance result. The corruption case still stops playback and exits 1, with clean render-resource cleanup after the error.

```powershell
./tools/preview-streaming.ps1
```

The default is `local/content/rpv-stream-preview.orpack`. Play/Pause uses a simulated speed (25 km/h initially). Scrub distance using the bottom slider; drag to orbit and scroll to zoom. The camera follows the route. The status line reports live chunks. This separate runtime-preview scene omits authoring overlays and has no trainer connection.

## Build

```powershell
python tools/chunk-scenery.py export/rpv-m3-road-route/preparation.json local/content/rpv-stream-preview.orpack
```

Output must be a new directory. This experimental format is `openride.stream-preview` schema 1 and uses the dedicated launcher; it is not yet supported by the authoring Library browser or exported rider executable.

The builder partitions terrain triangles, generated road triangles, and OSM context lines into 256 m X/Z cells. Each primitive belongs to its centroid cell, preserving all primitives exactly once. Existing vertices and positions remain unchanged. Primitive edges are limited to 32 m, bounding overhang beyond the owning cell. Terrain shading normals are generated per chunk, so shading seams remain possible.

The reader keeps a 3×3 cell neighborhood around the rider: at most nine live chunks. On a cell transition it frees departing nodes/meshes before reading new chunks. JSON geometry is discarded after conversion to meshes; there is no unbounded mesh cache. The complete generated route point list and chunk index stay resident for distance/grade lookup. Camera zoom is limited to suit the small neighborhood; distant terrain/LOD is not yet supplied.

Each chunk is bounded to 2 MiB, 30,000 vertices and 90,000 indices per mesh, and 30,000 line vertices. Manifest/route limits are 1 MiB/2 MiB, with at most 4,096 indexed cells. Fixed derived filenames prevent path traversal. Startup checks indexed files and lengths; chunks are SHA-256 checked as they are loaded. Missing, corrupt, oversized or invalid data stops playback and reports an error. The builder writes the manifest last. Checksums verify consistency, not publisher authenticity. Source attribution is carried in the manifest; the current scene targets the USGS/OSM pipeline and displays those credits.

## RPV evidence

- 90 chunk files; total package 6,706,689 bytes.
- All 130,050 terrain triangles and 832 road triangles retained.
- Resident route JSON: 79,027 bytes / 417 points, plus the full chunk index.
- Automated traversal sampled every 25 m through the full 1,972.489 m route: peak 9 live chunks, 39 loads and 30 unloads.
- Peak summed serialized payload size for live chunks: 860,538 bytes. This is **not** process RAM or GPU usage; decoded arrays/meshes and rendering overhead consume more.
- Rendered validation on Godot 4.6.2 Compatibility / AMD Radeon 8060S reported maximum synchronous neighborhood update of 33.493 ms. This includes initial loading and is not a frame-time distribution, acceptance benchmark or 16 GB / 8 GB reference-machine result.
- Corrupting a needed chunk without changing its length produced a checksum failure and exit code 1.
- 30 Python tests passed; chunk tests verify boundary partitioning, primitive counts, checksums, source preservation, exclusive outputs and overhang rejection. Rendered and headless full-route traversal checks passed.

## Next

The optional [riding presentation](m3-riding-view.md) now shares this loader; use `tools/preview-riding.ps1` for the rider-height view.

Measure full frame-time distributions, upload/load stalls and actual RAM/GPU residency during sustained playback. Add distant terrain/LOD, library and exported-client integration, and a named reference-machine benchmark. Background loading/prefetch is implemented as described above; the 33.5 ms synchronous result is retained as historical evidence. Terrain-derived route elevation remains deferred.
