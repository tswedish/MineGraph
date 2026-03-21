Below is a **system-level technical specification** for the full RamseyNet platform, including:

* Core graph primitive
* GemForge subsystem
* Record Library (mining + event stream)
* Museum
* Duel System
* WASM/WASI verifier architecture
* Agent ecosystem boundary
* Determinism + security model

This is written as an implementable engineering spec.

---

# RAMSEYNET

## Full System Architecture Specification

Version 0.1 (Foundational Design)

---

# 0. Guiding Principles

1. **Single primitive**: the 2-color complete graph.
2. **Deterministic artifacts**: same graph → same gem.
3. **Append-only truth**: record library uses signed event stream.
4. **Client-heavy compute**: WASM verification minimizes server load.
5. **Separation of concerns**:

   * Verifier = canonical, deterministic
   * Solver/agent = competitive, optional, untrusted
6. **Composable systems**: Mining, Museum, Duel operate on same graph object.

---

# 1. Core Primitive

## 1.1 ChromaticGraph

A complete graph (K_n) with 3 possible edge states:

```
enum EdgeColor {
    Uncolored = 0,
    Black = 1,
    White = 2
}
```

### Storage format

Edges stored upper-triangular:

```
struct ChromaticGraph {
    n: u16,
    edges: Vec<u8> // 2 bits per edge packed
}
```

Edge index mapping:

```
index(i, j) where i < j:
    idx = i*n - (i*(i+1))/2 + (j - i - 1)
```

---

# 2. Canonicalization and Identity

## 2.1 Canonical Labeling

Each completed graph must be canonicalized before identity hash:

```
canonical_graph = canon_label(original_graph)
```

Canonical labeling must be:

* deterministic
* versioned
* stable across platforms

If full automorphism support is not available in v1:

* use WL color refinement + deterministic tie-breaking
* mark canonicalization version in receipt

---

## 2.2 Graph Hash

```
graph_hash = BLAKE3(canonical_edge_bytes || version_id)
```

Hash is the primary identity across all subsystems.

---

# 3. GemForge Subsystem

## Purpose

Transform a ChromaticGraph into:

* Geometry (mesh)
* Material parameters
* Shader configuration
* GemProfile metadata

---

## 3.1 GemProfile

```
struct GemProfile {
    graph_hash: Hash,
    n: u16,
    density: f32,
    degree_stats: DegreeStats,
    wl_partitions: Vec<Cell>,
    spectral_summary: SpectralSummary,
    motif_stats: MotifStats,
    symmetry_score: f32,
    rarity_score: f32,
    hardness_score: f32,
    gem_attributes: GemAttributes
}
```

---

## 3.2 Geometry Generation Pipeline

### Stage A: Analysis

* Degree distribution
* Triangle count
* k-clique detection up to Kmax (small, configurable)
* WL partitioning
* Spectral embedding (Laplacian eigenvectors)
* Motif stress map

### Stage B: Embedding

Use:

* Spectral embedding → 3D coordinates
* Normalize to sphere
* Apply orbit shell scaling

### Stage C: Mesh Construction

* Convex hull → base polyhedron
* Facet refinement based on WL cells
* Ridge graph overlay from high-stress edges
* Optional internal lattice mesh

### Stage D: Material Mapping

Map invariants to:

* IOR
* Roughness
* Dispersion strength
* Absorption tint
* Inclusion density
* Engraved sigil (hash-based)

---

## 3.3 Output

```
struct GemArtifact {
    profile: GemProfile,
    surface_mesh: Mesh,
    ridge_mesh: Option<Mesh>,
    internal_mesh: Option<Mesh>,
    material: GemMaterial,
    shader_bundle: ShaderBundle
}
```

Deterministic across platforms.

---

# 4. Record Library (Mining System)

## Purpose

Immutable ledger of verified gems and discoveries.

---

## 4.1 Record Object

```
struct GemRecord {
    graph_hash: Hash,
    gem_profile_hash: Hash,
    submission: ClaimInfo,
    verification: VerificationReceipt,
    leaderboard_tags: Vec<Tag>,
    created_at: Timestamp
}
```

---

## 4.2 Claim Workflow

1. User submits graph + receipt
2. Server validates receipt
3. Deduplicate by graph_hash
4. If new:

   * Append event to log
   * Publish record

---

## 4.3 Event Stream

Append-only log:

```
enum Event {
    ClaimSubmitted,
    ClaimVerified,
    RecordPublished,
    RecordRevoked,
    DuelMatchCompleted
}
```

Each event includes:

* previous_event_hash
* server_signature
* timestamp

Creates tamper-evident chain.

---

## 4.4 Leaderboards

Computed views:

* Max n per (s,t)
* Rarity score ranking
* Symmetry ranking
* Hardness ranking
* Duel gem rankings

Not authoritative — derived from records.

---

# 5. Gem Museum

## Purpose

Universal graph-to-gem explorer.

---

## 5.1 Capabilities

* Upload graph
* Generate gem preview (client WASM)
* Inspect invariants
* Compare similarity
* Browse curated collections
* View spectral fingerprint

---

## 5.2 Similarity Search

Embedding vector:

```
[degree_entropy,
 spectral_gap,
 wl_entropy,
 density,
 triangle_ratio,
 clique_profile_small]
```

Nearest-neighbor search.

---

# 6. Duel System

## 6.1 Match Object

```
struct Match {
    match_id: UUID,
    n: u16,
    ruleset: RulesetID,
    state: ChromaticGraph,
    move_log: Vec<Move>,
    status: MatchStatus
}
```

---

## 6.2 Rulesets

### Threshold Ladder (Primary)

* Ignore triangles
* Track max clique size ≥ K0
* Tie-break lexicographically by clique counts
* Deterministic end when board full

### First-Threshold Loss

* First to form K_K loses

### Asymmetric Ramsey

* Black avoids K_s
* White avoids K_t

---

## 6.3 Gem Creation

At match end:

```
final_graph → GemForge → MatchGem
```

Winner owns MatchGem.

Optional promotion to Record Library.

---

# 7. WASM/WASI Verifier Architecture

## 7.1 Goals

* Offload compute to clients
* Deterministic receipts
* Reduce server cost

---

## 7.2 Verifier Core

Shared Rust crate:

```
ramseynet-core
```

Includes:

* Graph structure
* Clique detection (bitset)
* Complement operations
* WL partitioning
* Receipt generator

---

## 7.3 WASM Browser Mode

* WebWorker execution
* Deterministic integer math
* Progress reporting
* Resource caps

---

## 7.4 WASI CLI Mode

* Headless verification
* Tournament pods
* Identical receipt output

---

## 7.5 Verification Receipt

```
struct VerificationReceipt {
    graph_hash: Hash,
    ruleset_id: String,
    result: PassFail,
    witness: Option<Witness>,
    metrics: MetricSummary,
    verifier_version: String,
    resource_limits: Limits,
    client_pubkey: PublicKey,
    signature: Signature
}
```

Server trusts only approved verifier builds.

---

# 8. Agent / Solver Ecosystem

## Separation of Roles

### Verifier (trusted)

* Standardized
* Versioned
* Deterministic

### Solver/Agent (untrusted)

* Any implementation
* Can be shared externally
* Only outputs graphs

Optional metadata:

```
struct SolverMetadata {
    solver_id: String,
    build_hash: Hash,
    seed: u64,
    params: Map<String,String>
}
```

Attached to claims but not trusted.

---

# 9. Security Model

* All records immutable
* All receipts signed
* Server only re-verifies rare/high-value claims
* Rate limits per identity
* Canonical deduplication
* Versioned verifier builds

---

# 10. Deployment Model

## Services

* API server (claims + records)
* Static asset server (museum)
* WebSocket server (duels)
* Optional verification pods

## Client

* Web app
* WASM verifier
* 3D viewer (WebGL/OpenGL)
* Agent integration (optional)

---

# 11. Data Flow Summary

Mining:

```
Solver → Graph → WASM Verify → Receipt → Server → Record → Leaderboard
```

Museum:

```
Graph → WASM Analyze → GemForge → Viewer
```

Duel:

```
Match → Moves → Final Graph → GemForge → Winner Collection
```

---

# 12. MVP Scope

Phase 1:

* Graph primitive
* Basic clique verifier
* Threshold Ladder duel
* Basic GemForge mesh (convex hull + color)
* Receipt generation
* Claim submission + dedup

Phase 2:

* Full WL partitioning
* Spectral embedding
* Leaderboards
* Museum search
* WASI CLI

Phase 3:

* Advanced gem shading
* Agent tournaments
* Similarity indexing
* Construction lineage graph

---

# 13. Long-Term Vision

RamseyNet becomes:

* A combinatorial mining network
* A persistent gem archive
* A competitive arena
* A reproducible research platform
* A generative art system grounded in real mathematics

All unified by one primitive:

The 2-color complete graph.

---

If you want, next I can:

* Write the exact Rust module layout,
* Define canonicalization interface,
* Or formalize duel rules in a mathematically precise way suitable for implementation and AI training.
